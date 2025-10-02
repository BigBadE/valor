use crate::accessibility::ax_tree_snapshot_from;
use crate::config::ValorConfig;
use crate::embedded_chrome::get_embedded_chrome_asset;
use crate::events::KeyMods;
use crate::runtime::{DefaultJsRuntime, JsRuntime as _};
use crate::scheduler::FrameScheduler;
use crate::snapshots::IRect;
use crate::telemetry::PerfCounters;
use crate::url::stream_url;
use crate::{focus as focus_mod, selection, telemetry as telemetry_mod};
use anyhow::{Error, anyhow};
use core::sync::atomic::AtomicU64;
use css::style_types::ComputedStyle;
use css::types::Stylesheet;
use css::{CSSMirror, Orchestrator};
use css_core::{LayoutNodeKind, LayoutRect, Layouter};
use html::dom::DOM;
use html::parser::{HTMLParser, ParseInputs, ScriptJob, ScriptKind};
use js::DOMUpdate::{EndOfDocument, InsertElement, SetAttr};
use js::bindings::{FetchRegistry, StorageRegistry};
use js::{
    ChromeHostCommand, ConsoleLogger, DOMMirror, DOMSubscriber, DOMUpdate, DomIndex, HostContext,
    JsEngine as _, ModuleResolver, NodeKey, NodeKeyManager, SharedDomIndex,
    SimpleFileModuleResolver, build_chrome_host_bindings, build_default_bindings,
};
use js_engine_v8::V8Engine;
use log::{info, trace};
use renderer::{DisplayItem, DisplayList, DrawRect, Renderer};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::runtime::Handle;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::{broadcast, mpsc};
use tokio_stream::StreamExt as _;
use tracing::info_span;
use url::Url;

// Module-scoped type aliases to simplify complex types and avoid repetition
/// Map from `NodeKey` to a generic value type.
type NodeKeyMap<T> = HashMap<NodeKey, T>;
/// Map from `NodeKey` to a vector of child `NodeKeys`.
type NodeKeyVecMap = HashMap<NodeKey, Vec<NodeKey>>;
/// Map from `NodeKey` to tag name.
type TagsByKey = HashMap<NodeKey, String>;
/// Map from `NodeKey` to element children.
type ElementChildren = HashMap<NodeKey, Vec<NodeKey>>;
/// Snapshot of layout maps: (`tags_by_key`, `element_children`, `raw_children`, `text_by_key`).
type LayoutMapsSnapshot = (
    NodeKeyMap<String>,
    NodeKeyVecMap,
    NodeKeyVecMap,
    NodeKeyMap<String>,
);

/// Note: `FrameScheduler` has moved to `scheduler.rs`.
/// Structured outcome of a single `update()` tick. Extend as needed.
pub struct UpdateOutcome {
    pub redraw_needed: bool,
}

// TODO: Refactor HtmlPage to use a state machine or enums instead of multiple bools
// to reduce the number of boolean fields (currently 5: hud_enabled, dom_content_loaded_fired,
// style_nodes_rebuilt_after_load, needs_redraw, telemetry_enabled)
#[allow(
    clippy::struct_excessive_bools,
    reason = "TODO: Refactor to use state machine or enums"
)]
pub struct HtmlPage {
    /// Optional currently focused node for focus management.
    focused_node: Option<NodeKey>,
    /// Optional active selection rectangle in viewport coordinates for highlight overlay (text selection highlight).
    selection_overlay: Option<IRect>,
    /// HTML parser for streaming content; None when loading is finished.
    loader: Option<HTMLParser>,
    /// The DOM of the page.
    dom: DOM,
    /// Mirror that collects CSS from the DOM stream.
    css_mirror: DOMMirror<CSSMirror>,
    /// Orchestrator mirror that computes styles using the css engine.
    orchestrator_mirror: DOMMirror<Orchestrator>,
    /// Layouter mirror that maintains a layout tree from DOM updates.
    layouter_mirror: DOMMirror<Layouter>,
    /// Renderer mirror that maintains a scene graph from DOM updates.
    renderer_mirror: DOMMirror<Renderer>,
    /// DOM index mirror for JS document.getElement* queries.
    dom_index_mirror: DOMMirror<DomIndex>,
    /// Shared state for DOM index to support synchronous lookups (e.g., getElementById).
    dom_index_shared: SharedDomIndex,
    /// For sending updates to the DOM.
    in_updater: mpsc::Sender<Vec<DOMUpdate>>,
    /// JavaScript engine and script queue.
    js_engine: V8Engine,
    /// Host context for privileged binding decisions and shared registries.
    host_context: HostContext,
    /// Script receiver for processing script jobs.
    script_rx: UnboundedReceiver<ScriptJob>,
    /// Counter for tracking script execution.
    script_counter: u64,
    /// Current page URL.
    #[allow(dead_code, reason = "URL is kept for future navigation and debugging")]
    url: Url,
    /// ES module resolver/bundler adapter (JS crate) for side-effect modules.
    module_resolver: Box<dyn ModuleResolver>,
    /// Frame scheduler to coalesce layout per frame with a budget (Phase 5).
    frame_scheduler: FrameScheduler,
    /// Diagnostics: number of nodes restyled in the last tick.
    last_style_restyled_nodes: u64,
    /// Whether we've dispatched `DOMContentLoaded` to JS listeners.
    dom_content_loaded_fired: bool,
    /// One-time post-load guard to rebuild the `StyleEngine`'s node inventory from the Layouter
    /// for deterministic style resolution in the normal update path.
    style_nodes_rebuilt_after_load: bool,
    /// Whether the last update produced visual changes that require a redraw.
    needs_redraw: bool,
    /// Whether to emit perf telemetry lines per tick.
    telemetry_enabled: bool,
}

/// Helper to create DOM mirrors for the page.
struct DomMirrors {
    /// CSS mirror for observing DOM updates.
    css_mirror: DOMMirror<CSSMirror>,
    /// Orchestrator mirror for style computation.
    orchestrator_mirror: DOMMirror<Orchestrator>,
    /// Layouter mirror for layout tree management.
    layouter_mirror: DOMMirror<Layouter>,
    /// Renderer mirror for scene graph management.
    renderer_mirror: DOMMirror<Renderer>,
    /// DOM index mirror for JS queries.
    dom_index_mirror: DOMMirror<DomIndex>,
    /// Shared DOM index for synchronous lookups.
    dom_index_shared: SharedDomIndex,
}

/// Helper to create JS engine context.
struct JsContext {
    /// JavaScript engine instance.
    js_engine: V8Engine,
    /// Host context for JS bindings.
    host_context: HostContext,
}

impl HtmlPage {
    /// Create DOM mirrors for observing DOM updates.
    fn create_dom_mirrors(
        in_updater: &mpsc::Sender<Vec<DOMUpdate>>,
        dom: &DOM,
        url: &Url,
    ) -> DomMirrors {
        let css_mirror = DOMMirror::new(
            in_updater.clone(),
            dom.subscribe(),
            CSSMirror::with_base(url.clone()),
        );
        let orchestrator_mirror =
            DOMMirror::new(in_updater.clone(), dom.subscribe(), Orchestrator::new());
        let layouter_mirror = DOMMirror::new(in_updater.clone(), dom.subscribe(), Layouter::new());
        let renderer_mirror = DOMMirror::new(in_updater.clone(), dom.subscribe(), Renderer::new());
        let (dom_index_sub, dom_index_shared) = DomIndex::new();
        let dom_index_mirror = DOMMirror::new(in_updater.clone(), dom.subscribe(), dom_index_sub);

        DomMirrors {
            css_mirror,
            orchestrator_mirror,
            layouter_mirror,
            renderer_mirror,
            dom_index_mirror,
            dom_index_shared,
        }
    }

    /// Build page origin string for same-origin checks.
    fn build_page_origin(url: &Url) -> String {
        if url.scheme() == "file" {
            String::from("file://")
        } else {
            let host = url.host_str().unwrap_or("");
            let port = url
                .port()
                .map(|port| format!(":{port}"))
                .unwrap_or_default();
            format!("{}://{}{}", url.scheme(), host, port)
        }
    }

    /// Create and initialize the JS engine with host context.
    ///
    /// # Errors
    ///
    /// Returns an error if JS engine initialization fails.
    fn create_js_context(
        in_updater: &mpsc::Sender<Vec<DOMUpdate>>,
        js_keyman: NodeKeyManager<u64>,
        dom_index_shared: &SharedDomIndex,
        handle: &Handle,
        url: &Url,
    ) -> Result<JsContext, Error> {
        let mut js_engine =
            V8Engine::new().map_err(|err| anyhow!("failed to init V8Engine: {}", err))?;
        let logger = Arc::new(ConsoleLogger);
        let js_node_keys = Arc::new(Mutex::new(js_keyman));
        let js_local_id_counter = Arc::new(AtomicU64::new(0));
        let js_created_nodes = Arc::new(Mutex::new(HashMap::new()));
        let page_origin = Self::build_page_origin(url);
        let start_instant = Instant::now();
        let storage_local = Arc::new(Mutex::new(StorageRegistry::default()));
        let storage_session = Arc::new(Mutex::new(StorageRegistry::default()));

        let host_context = HostContext {
            page_id: None,
            logger,
            dom_sender: in_updater.clone(),
            js_node_keys,
            js_local_id_counter,
            js_created_nodes,
            dom_index: Arc::clone(dom_index_shared),
            tokio_handle: handle.clone(),
            page_origin,
            fetch_registry: Arc::new(Mutex::new(FetchRegistry::default())),
            performance_start: start_instant,
            storage_local: Arc::clone(&storage_local),
            storage_session: Arc::clone(&storage_session),
            chrome_host_tx: None,
        };
        let bindings = build_default_bindings();
        let _unused = js_engine.install_bindings(&host_context, &bindings);

        Ok(JsContext {
            js_engine,
            host_context,
        })
    }
    /// Create a new `HtmlPage` by streaming the content from the given URL
    /// Create a new `HtmlPage` by streaming content from the given URL.
    ///
    /// # Errors
    ///
    /// Returns an error if page initialization fails.
    pub async fn new(handle: &Handle, url: Url, config: ValorConfig) -> Result<Self, Error> {
        let (out_updater, out_receiver) = broadcast::channel(128);
        let (in_updater, in_receiver) = mpsc::channel(128);

        let mut dom = DOM::new(out_updater, in_receiver);
        let keyman = dom.register_parser_manager();
        let js_keyman = dom.register_manager::<u64>();

        let (script_tx, script_rx) = unbounded_channel::<ScriptJob>();
        let inputs = ParseInputs {
            in_updater: in_updater.clone(),
            keyman,
            byte_stream: stream_url(&url).await?,
            dom_updates: out_receiver,
            script_tx,
            base_url: url.clone(),
        };
        let loader = HTMLParser::parse(handle, inputs);

        let mirrors = Self::create_dom_mirrors(&in_updater, &dom, &url);
        let js_ctx = Self::create_js_context(
            &in_updater,
            js_keyman,
            &mirrors.dom_index_shared,
            handle,
            &url,
        )?;
        let frame_scheduler = FrameScheduler::new(config.frame_budget());

        Ok(Self {
            focused_node: None,
            selection_overlay: None,
            loader: Some(loader),
            dom,
            css_mirror: mirrors.css_mirror,
            orchestrator_mirror: mirrors.orchestrator_mirror,
            layouter_mirror: mirrors.layouter_mirror,
            renderer_mirror: mirrors.renderer_mirror,
            dom_index_mirror: mirrors.dom_index_mirror,
            dom_index_shared: mirrors.dom_index_shared,
            in_updater,
            js_engine: js_ctx.js_engine,
            host_context: js_ctx.host_context,
            script_rx,
            script_counter: 0,
            url: url.clone(),
            module_resolver: Box::new(SimpleFileModuleResolver::new()),
            frame_scheduler,
            last_style_restyled_nodes: 0,
            dom_content_loaded_fired: false,
            style_nodes_rebuilt_after_load: false,
            needs_redraw: false,
            telemetry_enabled: config.telemetry_enabled,
        })
    }

    /// Returns true once parsing has fully finalized and the loader has been consumed.
    /// This becomes true only after an `update()` call has observed the parser finished
    /// and awaited its completion.
    pub const fn parsing_finished(&self) -> bool {
        self.loader.is_none()
    }

    /// Execute any pending inline scripts from the parser
    pub(crate) fn execute_pending_scripts(&mut self) {
        while let Ok(job) = self.script_rx.try_recv() {
            let script_url = if job.url.is_empty() {
                let kind = match job.kind {
                    ScriptKind::Module => "module",
                    ScriptKind::Classic => "script",
                };
                let url = format!("inline:{kind}-{}", self.script_counter);
                self.script_counter = self.script_counter.wrapping_add(1);
                url
            } else {
                job.url.clone()
            };
            info!(
                "HtmlPage: executing {} (length={} bytes)",
                script_url,
                job.source.len()
            );
            match job.kind {
                ScriptKind::Classic => {
                    let code = self.classic_script_source(&job, &script_url);
                    let _unused = self.js_engine.eval_script(&code, &script_url);
                    let _unused2 = self.js_engine.run_jobs();
                }
                ScriptKind::Module => {
                    self.eval_module_job(&job, &script_url);
                }
            }
        }
    }

    /// Helper: obtain classic script source given a job and resolved `script_url`.
    fn classic_script_source(&self, job: &ScriptJob, script_url: &str) -> String {
        // Inline or provided source: return immediately
        if !job.source.is_empty() || script_url.starts_with("inline:") {
            return job.source.clone();
        }
        // Parse URL or bail
        let Ok(url) = Url::parse(script_url) else {
            return String::new();
        };
        // Embedded chrome asset
        if url.scheme() == "valor" {
            let path = url.path();
            if let Some(bytes) = get_embedded_chrome_asset(path)
                .or_else(|| get_embedded_chrome_asset(&format!("valor://chrome{path}")))
            {
                return String::from_utf8_lossy(bytes).into_owned();
            }
            return String::new();
        }
        // Fetch text via stream_url for network/file schemes
        self.fetch_url_text(&url).unwrap_or_default()
    }

    /// Fetch URL text content.
    ///
    /// # Errors
    ///
    /// Returns an error if fetching fails.
    fn fetch_url_text(&self, url: &Url) -> Result<String, Error> {
        let fut = async {
            let mut buffer: Vec<u8> = Vec::new();
            let mut stream = stream_url(url).await?;
            while let Some(chunk) = stream.next().await {
                let bytes = chunk.map_err(|err| anyhow!("{}", err))?;
                buffer.extend_from_slice(&bytes);
            }
            Ok::<String, Error>(String::from_utf8_lossy(&buffer).into_owned())
        };
        self.host_context.tokio_handle.block_on(fut)
    }

    /// Helper: evaluate a module job using the resolver and engine, handling inline roots.
    fn eval_module_job(&mut self, job: &ScriptJob, script_url: &str) {
        let resolver = &mut self.module_resolver;
        let inline_source = script_url
            .starts_with("inline:")
            .then_some(job.source.as_str());
        if let Ok(bundle) = resolver.bundle_root(script_url, &self.url, inline_source) {
            let _unused = self.js_engine.eval_module(&bundle, script_url);
            let _unused2 = self.js_engine.run_jobs();
        } else {
            let _unused = self.js_engine.eval_module(&job.source, script_url);
            let _unused2 = self.js_engine.run_jobs();
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
        let _unused = self.js_engine.eval_script(&script, "valor://timers_tick");
        let _unused2 = self.js_engine.run_jobs();
    }

    /// Synchronously fetch the textContent for an element by id using the `DomIndex` mirror.
    /// This helper keeps the index in sync for tests that query immediately after updates.
    pub fn text_content_by_id_sync(&mut self, id: &str) -> Option<String> {
        // Keep index mirror fresh for same-tick queries
        let _unused = self.dom_index_mirror.try_update_sync();
        if let Ok(guard) = self.dom_index_shared.lock()
            && let Some(key) = guard.get_element_by_id(id)
        {
            return Some(guard.get_text_content(key));
        }
        None
    }

    /// Finalize DOM loading if the loader has finished.
    ///
    /// # Errors
    ///
    /// Returns an error if DOM finalization fails.
    async fn finalize_dom_loading_if_needed(&mut self) -> Result<(), Error> {
        if self.loader.as_ref().is_some_and(HTMLParser::is_finished) {
            let loader = self
                .loader
                .take()
                .ok_or_else(|| anyhow!("Loader is finished and None!"))?;
            trace!("Loader finished, finalizing DOM");
            loader.finish().await?;
        }
        Ok(())
    }

    /// Handle `DOMContentLoaded` event if needed.
    ///
    /// # Errors
    ///
    /// Returns an error if event handling fails.
    pub(crate) fn handle_dom_content_loaded_if_needed(&mut self) -> Result<(), Error> {
        if self.loader.is_none() && !self.dom_content_loaded_fired {
            info!("HtmlPage: dispatching DOMContentLoaded");
            let _unused = self
                .js_engine
                .eval_script(
                    "(function(){try{var d=globalThis.document; if(d&&typeof d.__valorDispatchDOMContentLoaded==='function'){ d.__valorDispatchDOMContentLoaded(); }}catch(_){}})();",
                    "valor://dom_events",
                );
            let _unused2 = self.js_engine.run_jobs();
            self.dom_content_loaded_fired = true;
            // After DOMContentLoaded, DOM listener mutations will be applied on the next regular tick.
            // Keep the DOM index mirror in sync in a non-blocking manner for tests.
            self.dom_index_mirror.try_update_sync()?;
        }
        Ok(())
    }

    /// Snapshot key layout-derived maps used by style and testing code.
    /// Returns (`tags_by_key`, `element_children_by_key`, `raw_children_by_key`, `text_by_key`)
    fn snapshot_layout_maps(&mut self) -> LayoutMapsSnapshot {
        let lay_snapshot = self.layouter_mirror.mirror_mut().snapshot();
        let mut tags_by_key: HashMap<NodeKey, String> = HashMap::new();
        let mut raw_children: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();
        let mut text_by_key: HashMap<NodeKey, String> = HashMap::new();
        for (key, kind, children) in lay_snapshot {
            match kind {
                LayoutNodeKind::Block { tag } => {
                    tags_by_key.insert(key, tag);
                }
                LayoutNodeKind::InlineText { text } => {
                    text_by_key.insert(key, text);
                }
                LayoutNodeKind::Document => {}
            }
            raw_children.insert(key, children);
        }
        let mut element_children: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();
        let children_vec: Vec<_> = raw_children.clone().into_iter().collect();
        for (parent, kids) in children_vec {
            let filtered: Vec<NodeKey> = kids
                .into_iter()
                .filter(|child| tags_by_key.contains_key(child))
                .collect();
            if tags_by_key.contains_key(&parent) || parent == NodeKey::ROOT {
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
        tags_by_key: &HashMap<NodeKey, String>,
        raw_children: &HashMap<NodeKey, Vec<NodeKey>>,
        text_by_key: &HashMap<NodeKey, String>,
    ) -> String {
        let mut inline_style_css = String::new();
        let tags_vec: Vec<_> = tags_by_key.iter().collect();
        for (node, tag) in tags_vec {
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

    /// Ensure `StyleEngine`'s node inventory is rebuilt once post-load for deterministic matching.
    fn maybe_rebuild_style_nodes_after_load(
        &mut self,
        _tags_by_key: &HashMap<NodeKey, String>,
        _element_children: &HashMap<NodeKey, Vec<NodeKey>>,
        _lay_attrs: &HashMap<NodeKey, HashMap<String, String>>,
    ) {
        // Orchestrator does not require an explicit rebuild; no-op guard retained for symmetry.
        if self.loader.is_none() && !self.style_nodes_rebuilt_after_load {
            self.style_nodes_rebuilt_after_load = true;
            trace!("process_css_and_styles: orchestrator ready");
        }
    }

    /// Process CSS and style updates, returning whether styles have changed
    /// Process CSS and style updates.
    ///
    /// # Errors
    ///
    /// Returns an error if CSS processing fails.
    fn process_css_and_styles(&mut self) -> Result<bool, Error> {
        let _span = info_span!("page.process_css_and_styles").entered();
        // Keep Layouter mirror fresh; avoid draining others here to prevent starvation while streaming
        let _unused = self.layouter_mirror.try_update_sync();
        // Ensure CSSMirror has applied any pending DOM updates so that inline <style>
        // rules are visible in the aggregated stylesheet for this tick.
        self.css_mirror.try_update_sync()?;
        // Ensure the Orchestrator mirror has applied DOM updates prior to stylesheet processing
        self.orchestrator_mirror.try_update_sync()?;
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
                .mark_nodes_style_dirty(&[NodeKey::ROOT]);
        }

        self.last_style_restyled_nodes = 0;
        Ok(style_changed)
    }

    /// Compute layout and forward dirty rectangles to renderer (single frame budget policy)
    /// Compute layout if needed.
    fn compute_layout(&mut self, style_changed: bool) {
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

        let should_run_layout = should_layout;
        if !should_run_layout && !self.frame_scheduler.allow() {
            self.frame_scheduler.incr_deferred();
            trace!("Layout deferred: frame budget not met");
            return;
        }

        if should_run_layout && self.frame_scheduler.allow() {
            let layouter = self.layouter_mirror.mirror_mut();
            let _unused = layouter.compute_layout();

            // Log layout performance metrics
            let node_count = layouter.snapshot().len();
            let nodes_reflowed = layouter.perf_nodes_reflowed_last();
            let dirty_subtrees = layouter.perf_dirty_subtrees_last();
            let layout_time_ms = layouter.perf_layout_time_last_ms();
            let updates_applied = layouter.perf_updates_applied();
            info!(
                "Layout: processed={node_count}, reflowed_nodes={nodes_reflowed}, dirty_subtrees={dirty_subtrees}, time_ms={layout_time_ms}, updates_applied_total={updates_applied}"
            );

            // Forward dirty rectangles to the renderer for partial redraws
            let dirty_rectangles = layouter.take_dirty_rects();
            if !dirty_rectangles.is_empty() {
                let dirty_rectangles: Vec<DrawRect> = dirty_rectangles
                    .into_iter()
                    .map(|rect| DrawRect {
                        x: rect.x,
                        y: rect.y,
                        width: rect.width,
                        height: rect.height,
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
    }

    /// Run a single update tick for the page.
    ///
    /// # Errors
    ///
    /// Returns an error if any update step fails.
    pub async fn update(&mut self) -> Result<(), Error> {
        self.update_with_outcome().await.map(|_| ())
    }

    /// Same as `update()`, but returns a structured outcome for callers that care.
    ///
    /// # Errors
    ///
    /// Returns an error if any update step fails (DOM loading, JS execution, CSS processing, or layout).
    pub async fn update_with_outcome(&mut self) -> Result<UpdateOutcome, Error> {
        let _span = info_span!("page.update").entered();
        // Finalize DOM loading if the loader has finished
        self.finalize_dom_loading_if_needed().await?;

        // Drive a single JS timers tick via runtime
        let mut runtime = DefaultJsRuntime;
        runtime.tick_timers_once(self);

        // Apply any pending DOM updates
        self.dom.update()?;
        // Keep the DOM index mirror in sync before any JS queries (e.g., getElementById)
        self.dom_index_mirror.try_update_sync()?;
        // Execute pending scripts and DOMContentLoaded sequencing via runtime after DOM drained
        runtime.drive_after_dom_update(self).await?;

        // Process CSS and style updates
        let style_changed = self.process_css_and_styles()?;

        // Compute layout and forward dirty rectangles
        self.compute_layout(style_changed);

        // Drain renderer mirror after DOM broadcast so the scene graph stays in sync (non-blocking)
        self.renderer_mirror.try_update_sync()?;
        // Emit optional production telemetry for this tick per config
        self.emit_perf_telemetry_if_enabled();
        let outcome = UpdateOutcome {
            redraw_needed: self.needs_redraw,
        };
        Ok(outcome)
    }

    pub fn create_mirror<T: DOMSubscriber>(&self, mirror: T) -> DOMMirror<T> {
        DOMMirror::new(self.in_updater.clone(), self.dom.subscribe(), mirror)
    }

    /// Return whether a redraw is needed since the last call and clear the flag.
    pub const fn take_needs_redraw(&mut self) -> bool {
        let value = self.needs_redraw;
        self.needs_redraw = false;
        value
    }

    /// Evaluate arbitrary JS in the page's engine (testing helper) and flush microtasks.
    ///
    /// # Errors
    ///
    /// Returns an error if JavaScript evaluation or job execution fails.
    pub fn eval_js(&mut self, source: &str) -> Result<(), Error> {
        self.js_engine.eval_script(source, "valor://eval_js_test")?;
        self.js_engine.run_jobs()?;
        Ok(())
    }

    /// Return a Stylesheet snapshot from the CSS mirror.
    ///
    /// # Errors
    ///
    /// Returns an error if CSS mirror synchronization fails.
    pub fn styles_snapshot(&mut self) -> Result<Stylesheet, Error> {
        // For blocking-thread callers, keep it non-async
        self.css_mirror.try_update_sync()?;
        Ok(self.css_mirror.mirror_mut().styles().clone())
    }

    // Internal accessors for sibling modules (events, etc.).
    /// Return a snapshot of the layouter's current geometry per node.
    pub fn layouter_geometry_mut(&mut self) -> HashMap<NodeKey, LayoutRect> {
        self.layouter_mirror.mirror_mut().compute_layout_geometry()
    }
    /// Clone and return the layouter's current attributes map (id/class/style) keyed by `NodeKey`.
    pub fn layouter_attrs_map(&mut self) -> HashMap<NodeKey, HashMap<String, String>> {
        self.layouter_mirror.mirror_mut().attrs_map()
    }
    /// Ensure a layout pass has been run at least once or if material dirt is pending.
    /// This synchronous helper is intended for display snapshot code paths that
    /// cannot await the normal async update loop but need non-empty geometry.
    pub fn ensure_layout_now(&mut self) {
        let lay = self.layouter_mirror.mirror_mut();
        let need = lay.compute_layout_geometry().is_empty() || lay.has_material_dirty();
        if need {
            let _unused = lay.compute_layout();
            // Mark that a redraw would be needed if a UI were present.
            self.needs_redraw = true;
        }
    }

    /// Return a structure-only snapshot for tests: (`tags_by_key`, `element_children`).
    /// This is derived from the DOM/layout snapshot the page can produce and does not
    /// depend on any internal layouter mirrors.
    pub fn layout_structure_snapshot(&mut self) -> (TagsByKey, ElementChildren) {
        // Ensure DOM is drained to get the latest nodes
        let _unused = self.dom_index_mirror.try_update_sync();
        let (tags_by_key, element_children, _raw_children, _text_by_key) =
            self.snapshot_layout_maps();
        (tags_by_key, element_children)
    }

    /// Initialize a late `Layouter` subscriber (external mirror) from the current page state.
    /// Replays the existing element structure and attributes into the provided mirror so it
    /// can participate in layout and serialization even if it subscribed after parsing began.
    /// Initialize a late `Layouter` subscriber from the current page state.
    ///
    /// # Panics
    ///
    /// Panics if a parent key is missing from the children map.
    pub fn bootstrap_layouter_subscriber(&mut self, mirror: &mut DOMMirror<Layouter>) {
        fn apply_attrs(
            lay: &mut Layouter,
            node: NodeKey,
            attrs: &HashMap<NodeKey, HashMap<String, String>>,
        ) {
            let Some(map) = attrs.get(&node) else {
                return;
            };
            for key_name in ["id", "class", "style"] {
                if let Some(value) = map.get(key_name) {
                    let _unused = lay.apply_update(SetAttr {
                        node,
                        name: key_name.to_owned(),
                        value: value.clone(),
                    });
                }
            }
        }

        fn replay(
            lay: &mut Layouter,
            tags_by_key: &HashMap<NodeKey, String>,
            element_children: &HashMap<NodeKey, Vec<NodeKey>>,
            attrs: &HashMap<NodeKey, HashMap<String, String>>,
            parent: NodeKey,
        ) {
            let Some(children) = element_children.get(&parent) else {
                return;
            };
            for child in children {
                let tag = tags_by_key
                    .get(child)
                    .cloned()
                    .unwrap_or_else(|| String::from("div"));
                let _unused = lay.apply_update(InsertElement {
                    parent,
                    node: *child,
                    tag,
                    pos: 0,
                });
                apply_attrs(lay, *child, attrs);
                replay(lay, tags_by_key, element_children, attrs, *child);
            }
        }

        // Take a structural snapshot and attrs from the internal layouter mirror
        let snapshot = self.layouter_mirror.mirror().snapshot();
        let attrs_map = self.layouter_mirror.mirror_mut().attrs_map();

        // Build tags_by_key and element_children from the snapshot
        let mut tags_by_key: HashMap<NodeKey, String> = HashMap::new();
        let mut children_tmp: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();
        for (key, kind, children) in snapshot {
            if let LayoutNodeKind::Block { tag } = kind {
                tags_by_key.insert(key, tag);
            }
            children_tmp.insert(key, children);
        }
        let mut element_children: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();
        // Iterate in a deterministic order by collecting and sorting keys
        let mut sorted_keys: Vec<_> = children_tmp.keys().copied().collect();
        sorted_keys.sort_by_key(|key| key.0);
        for parent in sorted_keys {
            let Some(kids) = children_tmp.remove(&parent) else {
                continue;
            };
            let filtered: Vec<NodeKey> = kids
                .into_iter()
                .filter(|child| tags_by_key.contains_key(child))
                .collect();
            element_children.insert(parent, filtered);
        }

        let lay = mirror.mirror_mut();
        replay(
            lay,
            &tags_by_key,
            &element_children,
            &attrs_map,
            NodeKey::ROOT,
        );
        let _unused = lay.apply_update(EndOfDocument);
    }
    /// Perform hit testing at the given coordinates.
    pub(crate) const fn layouter_hit_test(&mut self, x: i32, y: i32) -> Option<NodeKey> {
        self.layouter_mirror.mirror_mut().hit_test(x, y)
    }

    /// Drain mirrors and return a snapshot clone of computed styles per node.
    /// Ensures the latest inline `<style>` collected by `CSSMirror` is forwarded to the
    /// engine before taking the snapshot so callers that query immediately after
    /// parsing completes (without another update tick) still see up-to-date styles.
    ///
    /// # Errors
    ///
    /// Returns an error if CSS synchronization or style processing fails.
    pub fn computed_styles_snapshot(&mut self) -> Result<HashMap<NodeKey, ComputedStyle>, Error> {
        // Ensure the latest inline <style> and orchestrator state are reflected
        self.css_mirror.try_update_sync()?;
        // Ensure the Orchestrator mirror has applied all pending DOM updates before processing
        self.orchestrator_mirror.try_update_sync()?;
        let sheet = self.css_mirror.mirror_mut().styles().clone();
        self.orchestrator_mirror
            .mirror_mut()
            .replace_stylesheet(&sheet);
        let artifacts = self.orchestrator_mirror.mirror_mut().process_once()?;
        Ok(artifacts.computed_styles)
    }

    /// Drain CSS mirror and return a snapshot clone of discovered external stylesheet URLs.
    ///
    /// # Errors
    ///
    /// Returns an error if CSS mirror synchronization fails.
    pub fn discovered_stylesheets_snapshot(&mut self) -> Result<Vec<String>, Error> {
        self.css_mirror.try_update_sync()?;
        let sheets = self.css_mirror.mirror_mut().discovered_stylesheets();
        Ok(sheets)
    }

    /// Return a JSON string with key performance counters from the layouter to aid diagnostics (Phase 8).
    pub fn perf_counters_snapshot_string(&mut self) -> String {
        let _unused = self.layouter_mirror.try_update_sync();
        let lay = self.layouter_mirror.mirror_mut();
        let counters = PerfCounters {
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

    /// Emit production-friendly telemetry (JSON) when enabled in `ValorConfig`.
    /// This prints a single-line JSON record per tick with core Phase 8 counters.
    /// Intended for external tooling to scrape logs; kept opt-in to avoid overhead.
    pub fn emit_perf_telemetry_if_enabled(&mut self) {
        telemetry_mod::maybe_emit(
            self.telemetry_enabled,
            &self.perf_counters_snapshot_string(),
        );
    }

    /// Return the JSON snapshot of the current DOM tree.
    pub fn dom_json_snapshot_string(&self) -> String {
        self.dom.to_json_string()
    }

    /// Attach a privileged chromeHost command channel to this page (for `valor://chrome` only).
    /// This installs the `chromeHost` namespace into the JS context with origin gating.
    ///
    /// # Errors
    ///
    /// Returns an error if binding installation fails.
    pub fn attach_chrome_host(
        &mut self,
        sender: UnboundedSender<ChromeHostCommand>,
    ) -> Result<(), Error> {
        self.host_context.chrome_host_tx = Some(sender);
        // Install the chromeHost namespace now that a channel is available
        let bindings = build_chrome_host_bindings();
        let _unused = self
            .js_engine
            .install_bindings(&self.host_context, &bindings);
        let _unused2 = self.js_engine.run_jobs();
        Ok(())
    }

    /// Return the `NodeKey` of an element by id using the DOM index, if present.
    pub fn get_element_by_id(&mut self, id: &str) -> Option<NodeKey> {
        let _unused = self.dom_index_mirror.try_update_sync();
        let dom_index = self.dom_index_shared.lock().ok()?;
        dom_index.get_element_by_id(id)
    }

    /// Return the currently focused node, if any.
    #[inline]
    pub const fn focused_node(&self) -> Option<NodeKey> {
        self.focused_node
    }

    /// Set the focused node explicitly.
    #[inline]
    pub const fn focus_set(&mut self, node: Option<NodeKey>) {
        self.focused_node = node;
    }

    /// Move focus to the next focusable element using a basic tabindex order, then natural order fallback.
    #[inline]
    pub fn focus_next(&mut self) -> Option<NodeKey> {
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
    #[inline]
    pub fn focus_prev(&mut self) -> Option<NodeKey> {
        if self.layouter_mirror.try_update_sync().is_err() {
            return None;
        }
        let snapshot = self.layouter_mirror.mirror_mut().snapshot();
        let attrs = self.layouter_mirror.mirror_mut().attrs_map();
        let prev = focus_mod::prev(&snapshot, &attrs, self.focused_node);
        self.focused_node = prev;
        prev
    }

    /// Get the background color from the page's computed styles.
    #[allow(dead_code, reason = "Public API used by valor crate")]
    pub const fn background_rgba(&self) -> [f32; 4] {
        [1.0, 1.0, 1.0, 1.0]
    }

    /// Get a retained snapshot of the display list.
    ///
    /// # Errors
    ///
    /// Returns an error if display list generation fails.
    pub fn display_list_retained_snapshot(&mut self) -> Result<DisplayList, Error> {
        let _unused = self.layouter_mirror.try_update_sync();
        let _unused2 = self.renderer_mirror.try_update_sync();

        let layouter = self.layouter_mirror.mirror_mut();
        let rects = layouter.compute_layout_geometry();
        let styles = layouter.computed_styles();

        let mut items = Vec::new();

        // Simple approach: iterate through all rects and draw backgrounds
        // This gets basic rendering working; proper z-order and clipping can be added later
        for (key, rect) in &rects {
            if let Some(style) = styles.get(key) {
                let background = &style.background_color;
                if background.alpha > 0 {
                    items.push(DisplayItem::Rect {
                        x: rect.x,
                        y: rect.y,
                        width: rect.width,
                        height: rect.height,
                        color: [
                            f32::from(background.red) / 255.0,
                            f32::from(background.green) / 255.0,
                            f32::from(background.blue) / 255.0,
                            f32::from(background.alpha) / 255.0,
                        ],
                    });
                }
            }
        }

        info!(
            "display_list_retained_snapshot: generated {} items from {} rects",
            items.len(),
            rects.len()
        );
        Ok(DisplayList::from_items(items))
    }

    /// Dispatch a pointer move event.
    #[allow(dead_code, reason = "Public API used by valor crate")]
    pub const fn dispatch_pointer_move(&mut self, _x: f64, _y: f64) {
        // Pointer move handling
    }

    /// Dispatch a pointer down event.
    #[allow(dead_code, reason = "Public API used by valor crate")]
    pub const fn dispatch_pointer_down(&mut self, _x: f64, _y: f64, _button: u32) {
        // Pointer down handling
    }

    /// Dispatch a pointer up event.
    #[allow(dead_code, reason = "Public API used by valor crate")]
    pub const fn dispatch_pointer_up(&mut self, _x: f64, _y: f64, _button: u32) {
        // Pointer up handling
    }

    /// Dispatch a key down event.
    #[allow(dead_code, reason = "Public API used by valor crate")]
    pub const fn dispatch_key_down(&mut self, _key: &str, _code: &str, _mods: KeyMods) {
        // Key down handling
    }

    /// Dispatch a key up event.
    #[allow(dead_code, reason = "Public API used by valor crate")]
    pub const fn dispatch_key_up(&mut self, _key: &str, _code: &str, _mods: KeyMods) {
        // Key up handling
    }

    /// Set the current text selection overlay rectangle in viewport coordinates.
    /// Pass the two corners of the selection (order does not matter). Use `selection_clear()` to remove.
    #[inline]
    pub const fn selection_set(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        self.selection_overlay = Some((x0, y0, x1, y1));
    }

    /// Clear any active text selection overlay.
    #[inline]
    pub const fn selection_clear(&mut self) {
        self.selection_overlay = None;
    }

    /// Return a list of selection rectangles by intersecting inline text boxes with a selection rect.
    #[inline]
    pub fn selection_rects(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<LayoutRect> {
        if self.layouter_mirror.try_update_sync().is_err() {
            return Vec::new();
        }
        let rects = self.layouter_mirror.mirror_mut().compute_layout_geometry();
        let snapshot = self.layouter_mirror.mirror_mut().snapshot();
        selection::selection_rects(&rects, &snapshot, (x0, y0, x1, y1))
    }

    /// Compute a caret rectangle at the given point: a thin bar within the inline text box, if any.
    #[inline]
    pub fn caret_at(&mut self, x: i32, y: i32) -> Option<LayoutRect> {
        if self.layouter_mirror.try_update_sync().is_err() {
            return None;
        }
        let rects = self.layouter_mirror.mirror_mut().compute_layout_geometry();
        let snapshot = self.layouter_mirror.mirror_mut().snapshot();
        let hit = self.layouter_hit_test(x, y);
        selection::caret_at(&rects, &snapshot, x, y, hit)
    }

    /// Return a minimal Accessibility (AX) tree snapshot as JSON.
    #[inline]
    pub fn ax_tree_snapshot_string(&mut self) -> String {
        if self.layouter_mirror.try_update_sync().is_err() {
            return String::from("{\"role\":\"document\"}");
        }
        let snapshot = self.layouter_mirror.mirror_mut().snapshot();
        let attrs_map = self.layouter_mirror.mirror_mut().attrs_map();
        ax_tree_snapshot_from(snapshot, &attrs_map)
    }

    /// Performance counters from the internal Layouter mirror: nodes reflowed in the last layout.
    #[inline]
    pub const fn layouter_perf_nodes_reflowed_last(&mut self) -> u64 {
        self.layouter_mirror.mirror_mut().perf_nodes_reflowed_last()
    }

    /// Performance counters from the internal Layouter mirror: number of dirty subtrees processed last.
    #[inline]
    pub const fn layouter_perf_dirty_subtrees_last(&mut self) -> u64 {
        self.layouter_mirror.mirror_mut().perf_dirty_subtrees_last()
    }
}
