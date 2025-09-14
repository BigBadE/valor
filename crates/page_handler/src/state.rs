use crate::url::stream_url;
use anyhow::{anyhow, Error};
use js::{DOMMirror, DOMSubscriber, DOMUpdate, JsEngine, DomIndex};
use html::dom::DOM;
use html::parser::HTMLParser;
use log::{trace, info};
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use tokio::sync::mpsc::error::TryRecvError as UnboundedTryRecvError;
use js_engine_v8::V8Engine;
use url::Url;
use css::CSSMirror;
use css::types::Stylesheet;
use layouter::Layouter;
use style_engine::StyleEngine;
use wgpu_renderer::{Renderer, DrawRect, DrawText, DisplayList};
use std::sync::Arc;
use tracing::info_span;
use crate::scheduler::FrameScheduler;
use crate::config::ValorConfig;
use crate::{selection, focus as focus_mod, telemetry as telemetry_mod};
use crate::runtime::JsRuntime;

/// Note: FrameScheduler has moved to `scheduler.rs`.
/// Structured outcome of a single update() tick. Extend as needed.
pub struct UpdateOutcome {
    pub needs_redraw: bool,
}

pub struct HtmlPage {
    /// Optional currently focused node for focus management.
    focused_node: Option<js::NodeKey>,
    /// Optional active selection rectangle in viewport coordinates for highlight overlay (text selection highlight).
    selection_overlay: Option<(i32, i32, i32, i32)>,
    // If none, loading is finished. If some, still streaming.
    loader: Option<HTMLParser>,
    // The DOM of the page.
    dom: DOM,
    // Mirror that collects CSS from the DOM stream.
    css_mirror: DOMMirror<CSSMirror>,
    // StyleEngine mirror that will compute styles (skeleton for now).
    style_engine_mirror: DOMMirror<StyleEngine>,
    // Layouter mirror that maintains a layout tree from DOM updates.
    layouter_mirror: DOMMirror<Layouter>,
    // Renderer mirror that maintains a scene graph from DOM updates.
    renderer_mirror: DOMMirror<Renderer>,
    // DOM index mirror for JS document.getElement* queries.
    dom_index_mirror: DOMMirror<DomIndex>,
    /// Shared state for DOM index to support synchronous lookups (e.g., getElementById).
    dom_index_shared: js::SharedDomIndex,
    // For sending updates to the DOM
    in_updater: mpsc::Sender<Vec<DOMUpdate>>,
    // JavaScript engine and script queue
    js_engine: V8Engine,
    /// Host context for privileged binding decisions and shared registries.
    host_context: js::HostContext,
    script_rx: UnboundedReceiver<html::parser::ScriptJob>,
    script_counter: u64,
    #[allow(dead_code)]
    url: Url,
    /// ES module resolver/bundler adapter (JS crate) for side-effect modules.
    module_resolver: Box<dyn js::ModuleResolver>,
    /// Display builder used to construct display lists from layout and styles.
    display_builder: Box<dyn crate::display::DisplayBuilder>,
    // Frame scheduler to coalesce layout per frame with a budget (Phase 5)
    frame_scheduler: FrameScheduler,
    /// Whether to draw a small perf HUD overlay in display list snapshots.
    hud_enabled: bool,
    /// Diagnostics: number of nodes restyled in the last tick.
    last_style_restyled_nodes: u64,
    // Whether we've dispatched DOMContentLoaded to JS listeners.
    dom_content_loaded_fired: bool,
    /// One-time post-load guard to rebuild the StyleEngine's node inventory from the Layouter
    /// for deterministic style resolution in the normal update path.
    style_nodes_rebuilt_after_load: bool,
    /// Whether the last update produced visual changes that require a redraw.
    needs_redraw: bool,
    /// Whether to emit perf telemetry lines per tick.
    telemetry_enabled: bool,
    /// One-time guard for merging inline <style> rules into the author stylesheet after load.
    inline_styles_merged_once: bool,
}

impl HtmlPage {
    /// Create a new HtmlPage by streaming the content from the given URL
    pub async fn new(handle: &Handle, url: Url, config: ValorConfig) -> Result<Self, Error> {
        // For updates from the DOM to subcomponents
        let (out_updater, out_receiver) = broadcast::channel(128);

        // For updates from subcomponents to the DOM
        let (in_updater, in_receiver) = mpsc::channel(128);

        // Create DOM first so it can assign a producer shard for NodeKey generation
        let mut dom = DOM::new(out_updater, in_receiver);
        let keyman = dom.register_parser_manager();
                // Register a NodeKey manager shard for JS-created nodes too
                let js_keyman = dom.register_manager::<u64>();

        // Channel for inline script execution requests from the parser
        let (script_tx, script_rx) = unbounded_channel::<html::parser::ScriptJob>();
        let loader = HTMLParser::parse(
            handle,
            in_updater.clone(),
            keyman,
            stream_url(&url).await?,
            out_receiver,
            script_tx,
            url.clone(),
        );

        // Create and attach the CSS mirror to observe DOM updates
        let css_mirror = DOMMirror::new(in_updater.clone(), dom.subscribe(), CSSMirror::with_base(url.clone()));
        // Create and attach the StyleEngine mirror to observe DOM updates
        let style_engine_mirror = DOMMirror::new(in_updater.clone(), dom.subscribe(), StyleEngine::new());
        // Create and attach the Layouter mirror to observe DOM updates
        let layouter_mirror = DOMMirror::new(in_updater.clone(), dom.subscribe(), Layouter::new());
        // Create and attach the Renderer mirror to observe DOM updates
        let renderer_mirror = DOMMirror::new(in_updater.clone(), dom.subscribe(), Renderer::new());
        // Create and attach a lightweight DOM index for JS getElement* functions
        let (dom_index_sub, dom_index_shared) = js::DomIndex::new();
        let dom_index_mirror = DOMMirror::new(in_updater.clone(), dom.subscribe(), dom_index_sub);
        
        // Debouncing removed in favor of single frame budget policy

        // Create the JS engine
        let mut js_engine = V8Engine::new().map_err(|e| anyhow!("failed to init V8Engine: {}", e))?;
        // Install direct host bindings (e.g., console + document) from the js crate
        let logger = Arc::new(js::ConsoleLogger);
        // Prepare JS context wiring for DOM manipulation functions
        let js_node_keys = std::sync::Arc::new(std::sync::Mutex::new(js_keyman));
        let js_local_id_counter = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let js_created_nodes = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        // Build page origin string for same-origin checks
        let page_origin = match url.scheme() {
            "file" => String::from("file://"),
            _ => {
                let host = url.host_str().unwrap_or("");
                let port = url.port().map(|p| format!(":{}", p)).unwrap_or_default();
                format!("{}://{}{}", url.scheme(), host, port)
            }
        };
        // Establish a high-resolution time origin for performance.now and timers
        let start_instant = std::time::Instant::now();
        // Initialize per-origin storage registries (in-memory for this page/session)
        let storage_local = std::sync::Arc::new(std::sync::Mutex::new(js::bindings::StorageRegistry::default()));
        let storage_session = std::sync::Arc::new(std::sync::Mutex::new(js::bindings::StorageRegistry::default()));

        let host_context = js::HostContext {
            page_id: None,
            logger,
            dom_sender: in_updater.clone(),
            js_node_keys,
            js_local_id_counter,
            js_created_nodes,
            dom_index: dom_index_shared.clone(),
            tokio_handle: handle.clone(),
            page_origin,
            fetch_registry: std::sync::Arc::new(std::sync::Mutex::new(js::bindings::FetchRegistry::default())),
            performance_start: start_instant,
            storage_local: storage_local.clone(),
            storage_session: storage_session.clone(),
            chrome_host_tx: None,
        };
        let bindings = js::build_default_bindings();
        let _ = js_engine.install_bindings(host_context.clone(), &bindings);

        // Frame scheduler budget (ms) from ValorConfig
        let frame_scheduler = FrameScheduler::new(config.frame_budget());

        Ok(Self {
            focused_node: None,
            selection_overlay: None,
            loader: Some(loader),
            dom,
            css_mirror,
            style_engine_mirror,
            layouter_mirror,
            renderer_mirror,
            dom_index_mirror,
            dom_index_shared,
            in_updater,
            js_engine,
            host_context,
            script_rx,
            script_counter: 0,
            url: url.clone(),
            module_resolver: Box::new(js::SimpleFileModuleResolver::new()),
            display_builder: Box::new(crate::display::DefaultDisplayBuilder),
            frame_scheduler,
            last_style_restyled_nodes: 0,
            dom_content_loaded_fired: false,
            style_nodes_rebuilt_after_load: false,
            needs_redraw: false,
            hud_enabled: config.hud_enabled,
            telemetry_enabled: config.telemetry_enabled,
            inline_styles_merged_once: false,
        })
    }

    /// Returns true once parsing has fully finalized and the loader has been consumed.
    /// This becomes true only after an update() call has observed the parser finished
    /// and awaited its completion.
    pub fn parsing_finished(&self) -> bool {
        self.loader.is_none()
    }

    /// Execute any pending inline scripts from the parser
    pub(crate) fn execute_pending_scripts(&mut self) {
        loop {
            match self.script_rx.try_recv() {
                Ok(job) => {
                    let script_url = if job.url.is_empty() {
                        let kind = match job.kind { html::parser::ScriptKind::Module => "module", html::parser::ScriptKind::Classic => "script" };
                        let u = format!("inline:{}-{}", kind, self.script_counter);
                        self.script_counter = self.script_counter.wrapping_add(1);
                        u
                    } else { job.url.clone() };
                    info!("HtmlPage: executing {} (length={} bytes)", script_url, job.source.len());
                    match job.kind {
                        html::parser::ScriptKind::Classic => {
                            // If this is an external classic script, fetch its source first.
                            let code: String = if job.source.is_empty() && !script_url.starts_with("inline:") {
                                if let Ok(url) = url::Url::parse(&script_url) {
                                    match url.scheme() {
                                        "valor" => {
                                            // Embedded chrome asset
                                            let path = url.path();
                                            if let Some(bytes) = crate::embedded_chrome::get_embedded_chrome_asset(path)
                                                .or_else(|| crate::embedded_chrome::get_embedded_chrome_asset(&format!("valor://chrome{}", path)))
                                            {
                                                String::from_utf8_lossy(bytes).into_owned()
                                            } else { String::new() }
                                        }
                                        _ => {
                                            // Fetch via stream_url and concatenate
                                            let fut = async {
                                                let mut s = Vec::new();
                                                let mut stream = stream_url(&url).await?;
                                                use tokio_stream::StreamExt;
                                                while let Some(chunk) = stream.next().await {
                                                    let b = chunk.map_err(|e| anyhow::anyhow!("{}", e))?;
                                                    s.extend_from_slice(&b);
                                                }
                                                Ok::<String, anyhow::Error>(String::from_utf8_lossy(&s).into_owned())
                                            };
                                            self.host_context.tokio_handle.block_on(fut).unwrap_or_default()
                                        }
                                    }
                                } else { String::new() }
                            } else {
                                job.source.clone()
                            };
                            let _ = self.js_engine.eval_script(&code, &script_url);
                            let _ = self.js_engine.run_jobs();
                        }
                        html::parser::ScriptKind::Module => {
                            // Bundle static imports (side-effect only) and evaluate via module API.
                            let resolver = &mut self.module_resolver;
                            let inline_source = if script_url.starts_with("inline:") { Some(job.source.as_str()) } else { None };
                            if let Ok(bundle) = resolver.bundle_root(script_url.as_str(), &self.url, inline_source) {
                                let _ = self.js_engine.eval_module(&bundle, &script_url);
                                let _ = self.js_engine.run_jobs();
                            } else {
                                // Fallback: evaluate raw source
                                let _ = self.js_engine.eval_module(&job.source, &script_url);
                                let _ = self.js_engine.run_jobs();
                            }
                        }
                    }
                }
                Err(UnboundedTryRecvError::Empty) => break,
                Err(UnboundedTryRecvError::Disconnected) => break,
            }
        }
    }

    /// Execute at most one due JavaScript timer callback.
    /// This evaluates the runtime prelude hook `__valorTickTimersOnce(nowMs)` and then
    /// flushes engine microtasks to approximate browser ordering (microtasks after each task).
    pub(crate) fn tick_js_timers_once(&mut self) {
        // Use engine-provided clock (Date.now()/performance.now) by omitting the argument.
        // This keeps the runtime timer origin consistent with scheduling inside JS.
        let script = String::from(
            "(function(){ try { var f = globalThis.__valorTickTimersOnce; if (typeof f === 'function') f(); } catch(_){} })();"
        );
        let _ = self.js_engine.eval_script(&script, "valor://timers_tick");
        let _ = self.js_engine.run_jobs();
    }

    /// Synchronously fetch the textContent for an element by id using the DomIndex mirror.
    /// This helper keeps the index in sync for tests that query immediately after updates.
    pub fn text_content_by_id_sync(&mut self, id: &str) -> Option<String> {
        // Keep index mirror fresh for same-tick queries
        let _ = self.dom_index_mirror.try_update_sync();
        if let Ok(guard) = self.dom_index_shared.lock()
            && let Some(key) = guard.get_element_by_id(id)
        { return Some(guard.get_text_content(key)); }
        None
    }

    /// Finalize DOM loading if the loader has finished
    async fn finalize_dom_loading_if_needed(&mut self) -> Result<(), Error> {
        if let Some(true) = self.loader.as_ref().map(|loader| loader.is_finished()) {
            let loader = self
                .loader
                .take()
                .ok_or_else(|| anyhow!("Loader is finished and None!"))?;
            trace!("Loader finished, finalizing DOM");
            loader.finish().await?;
        }
        Ok(())
    }

    /// Handle DOM content loaded event if parsing is finished and not yet fired
    pub(crate) async fn handle_dom_content_loaded_if_needed(&mut self) -> Result<(), Error> {
        if self.loader.is_none() && !self.dom_content_loaded_fired {
            info!("HtmlPage: dispatching DOMContentLoaded");
            let _ = self
                .js_engine
                .eval_script(
                    "(function(){try{var d=globalThis.document; if(d&&typeof d.__valorDispatchDOMContentLoaded==='function'){ d.__valorDispatchDOMContentLoaded(); }}catch(_){}})();",
                    "valor://dom_events",
                );
            let _ = self.js_engine.run_jobs();
            self.dom_content_loaded_fired = true;
            // After DOMContentLoaded, DOM listener mutations will be applied on the next regular tick.
            // Keep the DOM index mirror in sync in a non-blocking manner for tests.
            self.dom_index_mirror.try_update_sync()?;
        }
        Ok(())
    }

    /// Process CSS and style updates, returning whether styles have changed
    async fn process_css_and_styles(&mut self) -> Result<bool, Error> {
        let _span = info_span!("page.process_css_and_styles").entered();
        // Keep Layouter mirror fresh; we will use its snapshot to optionally rebuild the StyleEngine node set.
        // Avoid draining other mirrors here to prevent starvation while the parser is streaming.
        let _ = self.layouter_mirror.try_update_sync();
        // Synchronize attributes as a safety net so id/class are available even if structure rebuild is skipped
        let lay_attrs = self.layouter_mirror.mirror_mut().attrs_map();
        trace!("process_css_and_styles: layouter_attrs_count={} nodes", lay_attrs.len());
        self.style_engine_mirror.mirror_mut().sync_attrs_from_map(&lay_attrs);

        // One-time deterministic rebuild of StyleEngine's node inventory after parsing is finished.
        // This ensures regular update path has a complete node set for reliable selector matching (e.g., overflow hidden).
        if self.loader.is_none() && !self.style_nodes_rebuilt_after_load {
            trace!("process_css_and_styles: rebuilding node inventory");
            // Build element-only tag and children maps from the layouter snapshot
            let lay_snapshot = self.layouter_mirror.mirror_mut().snapshot();
            let mut tags_by_key: std::collections::HashMap<js::NodeKey, String> = std::collections::HashMap::new();
            let mut raw_children: std::collections::HashMap<js::NodeKey, Vec<js::NodeKey>> = std::collections::HashMap::new();
            for (key, kind, children) in lay_snapshot.into_iter() {
                if let layouter::LayoutNodeKind::Block { tag } = kind {
                    tags_by_key.insert(key, tag);
                }
                raw_children.insert(key, children);
            }
            let mut children_by_key: std::collections::HashMap<js::NodeKey, Vec<js::NodeKey>> = std::collections::HashMap::new();
            for (parent, kids) in raw_children.into_iter() {
                // Keep only element children
                let filtered: Vec<js::NodeKey> = kids.into_iter().filter(|c| tags_by_key.contains_key(c)).collect();
                // Allow ROOT to host element children even though it has no tag
                if tags_by_key.contains_key(&parent) || parent == js::NodeKey::ROOT {
                    children_by_key.insert(parent, filtered);
                }
            }
            // Deterministically rebuild the StyleEngine's node inventory
            self.style_engine_mirror
                .mirror_mut()
                .rebuild_from_layout_snapshot(&tags_by_key, &children_by_key, &lay_attrs);
            self.style_nodes_rebuilt_after_load = true;
            trace!("process_css_and_styles: node inventory rebuilt");
        }

        // Build an AUTHOR stylesheet combining CSSMirror rules (and inline <style> rules only once post-load).
        // NOTE: Do NOT merge UA rules here; StyleEngine merges UA internally when replacing the author sheet.
        trace!("process_css_and_styles: author_styles.rules={} (pre-merge)", self.css_mirror.mirror_mut().styles().rules.len());
        let mut author_styles = self.css_mirror.mirror_mut().styles().clone();
        if self.loader.is_none() && !self.inline_styles_merged_once {
            // 2) Collect inline <style> text from the Layouter snapshot (one-time after load)
            let mut text_by_key: std::collections::HashMap<js::NodeKey, String> = std::collections::HashMap::new();
            let mut raw_children2: std::collections::HashMap<js::NodeKey, Vec<js::NodeKey>> = std::collections::HashMap::new();
            let mut tags_by_key2: std::collections::HashMap<js::NodeKey, String> = std::collections::HashMap::new();

            let lay_snapshot2 = self.layouter_mirror.mirror_mut().snapshot();
            for (key, kind, children) in lay_snapshot2.into_iter() {
                match kind {
                    layouter::LayoutNodeKind::InlineText { text } => { text_by_key.insert(key, text); },
                    layouter::LayoutNodeKind::Block { tag } => { tags_by_key2.insert(key, tag); },
                    _ => {}
                }
                raw_children2.insert(key, children);
            }
            let mut inline_style_css = String::new();
            for (node, tag) in &tags_by_key2 {
                if tag.eq_ignore_ascii_case("style")
                    && let Some(children) = raw_children2.get(node)
                {
                    for child in children { if let Some(txt) = text_by_key.get(child) { inline_style_css.push_str(txt); } }
                }
            }

            // 3) Append parsed author (inline <style>) to existing author stylesheet
            let inline_style_css_trimmed = inline_style_css.trim().to_string();
            if !inline_style_css_trimmed.is_empty() {
                // Compute source order offset as current author rule count, so new rules append correctly
                let author_count = author_styles.rules.len() as u32;
                // Use the streaming parser to robustly parse even if content is large or has trailing garbage
                let mut stream = css::parser::StylesheetStreamParser::new(css::types::Origin::Author, author_count);
                let mut acc = css::types::Stylesheet::default();
                stream.push_chunk(&inline_style_css_trimmed, &mut acc);
                let (tail, _next) = stream.finish_with_next();
                acc.rules.extend(tail.rules);
                author_styles.rules.extend(acc.rules);
                self.inline_styles_merged_once = true;
            }
        }
        trace!("process_css_and_styles: author_styles.rules={} (post-merge)", author_styles.rules.len());
        self.style_engine_mirror.mirror_mut().replace_stylesheet(author_styles.clone());
        // Coalesce and recompute dirty styles once per tick after draining updates and merging rules
        self.style_engine_mirror.mirror_mut().recompute_dirty();

        // Always forward the latest computed styles and stylesheet snapshot to the layouter.
        let computed_styles = self.style_engine_mirror.mirror_mut().computed_snapshot();
        // Forward AUTHOR stylesheet to layouter; UA defaults are applied by StyleEngine during computation.
        self.layouter_mirror.mirror_mut().set_stylesheet(author_styles);
        self.layouter_mirror.mirror_mut().set_computed_styles(computed_styles);

        // Mark dirty nodes for reflow only when styles actually changed.
        let style_changed = self.style_engine_mirror.mirror_mut().take_and_clear_style_changed();
        let mut changed_nodes_len: usize = 0;
        if style_changed {
            let changed_nodes = self.style_engine_mirror.mirror_mut().take_changed_nodes();
            changed_nodes_len = changed_nodes.len();
            if changed_nodes.is_empty() {
                // Conservative fallback when change set is not tracked
                self.layouter_mirror.mirror_mut().mark_nodes_style_dirty(&[js::NodeKey::ROOT]);
            } else {
                self.layouter_mirror.mirror_mut().mark_nodes_style_dirty(&changed_nodes);
            }
        }

        // Record restyled node count for diagnostics (Phase 8)
        self.last_style_restyled_nodes = if style_changed { changed_nodes_len as u64 } else { 0 };

        Ok(style_changed)
    }

    /// Compute layout and forward dirty rectangles to renderer (single frame budget policy)
    fn compute_layout(&mut self, style_changed: bool) -> Result<(), Error> {
        let _span = info_span!("page.compute_layout").entered();
        // Determine if layout should run based on actual style or material layouter changes
        let has_material_dirty = self.layouter_mirror.mirror_mut().has_material_dirty();
        let should_layout = style_changed || has_material_dirty;

        if should_layout {
            // Respect frame budget: run layout at most once per frame window
            if !self.frame_scheduler.allow() {
                trace!("Layout skipped due to frame budget ({:?})", self.frame_scheduler.budget());
                // Record spillover and treat as a no-op layout tick for observability
                self.frame_scheduler.incr_deferred();
                self.layouter_mirror.mirror_mut().mark_noop_layout_tick();
                return Ok(());
            }

            let node_count = self.layouter_mirror.mirror_mut().compute_layout();
            let layouter = self.layouter_mirror.mirror_mut();
            let nodes_reflowed = layouter.perf_nodes_reflowed_last();
            let dirty_subtrees = layouter.perf_dirty_subtrees_last();
            let layout_time_ms = layouter.perf_layout_time_last_ms();
            let updates_applied = layouter.perf_updates_applied();
            info!(
                "Layout: processed={node_count}, reflowed_nodes={}, dirty_subtrees={}, time_ms={}, updates_applied_total={}",
                nodes_reflowed, dirty_subtrees, layout_time_ms, updates_applied
            );
            
            // Forward dirty rectangles to the renderer for partial redraws
            let dirty_rectangles_i32 = layouter.take_dirty_rects();
            if !dirty_rectangles_i32.is_empty() {
                let dirty_rectangles: Vec<DrawRect> = dirty_rectangles_i32
                    .into_iter()
                    .map(|rect| DrawRect { 
                        x: rect.x as f32, 
                        y: rect.y as f32, 
                        width: rect.width as f32, 
                        height: rect.height as f32, 
                        color: [0.0, 0.0, 0.0] 
                    })
                    .collect();
                self.renderer_mirror.mirror_mut().set_dirty_rects(dirty_rectangles);
                // Dirty rects imply a visible change.
                self.needs_redraw = true;
            }
        } else {
            trace!("Layout skipped: no DOM/style changes in this tick");
            // Reset last-tick perf counters so observability reflects the no-op
            self.layouter_mirror.mirror_mut().mark_noop_layout_tick();
        }
        
        Ok(())
    }

    pub async fn update(&mut self) -> Result<(), Error> {
        self.update_with_outcome().await.map(|_| ())
    }

    /// Same as update(), but returns a structured outcome for callers that care.
    pub async fn update_with_outcome(&mut self) -> Result<UpdateOutcome, Error> {
        let _span = info_span!("page.update").entered();
        // Finalize DOM loading if the loader has finished
        self.finalize_dom_loading_if_needed().await?;

        // Drive a single JS timers tick via runtime
        let mut rt = crate::runtime::DefaultJsRuntime;
        rt.tick_timers_once(self);

        // Apply any pending DOM updates
        self.dom.update().await?;
        // Keep the DOM index mirror in sync before any JS queries (e.g., getElementById)
        self.dom_index_mirror.try_update_sync()?;
        // Execute pending scripts and DOMContentLoaded sequencing via runtime after DOM drained
        rt.drive_after_dom_update(self).await?;

        // Process CSS and style updates
        let style_changed = self.process_css_and_styles().await?;

        // Compute layout and forward dirty rectangles
        self.compute_layout(style_changed)?;

        // Drain renderer mirror after DOM broadcast so the scene graph stays in sync (non-blocking)
        self.renderer_mirror.try_update_sync()?;
        // Emit optional production telemetry for this tick per config
        self.emit_perf_telemetry_if_enabled();
        let outcome = UpdateOutcome { needs_redraw: self.needs_redraw };
        Ok(outcome)
    }

    pub fn create_mirror<T: DOMSubscriber>(&self, mirror: T) -> DOMMirror<T> {
        DOMMirror::new(self.in_updater.clone(), self.dom.subscribe(), mirror)
    }

    /// Return whether a redraw is needed since the last call and clear the flag.
    pub fn take_needs_redraw(&mut self) -> bool {
        let v = self.needs_redraw;
        self.needs_redraw = false;
        v
    }

    /// Drain CSS mirror and return a snapshot clone of the collected stylesheet
    /// Evaluate arbitrary JS in the page's engine (testing helper) and flush microtasks.
    pub fn eval_js(&mut self, source: &str) -> Result<(), Error> {
        self.js_engine.eval_script(source, "valor://eval_js_test")?;
        self.js_engine.run_jobs()?;
        Ok(())
    }

    pub fn styles_snapshot(&mut self) -> Result<Stylesheet, Error> {
        // For blocking-thread callers, keep it non-async
        self.css_mirror.try_update_sync()?;
        Ok(self.css_mirror.mirror_mut().styles().clone())
    }

    /// Drain mirrors and return a snapshot clone of computed styles per node.
    /// Ensures the latest inline <style> collected by CSSMirror is forwarded to the
    /// StyleEngine before taking the snapshot so callers that query immediately after
    /// parsing completes (without another update tick) still see up-to-date styles.
    pub fn computed_styles_snapshot(&mut self) -> Result<std::collections::HashMap<js::NodeKey, style_engine::ComputedStyle>, Error> {
        // 1) Drain CSS mirror to capture any late style chunks finalized at EndOfDocument
        self.css_mirror.try_update_sync()?;
        // 2) Drain StyleEngine DOM updates first so tag/id/class are up-to-date before stylesheet merge
        self.style_engine_mirror.try_update_sync()?;
        // 3) Drain Layouter mirror to obtain a consistent snapshot for rebuilding the StyleEngine node inventory
        let _ = self.layouter_mirror.try_update_sync();
        let lay_snapshot = self.layouter_mirror.mirror_mut().snapshot();
        // Build element-only tags and children maps from the layouter snapshot
        let mut tags_by_key: std::collections::HashMap<js::NodeKey, String> = std::collections::HashMap::new();
        let mut raw_children: std::collections::HashMap<js::NodeKey, Vec<js::NodeKey>> = std::collections::HashMap::new();
        for (key, kind, children) in lay_snapshot.into_iter() {
            if let layouter::LayoutNodeKind::Block { tag } = kind {
                tags_by_key.insert(key, tag);
            }
            raw_children.insert(key, children);
        }
        let mut children_by_key: std::collections::HashMap<js::NodeKey, Vec<js::NodeKey>> = std::collections::HashMap::new();
        for (parent, kids) in raw_children.into_iter() {
            // Keep only element children
            let filtered: Vec<js::NodeKey> = kids.into_iter().filter(|c| tags_by_key.contains_key(c)).collect();
            // Allow ROOT to host element children even though it has no tag
            if tags_by_key.contains_key(&parent) || parent == js::NodeKey::ROOT {
                children_by_key.insert(parent, filtered);
            }
        }
        // Attributes from the layouter mirror
        let attrs_by_key = self.layouter_mirror.mirror_mut().attrs_map();
        // 4) Deterministically rebuild the StyleEngine's node inventory
        self.style_engine_mirror
            .mirror_mut()
            .rebuild_from_layout_snapshot(&tags_by_key, &children_by_key, &attrs_by_key);
        
        // 4.1) Supplementary: extract inline <style> text from the Layouter snapshot and parse it
        // This ensures tests see a complete author stylesheet even if streaming CSS parsing under-collected.
        let mut text_by_key: std::collections::HashMap<js::NodeKey, String> = std::collections::HashMap::new();
        let mut raw_children2: std::collections::HashMap<js::NodeKey, Vec<js::NodeKey>> = std::collections::HashMap::new();
        let lay_snapshot2 = self.layouter_mirror.mirror_mut().snapshot();
        for (key, kind, children) in lay_snapshot2.into_iter() {
            if let layouter::LayoutNodeKind::InlineText { text } = kind {
                text_by_key.insert(key, text);
            }
            raw_children2.insert(key, children);
        }
        let mut inline_style_css = String::new();
        for (node, tag) in &tags_by_key {
            if tag.eq_ignore_ascii_case("style")
                && let Some(children) = raw_children2.get(node)
            {
                for child in children {
                    if let Some(txt) = text_by_key.get(child) { inline_style_css.push_str(txt); }
                }
            }
        }
        // 4.2) Build a merged stylesheet
        let current_styles = self.css_mirror.mirror_mut().styles().clone();
        let author_count_current = current_styles
            .rules
            .iter()
            .filter(|r| matches!(r.origin, css::types::Origin::Author))
            .count();
        let inline_style_css_trimmed = inline_style_css.trim().to_string();
        let mut final_styles: css::types::Stylesheet = css::types::Stylesheet::default();
        if !inline_style_css_trimmed.is_empty() {
            let ua_count = current_styles
                .rules
                .iter()
                .filter(|r| matches!(r.origin, css::types::Origin::UA))
                .count() as u32;
            let parsed_author = css::parser::parse_stylesheet(&inline_style_css_trimmed, css::types::Origin::Author, ua_count);
            if parsed_author.rules.len() > author_count_current {
                // Prefer UA from current + parsed author from snapshot when it is more complete
                final_styles.rules.extend(
                    current_styles
                        .rules
                        .iter()
                        .filter(|r| matches!(r.origin, css::types::Origin::UA))
                        .cloned(),
                );
                final_styles.rules.extend(parsed_author.rules);
            } else {
                final_styles = current_styles;
            }
        } else {
            final_styles = current_styles;
        }
        // 4.3) Merge/replace stylesheet in the StyleEngine and force a full restyle
        self.style_engine_mirror.mirror_mut().replace_stylesheet(final_styles);
        self.style_engine_mirror.mirror_mut().force_full_restyle();
        // 5) Return a stable snapshot of computed styles
        let snapshot = self.style_engine_mirror.mirror_mut().computed_snapshot();
        Ok(snapshot)
    }

    /// Drain CSS mirror and return a snapshot clone of discovered external stylesheet URLs
    pub fn discovered_stylesheets_snapshot(&mut self) -> Result<Vec<String>, Error> {
        self.css_mirror.try_update_sync()?;
        Ok(self.css_mirror.mirror_mut().discovered_stylesheets().to_vec())
    }

    /// Return a JSON string with key performance counters from the layouter to aid diagnostics (Phase 8).
    pub fn perf_counters_snapshot_string(&mut self) -> String {
        let _ = self.layouter_mirror.try_update_sync();
        let lay = self.layouter_mirror.mirror_mut();
        telemetry_mod::perf_counters_json(
            lay.perf_nodes_reflowed_last(),
            lay.perf_updates_applied(),
            lay.perf_dirty_subtrees_last(),
            lay.perf_layout_time_last_ms(),
            lay.perf_layout_time_total_ms(),
            self.last_style_restyled_nodes,
            self.frame_scheduler.deferred(),
            lay.perf_line_boxes_last(),
            lay.perf_shaped_runs_last(),
            lay.perf_early_outs_last(),
        )
    }

    /// Emit production-friendly telemetry (JSON) when enabled in ValorConfig.
    /// This prints a single-line JSON record per tick with core Phase 8 counters.
    /// Intended for external tooling to scrape logs; kept opt-in to avoid overhead.
    pub fn emit_perf_telemetry_if_enabled(&mut self) {
        telemetry_mod::maybe_emit(self.telemetry_enabled, &self.perf_counters_snapshot_string());
    }

    /// Return a JSON snapshot of the current DOM tree (deterministic schema for comparison)
    pub fn dom_json_snapshot_string(&self) -> String {
        self.dom.to_json_string()
    }

    /// Return the NodeKey of an element by id using the DOM index, if present.
    pub fn get_element_by_id(&mut self, id: &str) -> Option<js::NodeKey> {
        // Keep the index mirror in sync for immediate lookups
        let _ = self.dom_index_mirror.try_update_sync();
        self.dom_index_shared
            .lock()
            .ok()
            .and_then(|s| s.get_element_by_id(id))
    }
}

impl HtmlPage {
    /// Dispatch a synthetic host event to the document using the JS bridge.
    /// Props must be a JSON object string (e.g., {"bubbles":true,"clientX":10}).
    pub fn host_dispatch_document(&mut self, ty: &str, props_json: &str) {
        let mut js = String::from("(function(){try{return document.__valorHostDispatch(\"");
        js.push_str(ty);
        js.push_str("\",null,");
        js.push_str(props_json);
        js.push_str(");}catch(e){return false;}})();");
        let _ = self.js_engine.eval_script(&js, "host://dispatch_document.js");
    }

    /// Dispatch a synthetic host event to a specific target key (string form) via the JS bridge.
    /// The target_key should match the DOM handle key string used internally.
    pub fn host_dispatch_to_key(&mut self, ty: &str, target_key: &str, props_json: &str) {
        let mut js = String::from("(function(){try{return document.__valorHostDispatch(\"");
        js.push_str(ty);
        js.push_str("\",\"");
        js.push_str(&target_key.replace('"', "\\\""));
        js.push_str("\",");
        js.push_str(props_json);
        js.push_str(");}catch(e){return false;}})();");
        let _ = self.js_engine.eval_script(&js, "host://dispatch_target.js");
    }
}

impl HtmlPage {
    /// Performance counters from the internal Layouter mirror: nodes reflowed in the last layout.
    pub fn layouter_perf_nodes_reflowed_last(&mut self) -> u64 {
        self.layouter_mirror.mirror_mut().perf_nodes_reflowed_last()
    }
    /// Performance counters from the internal Layouter mirror: number of dirty subtrees processed last.
    pub fn layouter_perf_dirty_subtrees_last(&mut self) -> u64 {
        self.layouter_mirror.mirror_mut().perf_dirty_subtrees_last()
    }

    /// Build a simple display list of rectangles from the current layout geometry and styles.
    /// Colors are derived from computed text color when available; otherwise a neutral gray.
    pub fn display_list_snapshot(&mut self) -> Result<Vec<DrawRect>, Error> {
        self.layouter_mirror.try_update_sync()?;
        let rects = self.layouter_mirror.mirror_mut().compute_layout_geometry();
        let snapshot = self.layouter_mirror.mirror_mut().snapshot();
        Ok(self.display_builder.build_rect_list(&rects, &snapshot))
    }

    /// Build a retained display list combining rectangles, text, and overlays.
    /// If a selection overlay is set via selection_set(), semi-transparent highlight
    /// quads are emitted for intersected inline text boxes. A simple focus ring is
    /// drawn when a focused element exists. Optionally, a small perf HUD text is
    /// added when VALOR_HUD=1.
    pub fn display_list_retained_snapshot(&self) -> Result<DisplayList, Error> {
        // IMPORTANT: Side-effect free: only read mirrors here.
        let rects = self.layouter_mirror.mirror().compute_layout_geometry();
        let snapshot = self.layouter_mirror.mirror().snapshot();
        // Use layouter's view for styles to avoid recomputation here
        let computed_map = self.layouter_mirror.mirror().computed_styles().clone();
        let robust_styles = computed_map.clone();
        trace!("retained_snapshot: rects={} nodes, computed_styles={} nodes", rects.len(), computed_map.len());

        let inputs = crate::display::RetainedInputs {
            rects,
            snapshot,
            computed_map: computed_map.clone(),
            // Provide a stable fallback equal to the primary map; avoid robust recomputation.
            computed_fallback: computed_map.clone(),
            computed_robust: Some(robust_styles),
            selection_overlay: self.selection_overlay,
            focused_node: self.focused_node,
            hud_enabled: self.hud_enabled,
            spillover_deferred: self.frame_scheduler.deferred(),
            last_style_restyled_nodes: self.last_style_restyled_nodes,
        };

        Ok(self.display_builder.build_retained(inputs))
    }

    pub fn text_list_snapshot(&mut self) -> Result<Vec<DrawText>, Error> {
        // Drain updates for consistency
        self.layouter_mirror.try_update_sync()?;
        // Gather geometry and snapshot for finding text nodes
        let rects = self.layouter_mirror.mirror_mut().compute_layout_geometry();
        let snapshot = self.layouter_mirror.mirror_mut().snapshot();
        let computed_map = self.layouter_mirror.mirror_mut().computed_styles().clone();

        Ok(self.display_builder.build_text_list(&rects, &snapshot, &computed_map))
    }

    /// Hit-test screen coordinates against current layout boxes and return the topmost NodeKey.
    /// The hit testing respects overflow:hidden by requiring the point to lie within any
    /// ancestor with overflow hidden. Inline text nodes are considered hittable too.
    pub fn hit_test(&mut self, x: i32, y: i32) -> Option<js::NodeKey> {
        if self.layouter_mirror.try_update_sync().is_err() { return None; }
        self.layouter_mirror.mirror_mut().hit_test(x, y)
    }

    /// Return the currently focused node, if any.
    pub fn focused_node(&self) -> Option<js::NodeKey> { self.focused_node }

    /// Set the focused node explicitly.
    pub fn focus_set(&mut self, node: Option<js::NodeKey>) { self.focused_node = node; }

    /// Move focus to the next focusable element using a basic tabindex order, then natural order fallback.
    pub fn focus_next(&mut self) -> Option<js::NodeKey> {
        if self.layouter_mirror.try_update_sync().is_err() { return None; }
        let snapshot = self.layouter_mirror.mirror_mut().snapshot();
        let attrs = self.layouter_mirror.mirror_mut().attrs_map();
        let next = focus_mod::next(&snapshot, &attrs, self.focused_node);
        self.focused_node = next;
        next
    }

    /// Move focus to the previous focusable element.
    pub fn focus_prev(&mut self) -> Option<js::NodeKey> {
        if self.layouter_mirror.try_update_sync().is_err() { return None; }
        let snapshot = self.layouter_mirror.mirror_mut().snapshot();
        let attrs = self.layouter_mirror.mirror_mut().attrs_map();
        let prev = focus_mod::prev(&snapshot, &attrs, self.focused_node);
        self.focused_node = prev;
        prev
    }

    /// Set the current text selection overlay rectangle in viewport coordinates.
    /// Pass the two corners of the selection (order does not matter). Use selection_clear() to remove.
    pub fn selection_set(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) { self.selection_overlay = Some((x0, y0, x1, y1)); }

    /// Clear any active text selection overlay.
    pub fn selection_clear(&mut self) { self.selection_overlay = None; }

    /// Return a list of selection rectangles by intersecting inline text boxes with a selection rect.
    pub fn selection_rects(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<layouter::LayoutRect> {
        if self.layouter_mirror.try_update_sync().is_err() { return Vec::new(); }
        let rects = self.layouter_mirror.mirror_mut().compute_layout_geometry();
        let snapshot = self.layouter_mirror.mirror_mut().snapshot();
        selection::selection_rects(&rects, &snapshot, x0, y0, x1, y1)
    }

    /// Compute a caret rectangle at the given point: a thin bar within the inline text box, if any.
    pub fn caret_at(&mut self, x: i32, y: i32) -> Option<layouter::LayoutRect> {
        if self.layouter_mirror.try_update_sync().is_err() { return None; }
        let rects = self.layouter_mirror.mirror_mut().compute_layout_geometry();
        let snapshot = self.layouter_mirror.mirror_mut().snapshot();
        let hit = self.hit_test(x, y);
        selection::caret_at(&rects, &snapshot, x, y, hit)
    }
}

impl HtmlPage {
    /// Return a minimal Accessibility (AX) tree snapshot as JSON.
    pub fn ax_tree_snapshot_string(&mut self) -> String {
        if self.layouter_mirror.try_update_sync().is_err() { return String::from("{\"role\":\"document\"}"); }
        let snapshot = self.layouter_mirror.mirror_mut().snapshot();
        let attrs_map = self.layouter_mirror.mirror_mut().attrs_map();
        crate::accessibility::ax_tree_snapshot_from(snapshot, attrs_map)
    }
}

impl HtmlPage {
    /// Dispatch a synthetic pointer move event to the document.
    /// The event is delivered via the JS runtime by calling document.dispatchEvent
    /// with a plain object carrying standard MouseEvent-like fields.
    pub fn dispatch_pointer_move(&mut self, x: f64, y: f64) {
        let mut js = String::from("(function(){try{var e={type:'mousemove',clientX:");
        js.push_str(&x.to_string());
        js.push_str(",clientY:");
        js.push_str(&y.to_string());
        js.push_str("};document.dispatchEvent(e);}catch(_){}})();");
        let _ = self.js_engine.eval_script(&js, "valor://event/pointer_move");
        let _ = self.js_engine.run_jobs();
    }

    /// Dispatch a synthetic pointer down (mouse down) event.
    pub fn dispatch_pointer_down(&mut self, x: f64, y: f64, button: u32) {
        let mut js = String::from("(function(){try{var e={type:'mousedown',clientX:");
        js.push_str(&x.to_string());
        js.push_str(",clientY:");
        js.push_str(&y.to_string());
        js.push_str(",button:");
        js.push_str(&button.to_string());
        js.push_str("};document.dispatchEvent(e);}catch(_){}})();");
        let _ = self.js_engine.eval_script(&js, "valor://event/pointer_down");
        let _ = self.js_engine.run_jobs();
    }

    /// Dispatch a synthetic pointer up (mouse up) event.
    pub fn dispatch_pointer_up(&mut self, x: f64, y: f64, button: u32) {
        let mut js = String::from("(function(){try{var e={type:'mouseup',clientX:");
        js.push_str(&x.to_string());
        js.push_str(",clientY:");
        js.push_str(&y.to_string());
        js.push_str(",button:");
        js.push_str(&button.to_string());
        js.push_str("};document.dispatchEvent(e);}catch(_){}})();");
        let _ = self.js_engine.eval_script(&js, "valor://event/pointer_up");
        let _ = self.js_engine.run_jobs();
    }

    /// Dispatch a synthetic keydown event with optional modifier flags.
    pub fn dispatch_key_down(&mut self, key: &str, code: &str, ctrl: bool, alt: bool, shift: bool) {
        let mut js = String::from("(function(){try{var e={type:'keydown',key:");
        js.push_str(&format!("{:?}", key));
        js.push_str(",code:");
        js.push_str(&format!("{:?}", code));
        js.push_str(",ctrlKey:");
        js.push_str(if ctrl {"true"} else {"false"});
        js.push_str(",altKey:");
        js.push_str(if alt {"true"} else {"false"});
        js.push_str(",shiftKey:");
        js.push_str(if shift {"true"} else {"false"});
        js.push_str("};document.dispatchEvent(e);}catch(_){}})();");
        let _ = self.js_engine.eval_script(&js, "valor://event/key_down");
        let _ = self.js_engine.run_jobs();
    }

    /// Dispatch a synthetic keyup event with optional modifier flags.
    pub fn dispatch_key_up(&mut self, key: &str, code: &str, ctrl: bool, alt: bool, shift: bool) {
        let mut js = String::from("(function(){try{var e={type:'keyup',key:");
        js.push_str(&format!("{:?}", key));
        js.push_str(",code:");
        js.push_str(&format!("{:?}", code));
        js.push_str(",ctrlKey:");
        js.push_str(if ctrl {"true"} else {"false"});
        js.push_str(",altKey:");
        js.push_str(if alt {"true"} else {"false"});
        js.push_str(",shiftKey:");
        js.push_str(if shift {"true"} else {"false"});
        js.push_str("};document.dispatchEvent(e);}catch(_){}})();");
        let _ = self.js_engine.eval_script(&js, "valor://event/key_up");
        let _ = self.js_engine.run_jobs();
    }

    /// Dispatch a synthetic text input (character) event. This is sent on ReceivedCharacter.
    pub fn dispatch_text_input(&mut self, text: &str) {
        let mut js = String::from("(function(){try{var e={type:'textinput',data:");
        js.push_str(&format!("{:?}", text));
        js.push_str("};document.dispatchEvent(e);}catch(_){}})();");
        let _ = self.js_engine.eval_script(&js, "valor://event/text_input");
        let _ = self.js_engine.run_jobs();
    }
}


impl HtmlPage {
    /// Attach a privileged chromeHost command channel to this page (for valor://chrome only).
    /// This installs the `chromeHost` namespace into the JS context with origin gating.
    pub fn attach_chrome_host(&mut self, sender: tokio::sync::mpsc::UnboundedSender<js::ChromeHostCommand>) -> Result<(), Error> {
        self.host_context.chrome_host_tx = Some(sender);
        // Install the chromeHost namespace now that a channel is available
        let bindings = js::build_chrome_host_bindings();
        let _ = self.js_engine.install_bindings(self.host_context.clone(), &bindings);
        let _ = self.js_engine.run_jobs();
        Ok(())
    }
}
