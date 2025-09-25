use crate::config::ValorConfig;
use crate::snapshots::{IRect, Snapshot};

use crate::runtime::JsRuntime;
use crate::scheduler::FrameScheduler;
use crate::url::stream_url;
use crate::{focus as focus_mod, selection, telemetry as telemetry_mod};
use anyhow::{Error, anyhow};
use css::style_types::ComputedStyle;
use css::types::Stylesheet;
use css::{CSSMirror, Orchestrator};
use css_core::LayoutNodeKind;
use css_core::LayoutRect;
use css_core::Layouter;
use html::dom::DOM;
use html::parser::HTMLParser;
use js::DOMUpdate::{EndOfDocument, InsertElement, SetAttr};
use js::{DOMMirror, DOMSubscriber, DOMUpdate, DomIndex, JsEngine};
use js_engine_v8::V8Engine;
use log::{info, trace};
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use tokio::runtime::Handle;
use tokio::sync::mpsc::error::TryRecvError as UnboundedTryRecvError;

// Type aliases to simplify frequent layout structure mappings
type TagsByKey = HashMap<js::NodeKey, String>;
type ElementChildren = HashMap<js::NodeKey, Vec<js::NodeKey>>;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use tokio::sync::{broadcast, mpsc};
use tokio_stream::StreamExt;
use tracing::info_span;
use url::Url;
use wgpu_renderer::{DrawRect, Renderer};

// Module-scoped type aliases to simplify complex types and avoid repetition
type NodeKeyMap<T> = HashMap<js::NodeKey, T>;
type NodeKeyVecMap = HashMap<js::NodeKey, Vec<js::NodeKey>>;
type LayoutMapsSnapshot = (
    NodeKeyMap<String>,
    NodeKeyVecMap,
    NodeKeyVecMap,
    NodeKeyMap<String>,
);

/// Note: FrameScheduler has moved to `scheduler.rs`.
/// Structured outcome of a single update() tick. Extend as needed.
pub struct UpdateOutcome {
    pub needs_redraw: bool,
}
pub struct HtmlPage {
    /// Optional currently focused node for focus management.
    focused_node: Option<js::NodeKey>,
    /// Optional active selection rectangle in viewport coordinates for highlight overlay (text selection highlight).
    selection_overlay: Option<IRect>,
    // If none, loading is finished. If some, still streaming.
    loader: Option<HTMLParser>,
    // The DOM of the page.
    dom: DOM,
    // Mirror that collects CSS from the DOM stream.
    css_mirror: DOMMirror<CSSMirror>,
    // Orchestrator mirror that computes styles using the css engine.
    orchestrator_mirror: DOMMirror<Orchestrator>,
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
        let inputs = html::parser::ParseInputs {
            in_updater: in_updater.clone(),
            keyman,
            byte_stream: stream_url(&url).await?,
            dom_updates: out_receiver,
            script_tx,
            base_url: url.clone(),
        };
        let loader = HTMLParser::parse(handle, inputs);

        // Create and attach the CSS mirror to observe DOM updates
        let css_mirror = DOMMirror::new(
            in_updater.clone(),
            dom.subscribe(),
            CSSMirror::with_base(url.clone()),
        );
        // Create and attach the Orchestrator mirror to observe DOM updates
        let orchestrator_mirror =
            DOMMirror::new(in_updater.clone(), dom.subscribe(), Orchestrator::new());
        // Create and attach the Layouter mirror to observe DOM updates
        let layouter_mirror = DOMMirror::new(in_updater.clone(), dom.subscribe(), Layouter::new());
        // Create and attach the Renderer mirror to observe DOM updates
        let renderer_mirror = DOMMirror::new(in_updater.clone(), dom.subscribe(), Renderer::new());
        // Create and attach a lightweight DOM index for JS getElement* functions
        let (dom_index_sub, dom_index_shared) = js::DomIndex::new();
        let dom_index_mirror = DOMMirror::new(in_updater.clone(), dom.subscribe(), dom_index_sub);

        // Debouncing removed in favor of single frame budget policy

        // Create the JS engine
        let mut js_engine =
            V8Engine::new().map_err(|e| anyhow!("failed to init V8Engine: {}", e))?;
        // Install direct host bindings (e.g., console + document) from the js crate
        let logger = Arc::new(js::ConsoleLogger);
        // Prepare JS context wiring for DOM manipulation functions
        let js_node_keys = Arc::new(Mutex::new(js_keyman));
        let js_local_id_counter = Arc::new(AtomicU64::new(0));
        let js_created_nodes = Arc::new(Mutex::new(HashMap::new()));
        // Build page origin string for same-origin checks
        let page_origin = match url.scheme() {
            "file" => String::from("file://"),
            _ => {
                let host = url.host_str().unwrap_or("");
                let port = url.port().map(|p| format!(":{p}")).unwrap_or_default();
                format!("{}://{}{}", url.scheme(), host, port)
            }
        };
        // Establish a high-resolution time origin for performance.now and timers
        let start_instant = std::time::Instant::now();
        // Initialize per-origin storage registries (in-memory for this page/session)
        let storage_local = Arc::new(Mutex::new(js::bindings::StorageRegistry::default()));
        let storage_session = Arc::new(Mutex::new(js::bindings::StorageRegistry::default()));

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
            fetch_registry: Arc::new(Mutex::new(Default::default())),
            performance_start: start_instant,
            storage_local: storage_local.clone(),
            storage_session: storage_session.clone(),
            chrome_host_tx: None,
        };
        let bindings = js::build_default_bindings();
        let _ = js_engine.install_bindings(&host_context, &bindings);

        // Frame scheduler budget (ms) from ValorConfig
        let frame_scheduler = FrameScheduler::new(config.frame_budget());

        Ok(Self {
            focused_node: None,
            selection_overlay: None,
            loader: Some(loader),
            dom,
            css_mirror,
            orchestrator_mirror,
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
                        let kind = match job.kind {
                            html::parser::ScriptKind::Module => "module",
                            html::parser::ScriptKind::Classic => "script",
                        };
                        let u = format!("inline:{kind}-{}", self.script_counter);
                        self.script_counter = self.script_counter.wrapping_add(1);
                        u
                    } else {
                        job.url.clone()
                    };
                    info!(
                        "HtmlPage: executing {} (length={} bytes)",
                        script_url,
                        job.source.len()
                    );
                    match job.kind {
                        html::parser::ScriptKind::Classic => {
                            let code = self.classic_script_source(&job, &script_url);
                            let _ = self.js_engine.eval_script(&code, &script_url);
                            let _ = self.js_engine.run_jobs();
                        }
                        html::parser::ScriptKind::Module => {
                            self.eval_module_job(&job, &script_url);
                        }
                    }
                }
                Err(UnboundedTryRecvError::Empty) => break,
                Err(UnboundedTryRecvError::Disconnected) => break,
            }
        }
    }

    /// Helper: obtain classic script source given a job and resolved script_url.
    fn classic_script_source(&self, job: &html::parser::ScriptJob, script_url: &str) -> String {
        // Inline or provided source: return immediately
        if !job.source.is_empty() || script_url.starts_with("inline:") {
            return job.source.clone();
        }
        // Parse URL or bail
        let Ok(url) = url::Url::parse(script_url) else {
            return String::new();
        };
        // Embedded chrome asset
        if url.scheme() == "valor" {
            let path = url.path();
            if let Some(bytes) =
                crate::embedded_chrome::get_embedded_chrome_asset(path).or_else(|| {
                    crate::embedded_chrome::get_embedded_chrome_asset(&format!(
                        "valor://chrome{path}"
                    ))
                })
            {
                return String::from_utf8_lossy(bytes).into_owned();
            }
            return String::new();
        }
        // Fetch text via stream_url for network/file schemes
        self.fetch_url_text(&url).unwrap_or_default()
    }

    fn fetch_url_text(&self, url: &url::Url) -> Result<String, anyhow::Error> {
        let fut = async {
            let mut buffer: Vec<u8> = Vec::new();
            let mut stream = stream_url(url).await?;
            while let Some(chunk) = stream.next().await {
                let bytes = chunk.map_err(|e| anyhow::anyhow!("{}", e))?;
                buffer.extend_from_slice(&bytes);
            }
            Ok::<String, anyhow::Error>(String::from_utf8_lossy(&buffer).into_owned())
        };
        self.host_context.tokio_handle.block_on(fut)
    }

    /// Helper: evaluate a module job using the resolver and engine, handling inline roots.
    fn eval_module_job(&mut self, job: &html::parser::ScriptJob, script_url: &str) {
        let resolver = &mut self.module_resolver;
        let inline_source = if script_url.starts_with("inline:") {
            Some(job.source.as_str())
        } else {
            None
        };
        if let Ok(bundle) = resolver.bundle_root(script_url, &self.url, inline_source) {
            let _ = self.js_engine.eval_module(&bundle, script_url);
            let _ = self.js_engine.run_jobs();
        } else {
            let _ = self.js_engine.eval_module(&job.source, script_url);
            let _ = self.js_engine.run_jobs();
        }
    }

    /// Execute at most one due JavaScript timer callback.
    /// This evaluates the runtime prelude hook `__valorTickTimersOnce(nowMs)` and then
    /// flushes engine microtasks to approximate browser ordering (microtasks after each task).
    pub(crate) fn tick_js_timers_once(&mut self) {
        // Use engine-provided clock (Date.now()/performance.now) by omitting the argument.
        // This keeps the runtime timer origin consistent with scheduling inside JS.
        let script = String::from(
            "(function(){ try { var f = globalThis.__valorTickTimersOnce; if (typeof f === 'function') f(); } catch(_){} })();",
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
        {
            return Some(guard.get_text_content(key));
        }
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

    /// Snapshot key layout-derived maps used by style and testing code.
    /// Returns (tags_by_key, element_children_by_key, raw_children_by_key, text_by_key)
    fn snapshot_layout_maps(&mut self) -> LayoutMapsSnapshot {
        let lay_snapshot = self.layouter_mirror.mirror_mut().snapshot();
        let mut tags_by_key: HashMap<js::NodeKey, String> = HashMap::new();
        let mut raw_children: HashMap<js::NodeKey, Vec<js::NodeKey>> = HashMap::new();
        let mut text_by_key: HashMap<js::NodeKey, String> = HashMap::new();
        for (key, kind, children) in lay_snapshot.into_iter() {
            match kind {
                LayoutNodeKind::Block { tag } => {
                    tags_by_key.insert(key, tag);
                }
                LayoutNodeKind::InlineText { text } => {
                    text_by_key.insert(key, text);
                }
                _ => {}
            }
            raw_children.insert(key, children);
        }
        let mut element_children: HashMap<js::NodeKey, Vec<js::NodeKey>> = HashMap::new();
        for (parent, kids) in raw_children.clone().into_iter() {
            let filtered: Vec<js::NodeKey> = kids
                .into_iter()
                .filter(|c| tags_by_key.contains_key(c))
                .collect();
            if tags_by_key.contains_key(&parent) || parent == js::NodeKey::ROOT {
                element_children.insert(parent, filtered);
            }
        }
        (tags_by_key, element_children, raw_children, text_by_key)
    }

    /// Extract concatenated CSS text from inline <style> elements using the current layout snapshot.
    #[allow(
        dead_code,
        reason = "Kept for future inline <style> extraction and test helpers"
    )]
    fn extract_inline_style_css(
        &mut self,
        tags_by_key: &HashMap<js::NodeKey, String>,
        raw_children: &HashMap<js::NodeKey, Vec<js::NodeKey>>,
        text_by_key: &HashMap<js::NodeKey, String>,
    ) -> String {
        let mut inline_style_css = String::new();
        for (node, tag) in tags_by_key {
            if !tag.eq_ignore_ascii_case("style") {
                continue;
            }
            let Some(children) = raw_children.get(node) else {
                continue;
            };
            for child in children {
                if let Some(txt) = text_by_key.get(child) {
                    inline_style_css.push_str(txt);
                }
            }
        }
        inline_style_css
    }

    /// Ensure StyleEngine's node inventory is rebuilt once post-load for deterministic matching.
    fn maybe_rebuild_style_nodes_after_load(
        &mut self,
        tags_by_key: &HashMap<js::NodeKey, String>,
        element_children: &HashMap<js::NodeKey, Vec<js::NodeKey>>,
        lay_attrs: &HashMap<js::NodeKey, HashMap<String, String>>,
    ) {
        // Orchestrator does not require an explicit rebuild; no-op guard retained for symmetry.
        if self.loader.is_none() && !self.style_nodes_rebuilt_after_load {
            let _ = (tags_by_key, element_children, lay_attrs);
            self.style_nodes_rebuilt_after_load = true;
            trace!("process_css_and_styles: orchestrator ready");
        }
    }

    /// Process CSS and style updates, returning whether styles have changed
    async fn process_css_and_styles(&mut self) -> Result<bool, Error> {
        let _span = info_span!("page.process_css_and_styles").entered();
        // Keep Layouter mirror fresh; avoid draining others here to prevent starvation while streaming
        let _ = self.layouter_mirror.try_update_sync();
        // Ensure CSSMirror has applied any pending DOM updates so that inline <style>
        // rules are visible in the aggregated stylesheet for this tick.
        self.css_mirror.try_update_sync()?;
        // Synchronize attributes for potential future needs (kept for symmetry)
        let lay_attrs = self.layouter_mirror.mirror_mut().attrs_map();
        trace!(
            "process_css_and_styles: layouter_attrs_count={} nodes",
            lay_attrs.len()
        );
        // Snapshot structure once and optionally rebuild StyleEngine's inventory
        let (tags_by_key, element_children, _raw_children, _text_by_key) =
            self.snapshot_layout_maps();
        self.maybe_rebuild_style_nodes_after_load(&tags_by_key, &element_children, &lay_attrs);

        // Use CSSMirror's aggregated in-document stylesheet (rebuilds on <style> updates)
        let author_styles = self.css_mirror.mirror_mut().styles().clone();

        // Apply stylesheet to orchestrator and compute once
        self.orchestrator_mirror
            .mirror_mut()
            .replace_stylesheet(&author_styles);
        let artifacts = self.orchestrator_mirror.mirror_mut().process_once()?;
        let computed_styles = artifacts.computed_styles;
        self.layouter_mirror
            .mirror_mut()
            .set_stylesheet(author_styles);
        self.layouter_mirror
            .mirror_mut()
            .set_computed_styles(computed_styles);

        // Mark dirty nodes for reflow if styles changed
        let style_changed = artifacts.styles_changed;
        if style_changed {
            self.layouter_mirror
                .mirror_mut()
                .mark_nodes_style_dirty(&[js::NodeKey::ROOT]);
        }

        self.last_style_restyled_nodes = 0;
        Ok(style_changed)
    }

    /// Compute layout and forward dirty rectangles to renderer (single frame budget policy)
    fn compute_layout(&mut self, style_changed: bool) -> Result<(), Error> {
        let _span = info_span!("page.compute_layout").entered();
        // Determine if layout should run based on actual style or material layouter changes,
        // and also ensure we run at least once if no geometry has been computed yet.
        let has_material_dirty = self.layouter_mirror.mirror_mut().has_material_dirty();
        let geometry_empty = self
            .layouter_mirror
            .mirror_mut()
            .compute_layout_geometry()
            .is_empty();
        let should_layout = style_changed || has_material_dirty || geometry_empty;

        if should_layout {
            // Respect frame budget: run layout at most once per frame window
            if !self.frame_scheduler.allow() {
                trace!(
                    "Layout skipped due to frame budget ({:?})",
                    self.frame_scheduler.budget()
                );
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
                "Layout: processed={node_count}, reflowed_nodes={nodes_reflowed}, dirty_subtrees={dirty_subtrees}, time_ms={layout_time_ms}, updates_applied_total={updates_applied}"
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
                        color: [0.0, 0.0, 0.0],
                    })
                    .collect();
                self.renderer_mirror
                    .mirror_mut()
                    .set_dirty_rects(dirty_rectangles);
            }
            // Request a redraw after any successful layout pass so retained display lists
            // can be rebuilt and presented even if dirty regions were coalesced away.
            self.needs_redraw = true;
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
        let outcome = UpdateOutcome {
            needs_redraw: self.needs_redraw,
        };
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

    // Internal accessors for sibling modules (events, etc.).
    pub(crate) fn js_engine_mut(&mut self) -> &mut V8Engine {
        &mut self.js_engine
    }
    pub(crate) fn host_context_mut(&mut self) -> &mut js::HostContext {
        &mut self.host_context
    }
    pub(crate) fn layouter_try_update_sync(&mut self) -> Result<(), Error> {
        self.layouter_mirror.try_update_sync()
    }
    /// Return a snapshot of the layouter's current geometry per node.
    pub fn layouter_geometry_mut(&mut self) -> HashMap<js::NodeKey, LayoutRect> {
        self.layouter_mirror.mirror_mut().compute_layout_geometry()
    }
    pub(crate) fn layouter_snapshot_mut(&mut self) -> Snapshot {
        self.layouter_mirror.mirror_mut().snapshot()
    }
    pub(crate) fn layouter_snapshot(&self) -> Snapshot {
        self.layouter_mirror.mirror().snapshot()
    }
    /// Clone and return the layouter's current attributes map (id/class/style) keyed by NodeKey.
    pub fn layouter_attrs_map(&mut self) -> HashMap<js::NodeKey, HashMap<String, String>> {
        self.layouter_mirror.mirror_mut().attrs_map()
    }
    pub(crate) fn layouter_computed_styles(&self) -> HashMap<js::NodeKey, ComputedStyle> {
        self.layouter_mirror.mirror().computed_styles().clone()
    }

    /// Ensure a layout pass has been run at least once or if material dirt is pending.
    /// This synchronous helper is intended for display snapshot code paths that
    /// cannot await the normal async update loop but need non-empty geometry.
    pub fn ensure_layout_now(&mut self) {
        let lay = self.layouter_mirror.mirror_mut();
        let need = lay.compute_layout_geometry().is_empty() || lay.has_material_dirty();
        if need {
            let _ = lay.compute_layout();
            // Mark that a redraw would be needed if a UI were present.
            self.needs_redraw = true;
        }
    }

    /// Return a structure-only snapshot for tests: (tags_by_key, element_children).
    /// This is derived from the DOM/layout snapshot the page can produce and does not
    /// depend on any internal layouter mirrors.
    pub fn layout_structure_snapshot(&mut self) -> (TagsByKey, ElementChildren) {
        // Ensure DOM is drained to get the latest nodes
        let _ = self.dom_index_mirror.try_update_sync();
        let (tags_by_key, element_children, _raw_children, _text_by_key) =
            self.snapshot_layout_maps();
        (tags_by_key, element_children)
    }

    /// Initialize a late `Layouter` subscriber (external mirror) from the current page state.
    /// Replays the existing element structure and attributes into the provided mirror so it
    /// can participate in layout and serialization even if it subscribed after parsing began.
    pub fn bootstrap_layouter_subscriber(&mut self, mirror: &mut DOMMirror<Layouter>) {
        // Take a structural snapshot and attrs from the internal layouter mirror
        let snapshot = self.layouter_mirror.mirror().snapshot();
        let attrs_map = self.layouter_mirror.mirror_mut().attrs_map();

        // Build tags_by_key and element_children from the snapshot
        let mut tags_by_key: HashMap<js::NodeKey, String> = HashMap::new();
        let mut children_tmp: HashMap<js::NodeKey, Vec<js::NodeKey>> = HashMap::new();
        for (key, kind, children) in snapshot.into_iter() {
            if let LayoutNodeKind::Block { tag } = kind {
                tags_by_key.insert(key, tag);
            }
            children_tmp.insert(key, children);
        }
        let mut element_children: HashMap<js::NodeKey, Vec<js::NodeKey>> = HashMap::new();
        for (parent, kids) in children_tmp.into_iter() {
            let filtered: Vec<js::NodeKey> = kids
                .into_iter()
                .filter(|c| tags_by_key.contains_key(c))
                .collect();
            element_children.insert(parent, filtered);
        }

        fn apply_attrs(
            lay: &mut Layouter,
            node: js::NodeKey,
            attrs: &HashMap<js::NodeKey, HashMap<String, String>>,
        ) {
            let Some(map) = attrs.get(&node) else {
                return;
            };
            for key_name in ["id", "class", "style"] {
                if let Some(value) = map.get(key_name) {
                    let _ = lay.apply_update(SetAttr {
                        node,
                        name: key_name.to_owned(),
                        value: value.clone(),
                    });
                }
            }
        }

        fn replay(
            lay: &mut Layouter,
            tags_by_key: &HashMap<js::NodeKey, String>,
            element_children: &HashMap<js::NodeKey, Vec<js::NodeKey>>,
            attrs: &HashMap<js::NodeKey, HashMap<String, String>>,
            parent: js::NodeKey,
        ) {
            let Some(children) = element_children.get(&parent) else {
                return;
            };
            for child in children {
                let tag = tags_by_key
                    .get(child)
                    .cloned()
                    .unwrap_or_else(|| String::from("div"));
                let _ = lay.apply_update(InsertElement {
                    parent,
                    node: *child,
                    tag,
                    pos: 0,
                });
                apply_attrs(lay, *child, attrs);
                replay(lay, tags_by_key, element_children, attrs, *child);
            }
        }

        let lay = mirror.mirror_mut();
        replay(
            lay,
            &tags_by_key,
            &element_children,
            &attrs_map,
            js::NodeKey::ROOT,
        );
        let _ = lay.apply_update(EndOfDocument);
    }
    pub(crate) fn display_builder(&self) -> &dyn crate::display::DisplayBuilder {
        &*self.display_builder
    }
    pub(crate) fn selection_overlay(&self) -> Option<IRect> {
        self.selection_overlay
    }
    pub(crate) fn hud_enabled(&self) -> bool {
        self.hud_enabled
    }
    pub(crate) fn frame_spillover_deferred(&self) -> u64 {
        self.frame_scheduler.deferred()
    }
    pub(crate) fn last_style_restyled_nodes(&self) -> u64 {
        self.last_style_restyled_nodes
    }
    pub(crate) fn layouter_hit_test(&mut self, x: i32, y: i32) -> Option<js::NodeKey> {
        self.layouter_mirror.mirror_mut().hit_test(x, y)
    }

    /// Drain mirrors and return a snapshot clone of computed styles per node.
    /// Ensures the latest inline <style> collected by CSSMirror is forwarded to the
    /// engine before taking the snapshot so callers that query immediately after
    /// parsing completes (without another update tick) still see up-to-date styles.
    pub fn computed_styles_snapshot(
        &mut self,
    ) -> Result<HashMap<js::NodeKey, ComputedStyle>, Error> {
        // Ensure the latest inline <style> and orchestrator state are reflected
        self.css_mirror.try_update_sync()?;
        let sheet = self.css_mirror.mirror_mut().styles().clone();
        self.orchestrator_mirror
            .mirror_mut()
            .replace_stylesheet(&sheet);
        let artifacts = self.orchestrator_mirror.mirror_mut().process_once()?;
        Ok(artifacts.computed_styles)
    }

    /// Drain CSS mirror and return a snapshot clone of discovered external stylesheet URLs
    pub fn discovered_stylesheets_snapshot(&mut self) -> Result<Vec<String>, Error> {
        self.css_mirror.try_update_sync()?;
        Ok(self
            .css_mirror
            .mirror_mut()
            .discovered_stylesheets()
            .to_vec())
    }

    /// Return a JSON string with key performance counters from the layouter to aid diagnostics (Phase 8).
    pub fn perf_counters_snapshot_string(&mut self) -> String {
        let _ = self.layouter_mirror.try_update_sync();
        let lay = self.layouter_mirror.mirror_mut();
        let counters = crate::telemetry::PerfCounters {
            nodes_reflowed_last: lay.perf_nodes_reflowed_last(),
            nodes_reflowed_total: lay.perf_updates_applied(),
            dirty_subtrees_last: lay.perf_dirty_subtrees_last(),
            layout_time_last_ms: lay.perf_layout_time_last_ms(),
            layout_time_total_ms: lay.perf_layout_time_total_ms(),
            restyled_nodes_last: self.last_style_restyled_nodes,
            spillover_deferred: self.frame_scheduler.deferred(),
            line_boxes_last: lay.perf_line_boxes_last(),
            shaped_runs_last: lay.perf_shaped_runs_last(),
            early_outs_last: lay.perf_early_outs_last(),
        };
        telemetry_mod::perf_counters_json(&counters)
    }

    /// Emit production-friendly telemetry (JSON) when enabled in ValorConfig.
    /// This prints a single-line JSON record per tick with core Phase 8 counters.
    /// Intended for external tooling to scrape logs; kept opt-in to avoid overhead.
    pub fn emit_perf_telemetry_if_enabled(&mut self) {
        telemetry_mod::maybe_emit(
            self.telemetry_enabled,
            &self.perf_counters_snapshot_string(),
        );
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
    /// Return the currently focused node, if any.
    pub fn focused_node(&self) -> Option<js::NodeKey> {
        self.focused_node
    }

    /// Set the focused node explicitly.
    pub fn focus_set(&mut self, node: Option<js::NodeKey>) {
        self.focused_node = node;
    }

    /// Move focus to the next focusable element using a basic tabindex order, then natural order fallback.
    pub fn focus_next(&mut self) -> Option<js::NodeKey> {
        if self.layouter_mirror.try_update_sync().is_err() {
            return None;
        }
        let snapshot = self.layouter_mirror.mirror_mut().snapshot();
        let attrs = self.layouter_mirror.mirror_mut().attrs_map();
        let next = focus_mod::next(&snapshot, &attrs, self.focused_node);
        self.focused_node = next;
        next
    }

    /// Move focus to the previous focusable element.
    pub fn focus_prev(&mut self) -> Option<js::NodeKey> {
        if self.layouter_mirror.try_update_sync().is_err() {
            return None;
        }
        let snapshot = self.layouter_mirror.mirror_mut().snapshot();
        let attrs = self.layouter_mirror.mirror_mut().attrs_map();
        let prev = focus_mod::prev(&snapshot, &attrs, self.focused_node);
        self.focused_node = prev;
        prev
    }

    /// Set the current text selection overlay rectangle in viewport coordinates.
    /// Pass the two corners of the selection (order does not matter). Use selection_clear() to remove.
    pub fn selection_set(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        self.selection_overlay = Some((x0, y0, x1, y1));
    }

    /// Clear any active text selection overlay.
    pub fn selection_clear(&mut self) {
        self.selection_overlay = None;
    }

    /// Return a list of selection rectangles by intersecting inline text boxes with a selection rect.
    pub fn selection_rects(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<LayoutRect> {
        if self.layouter_mirror.try_update_sync().is_err() {
            return Vec::new();
        }
        let rects = self.layouter_mirror.mirror_mut().compute_layout_geometry();
        let snapshot = self.layouter_mirror.mirror_mut().snapshot();
        selection::selection_rects(&rects, &snapshot, (x0, y0, x1, y1))
    }

    /// Compute a caret rectangle at the given point: a thin bar within the inline text box, if any.
    pub fn caret_at(&mut self, x: i32, y: i32) -> Option<LayoutRect> {
        if self.layouter_mirror.try_update_sync().is_err() {
            return None;
        }
        let rects = self.layouter_mirror.mirror_mut().compute_layout_geometry();
        let snapshot = self.layouter_mirror.mirror_mut().snapshot();
        let hit = self.hit_test(x, y);
        selection::caret_at(&rects, &snapshot, x, y, hit)
    }
}

impl HtmlPage {
    /// Return a minimal Accessibility (AX) tree snapshot as JSON.
    pub fn ax_tree_snapshot_string(&mut self) -> String {
        if self.layouter_mirror.try_update_sync().is_err() {
            return String::from("{\"role\":\"document\"}");
        }
        let snapshot = self.layouter_mirror.mirror_mut().snapshot();
        let attrs_map = self.layouter_mirror.mirror_mut().attrs_map();
        crate::accessibility::ax_tree_snapshot_from(snapshot, attrs_map)
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
}
