//! HTML page state management.

mod accessors;
mod dom_processing;
mod events;
mod initialization;
mod js_execution;
mod lifecycle;
mod perf_helpers;
mod rendering;
mod style_layout;

use crate::core::incremental_layout::{IncrementalLayoutEngine, LayoutTreeSnapshot};
use crate::core::pipeline::Pipeline;
use crate::internal::runtime::DefaultJsRuntime;
use crate::internal::runtime::JsRuntime as _;
use crate::utilities::config::ValorConfig;
use crate::utilities::scheduler::FrameScheduler;
use crate::utilities::snapshots::IRect;
use anyhow::Error;
use css::style_types::ComputedStyle;
use css::types::Stylesheet;
use css::{CSSMirror, Orchestrator};
use css_core::LayoutRect;
use html::dom::DOM;
use html::parser::HTMLParser;
use html::parser::ScriptJob;
use js::{
    ChromeHostCommand, DOMMirror, DOMSubscriber, DOMUpdate, DomIndex, HostContext, JsEngine as _,
    ModuleResolver, NodeKey, SharedDomIndex, SimpleFileModuleResolver, build_chrome_host_bindings,
};
type SharedDomIndexRef = SharedDomIndex;
use js_engine_v8::V8Engine;
use log::info;
use renderer::Renderer;
use std::collections::HashMap;
use tokio::runtime::Handle;
use tokio::sync::mpsc::{Sender, UnboundedReceiver, UnboundedSender};
use tracing::info_span;
use url::Url;

/// Structured outcome of a single `update()` tick. Extend as needed.
pub struct UpdateOutcome {
    pub redraw_needed: bool,
}

/// Lifecycle event flags for the page.
#[derive(Default)]
struct LifecycleFlags {
    /// Whether we've dispatched `DOMContentLoaded` to JS listeners.
    dom_content_loaded_fired: bool,
    /// One-time post-load guard to rebuild the `StyleEngine`'s node inventory from the Layouter.
    style_nodes_rebuilt_after_load: bool,
}

/// Rendering and telemetry flags for the page.
#[derive(Default)]
struct RenderFlags {
    /// Whether the last update produced visual changes that require a redraw.
    needs_redraw: bool,
    /// Whether to emit perf telemetry lines per tick.
    telemetry_enabled: bool,
}

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
    /// Renderer mirror that maintains a scene graph from DOM updates.
    renderer_mirror: DOMMirror<Renderer>,
    /// DOM index mirror for JS document.getElement* queries.
    dom_index_mirror: DOMMirror<DomIndex>,
    /// Shared state for DOM index to support synchronous lookups (e.g., getElementById).
    dom_index_shared: SharedDomIndex,
    /// For sending updates to the DOM.
    in_updater: Sender<Vec<DOMUpdate>>,
    /// JavaScript engine and script queue.
    js_engine: V8Engine,
    /// Host context for privileged binding decisions and shared registries.
    host_context: HostContext,
    /// Script receiver for processing script jobs.
    script_rx: UnboundedReceiver<ScriptJob>,
    /// Counter for tracking script execution.
    script_counter: u64,
    /// Current page URL.
    url: Url,
    /// ES module resolver/bundler adapter (JS crate) for side-effect modules.
    module_resolver: Box<dyn ModuleResolver>,
    /// Frame scheduler to coalesce layout per frame with a budget (Phase 5).
    frame_scheduler: FrameScheduler,
    /// Diagnostics: number of nodes restyled in the last tick.
    last_style_restyled_nodes: u64,
    /// Lifecycle event flags for the page.
    lifecycle: LifecycleFlags,
    /// Rendering and telemetry flags for the page.
    render: RenderFlags,
    /// Parallel rendering pipeline for incremental updates
    _pipeline: Pipeline,
    /// Incremental layout engine with fine-grained dependency tracking
    incremental_layout: IncrementalLayoutEngine,
}

impl HtmlPage {
    /// Create a blank `HtmlPage` that will be populated via `DOMUpdate`s.
    /// This is useful for reactive components that generate their content programmatically.
    ///
    /// # Errors
    ///
    /// Returns an error if page initialization fails.
    pub fn new_blank(handle: &Handle, config: ValorConfig) -> Result<Self, Error> {
        let components = initialization::initialize_blank_page(handle, config)?;
        let blank_url = Url::parse("file:///blank")
            .map_err(|err| anyhow::anyhow!("Failed to parse blank URL: {err}"))?;

        Ok(Self {
            focused_node: None,
            selection_overlay: None,
            loader: components.loader,
            dom: components.dom,
            css_mirror: components.mirrors.css_mirror,
            orchestrator_mirror: components.mirrors.orchestrator_mirror,
            renderer_mirror: components.mirrors.renderer_mirror,
            dom_index_mirror: components.mirrors.dom_index_mirror,
            dom_index_shared: components.mirrors.dom_index_shared,
            in_updater: components.in_updater,
            js_engine: components.js_ctx.js_engine,
            host_context: components.js_ctx.host_context,
            script_rx: components.script_rx,
            script_counter: 0,
            url: blank_url,
            module_resolver: Box::new(SimpleFileModuleResolver::new()),
            frame_scheduler: components.frame_scheduler,
            last_style_restyled_nodes: 0,
            lifecycle: LifecycleFlags::default(),
            render: RenderFlags {
                needs_redraw: false,
                telemetry_enabled: components.telemetry_enabled,
            },
            _pipeline: components.pipeline,
            incremental_layout: components.incremental_layout,
        })
    }

    /// Create a new `HtmlPage` by streaming content from the given URL.
    ///
    /// # Errors
    ///
    /// Returns an error if page initialization fails.
    pub async fn new(handle: &Handle, url: Url, config: ValorConfig) -> Result<Self, Error> {
        let components = initialization::initialize_page(handle, url.clone(), config).await?;

        Ok(Self {
            focused_node: None,
            selection_overlay: None,
            loader: components.loader,
            dom: components.dom,
            css_mirror: components.mirrors.css_mirror,
            orchestrator_mirror: components.mirrors.orchestrator_mirror,
            renderer_mirror: components.mirrors.renderer_mirror,
            dom_index_mirror: components.mirrors.dom_index_mirror,
            dom_index_shared: components.mirrors.dom_index_shared,
            in_updater: components.in_updater,
            js_engine: components.js_ctx.js_engine,
            host_context: components.js_ctx.host_context,
            script_rx: components.script_rx,
            script_counter: 0,
            url,
            module_resolver: Box::new(SimpleFileModuleResolver::new()),
            frame_scheduler: components.frame_scheduler,
            last_style_restyled_nodes: 0,
            lifecycle: LifecycleFlags::default(),
            render: RenderFlags {
                needs_redraw: false,
                telemetry_enabled: components.telemetry_enabled,
            },
            _pipeline: components.pipeline,
            incremental_layout: components.incremental_layout,
        })
    }

    /// Returns the current page URL.
    pub const fn url(&self) -> &Url {
        &self.url
    }

    /// Execute any pending inline scripts from the parser
    pub(crate) fn execute_pending_scripts(&mut self) {
        let mut params = js_execution::ExecuteScriptsParams {
            script_rx: &mut self.script_rx,
            script_counter: &mut self.script_counter,
            js_engine: &mut self.js_engine,
            module_resolver: &mut self.module_resolver,
            url: &self.url,
            host_context: &self.host_context,
        };
        js_execution::execute_pending_scripts(&mut params);
    }

    /// Execute at most one due JavaScript timer callback.
    pub(crate) fn tick_js_timers_once(&mut self) {
        js_execution::tick_js_timers_once(&mut self.js_engine);
    }

    /// Synchronously fetch the textContent for an element by id using the `DomIndex` mirror.
    pub fn text_content_by_id_sync(&mut self, id: &str) -> Option<String> {
        accessors::text_content_by_id_sync(id, &mut self.dom_index_mirror, &self.dom_index_shared)
    }

    /// Compute layout if needed.
    fn compute_layout(&mut self, _style_changed: bool) {
        let _span = info_span!("page.compute_layout").entered();

        // Check if incremental engine has dirty nodes or if geometry is empty
        let has_dirty = self.incremental_layout.has_dirty_nodes();
        let geometry_empty = self.incremental_layout.rects().is_empty();
        let should_layout = has_dirty || geometry_empty;
        log::info!("compute_layout: has_dirty={}, geometry_empty={}, should_layout={}", has_dirty, geometry_empty, should_layout);

        if !should_layout && !self.frame_scheduler.allow() {
            self.frame_scheduler.incr_deferred();
            log::trace!("Layout deferred: frame budget not met");
            return;
        }

        let frame_allowed = self.frame_scheduler.allow();
        log::info!("compute_layout: frame_scheduler.allow()={}", frame_allowed);
        
        // Always run layout if we have dirty nodes, bypass frame scheduler
        // The frame scheduler is meant to prevent excessive polling, not to skip dirty work
        if should_layout && (frame_allowed || has_dirty) {
            // Use incremental layout engine
            if let Ok(layout_results) = self.incremental_layout.compute_layouts() {
                // Log layout performance metrics
                let node_count = layout_results.len();
                info!("Layout: processed={node_count} nodes");

                // Request a redraw after any successful layout pass
                self.render.needs_redraw = true;
            }
        } else {
            log::trace!("Layout skipped: no DOM/style changes in this tick");
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
        dom_processing::finalize_dom_loading_if_needed(&mut self.loader).await?;

        // Drive a single JS timers tick via runtime
        let mut runtime = DefaultJsRuntime;
        runtime.tick_timers_once(self);

        // Apply any pending DOM updates and get them for incremental engine
        let dom_updates = self.dom.update()?;
        // Apply DOM updates to incremental engine
        self.incremental_layout.apply_dom_updates(&dom_updates);
        // Keep the DOM index mirror in sync before any JS queries (e.g., getElementById)
        self.dom_index_mirror.try_update_sync()?;
        // Execute pending scripts and DOMContentLoaded sequencing via runtime after DOM drained
        runtime.drive_after_dom_update(self).await?;

        // Process CSS and style updates
        let style_changed = style_layout::process_css_and_styles(
            &mut self.css_mirror,
            &mut self.orchestrator_mirror,
            &mut self.incremental_layout,
            self.loader.as_ref(),
            &mut self.lifecycle.style_nodes_rebuilt_after_load,
        )?;

        // Compute layout and forward dirty rectangles
        self.compute_layout(style_changed);

        // Drain renderer mirror after DOM broadcast so the scene graph stays in sync (non-blocking)
        self.renderer_mirror.try_update_sync()?;
        let outcome = UpdateOutcome {
            redraw_needed: self.render.needs_redraw,
        };
        Ok(outcome)
    }

    pub fn create_mirror<T: DOMSubscriber>(&self, mirror: T) -> DOMMirror<T> {
        DOMMirror::new(self.in_updater.clone(), self.dom.subscribe(), mirror)
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
        if let Err(err) = self.incremental_layout.compute_layouts() {
            tracing::warn!("Failed to compute layouts: {err}");
        }
        self.incremental_layout.rects().clone()
    }

    /// Clone and return the layouter's current attributes map (id/class/style) keyed by `NodeKey`.
    pub fn layouter_attrs_map(&mut self) -> HashMap<NodeKey, HashMap<String, String>> {
        self.incremental_layout.attrs_map().clone()
    }

    /// Get layout snapshot (node, kind, children) for serialization/testing
    pub fn layouter_snapshot(&self) -> LayoutTreeSnapshot {
        accessors::layouter_snapshot(&self.incremental_layout)
    }

    /// Set viewport dimensions for layout computation
    pub fn set_viewport(&mut self, width: i32, height: i32) {
        self.incremental_layout.set_viewport(width, height);
    }

    /// Ensure a layout pass has been run at least once or if material dirt is pending.
    pub fn ensure_layout_now(&mut self) {
        let need = self.incremental_layout.rects().is_empty();
        if need {
            if let Err(err) = self.incremental_layout.compute_layouts() {
                tracing::warn!("Failed to compute layouts: {err}");
            }
            self.render.needs_redraw = true;
        }
    }

    /// Return a structure-only snapshot for tests: (`tags_by_key`, `element_children`).
    pub fn layout_structure_snapshot(&mut self) -> (accessors::TagsMap, accessors::ChildrenMap) {
        accessors::layout_structure_snapshot(&self.incremental_layout, &mut self.dom_index_mirror)
    }

    /// Drain mirrors and return a snapshot clone of computed styles per node.
    ///
    /// # Errors
    ///
    /// Returns an error if CSS synchronization or style processing fails.
    pub fn computed_styles_snapshot(&mut self) -> Result<HashMap<NodeKey, ComputedStyle>, Error> {
        accessors::computed_styles_snapshot(&mut self.css_mirror, &mut self.orchestrator_mirror)
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

    /// Return the JSON snapshot of the current DOM tree.
    pub fn dom_json_snapshot_string(&self) -> String {
        accessors::dom_json_snapshot_string(&self.dom)
    }

    /// Attach a privileged chromeHost command channel to this page (for `valor://chrome` only).
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

    /// Send DOM updates directly to the DOM, bypassing JavaScript.
    /// This allows for instant DOM manipulation from Rust code.
    ///
    /// # Errors
    /// Returns an error if the updates cannot be sent.
    pub fn send_dom_updates(&mut self, updates: Vec<DOMUpdate>) -> Result<(), Error> {
        self.in_updater
            .try_send(updates)
            .map_err(|err| anyhow::anyhow!("Failed to send DOM updates: {err}"))
    }

    /// Get a reference to the shared DOM index for querying DOM structure.
    #[inline]
    pub const fn dom_index_shared(&self) -> &SharedDomIndexRef {
        &self.dom_index_shared
    }

    /// Return a minimal Accessibility (AX) tree snapshot as JSON.
    #[inline]
    pub fn ax_tree_snapshot_string(&mut self) -> String {
        accessors::ax_tree_snapshot_string(&self.incremental_layout)
    }

    /// Synchronously inject CSS text directly into CSSMirror.
    /// This bypasses the async DOMUpdate channel to ensure deterministic ordering.
    ///
    /// # Errors
    /// Returns an error if CSS mirror update fails.
    pub fn inject_css_sync(&mut self, css_text: String) -> Result<(), Error> {
        // CRITICAL: Ensure CSSMirror has processed ALL pending updates from the channel
        // before we inject directly. Otherwise our direct injection might be processed
        // BEFORE the HTML's <style> elements, resulting in incorrect source order.
        self.css_mirror.try_update_sync()?;

        // Use sequential counter for unique IDs
        use std::sync::atomic::{AtomicU32, Ordering};
        static INJECT_COUNTER: AtomicU32 = AtomicU32::new(0);
        let counter = INJECT_COUNTER.fetch_add(1, Ordering::Relaxed);

        // Create unique node keys using very high IDs (0xFFFF_0000 + counter)
        let style_key = NodeKey::pack(0, 0, 0xFFFF_0000_u64 + u64::from(counter) * 2);
        let text_key = NodeKey::pack(0, 0, 0xFFFF_0000_u64 + u64::from(counter) * 2 + 1);

        // Directly apply updates to CSSMirror synchronously
        let updates = vec![
            DOMUpdate::InsertElement {
                parent: NodeKey::ROOT,
                node: style_key,
                tag: String::from("style"),
                pos: usize::MAX,
            },
            DOMUpdate::SetAttr {
                node: style_key,
                name: String::from("data-valor-injected"),
                value: String::from("1"),
            },
            DOMUpdate::InsertText {
                parent: style_key,
                node: text_key,
                text: css_text,
                pos: 0,
            },
        ];

        // Apply directly to CSS mirror bypassing async channel
        for update in &updates {
            self.css_mirror.mirror_mut().apply_update(update.clone())?;
        }

        Ok(())
    }
}
