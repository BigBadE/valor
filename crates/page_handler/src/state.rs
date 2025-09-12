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
use wgpu_renderer::{Renderer, DrawRect, DrawText, DisplayList, DisplayItem};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::info_span;

/// Simple frame scheduler to coalesce layout work per frame with a given time budget.
struct FrameScheduler {
    budget: Duration,
    last_frame_start: Option<Instant>,
    /// Number of times a layout request was deferred due to frame budget limits (spillover).
    deferred_count: u64,
}

impl FrameScheduler {
    fn new(budget: Duration) -> Self { Self { budget, last_frame_start: None, deferred_count: 0 } }
    /// Returns true if a new frame budget window has started and we can run layout now.
    fn allow(&mut self) -> bool {
        let now = Instant::now();
        match self.last_frame_start {
            None => { self.last_frame_start = Some(now); true }
            Some(start) => {
                if now.duration_since(start) >= self.budget {
                    self.last_frame_start = Some(now);
                    true
                } else {
                    false
                }
            }
        }
    }
    /// Increment the number of deferred layout attempts due to frame budgeting.
    fn incr_deferred(&mut self) { self.deferred_count = self.deferred_count.saturating_add(1); }
    /// Return the number of times layout was deferred due to budgeting during this session.
    fn deferred(&self) -> u64 { self.deferred_count }
}

pub struct HtmlPage {
    /// Optional currently focused node for Phase 7 focus management.
    focused_node: Option<js::NodeKey>,
    /// Optional active selection rectangle in viewport coordinates for highlight overlay (Phase 6 text selection highlight).
    selection_overlay: Option<(i32, i32, i32, i32)>,
    // If none, loading is finished. If some, still streaming.
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
    script_rx: UnboundedReceiver<String>,
    script_counter: u64,
    #[allow(dead_code)]
    url: Url,
    // Optional layout debounce to coalesce micro-changes across updates (Phase 4 start)
    layout_debounce: Option<std::time::Duration>,
    layout_debounce_deadline: Option<std::time::Instant>,
    // Frame scheduler to coalesce layout per frame with a budget (Phase 5)
    frame_scheduler: FrameScheduler,
    /// Diagnostics: number of nodes restyled in the last tick (Phase 8).
    last_style_restyled_nodes: u64,
    // Whether we've dispatched DOMContentLoaded to JS listeners.
    dom_content_loaded_fired: bool,
}

impl HtmlPage {
    /// Create a new HtmlPage by streaming the content from the given URL
    pub async fn new(handle: &Handle, url: Url) -> Result<Self, Error> {
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
        let (script_tx, script_rx) = unbounded_channel();
        let loader = HTMLParser::parse(
            handle,
            in_updater.clone(),
            keyman,
            stream_url(&url).await?,
            out_receiver,
            script_tx,
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
        
        // Optional debounce from env var (milliseconds)
        let layout_debounce = std::env::var("VALOR_LAYOUT_DEBOUNCE_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .and_then(|ms| if ms > 0 { Some(std::time::Duration::from_millis(ms)) } else { None });

        // Create the JS engine
        let mut js_engine = V8Engine::new().map_err(|e| anyhow!("failed to init V8Engine: {}", e))?;
        // Install direct host bindings (e.g., console + document) from the js crate
        let logger = Arc::new(js::ConsoleLogger);
        // Prepare JS context wiring for DOM manipulation functions
        let js_node_keys = std::sync::Arc::new(std::sync::Mutex::new(js_keyman));
        let js_local_id_counter = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let js_created_nodes = std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        let host_context = js::HostContext {
            page_id: None,
            logger,
            dom_sender: in_updater.clone(),
            js_node_keys,
            js_local_id_counter,
            js_created_nodes,
            dom_index: dom_index_shared.clone(),
        };
        let bindings = js::build_default_bindings();
        let _ = js_engine.install_bindings(host_context, &bindings);

        // Frame scheduler budget (ms), default to ~16ms per 60Hz frame
        let frame_budget_ms = std::env::var("VALOR_FRAME_BUDGET_MS").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(16);
        let frame_scheduler = FrameScheduler::new(Duration::from_millis(frame_budget_ms.max(1)));

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
            script_rx,
            script_counter: 0,
            url,
            layout_debounce,
            layout_debounce_deadline: None,
            frame_scheduler,
            last_style_restyled_nodes: 0,
            dom_content_loaded_fired: false,
        })
    }

    /// Returns true once parsing has fully finalized and the loader has been consumed.
    /// This becomes true only after an update() call has observed the parser finished
    /// and awaited its completion.
    pub fn parsing_finished(&self) -> bool {
        self.loader.is_none()
    }

    /// Execute any pending inline scripts from the parser
    fn execute_pending_scripts(&mut self) {
        loop {
            match self.script_rx.try_recv() {
                Ok(script_source) => {
                    let script_url = format!("inline:script-{}", self.script_counter);
                    self.script_counter = self.script_counter.wrapping_add(1);
                    // Test printing: log inline script execution
                    info!("HtmlPage: executing {} (length={} bytes)", script_url, script_source.len());
                    let _ = self.js_engine.eval_script(&script_source, &script_url);
                    let _ = self.js_engine.run_jobs();
                }
                Err(UnboundedTryRecvError::Empty) => break,
                Err(UnboundedTryRecvError::Disconnected) => break,
            }
        }
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
    async fn handle_dom_content_loaded_if_needed(&mut self) -> Result<(), Error> {
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
        // Drain DOM index mirror to keep getElement* lookups up-to-date (non-blocking in tests)
        self.dom_index_mirror.try_update_sync()?;
        // Drain CSS mirror after DOM broadcast (non-blocking)
        self.css_mirror.try_update_sync()?;

        // Forward current stylesheet to style engine and drain it
        let current_styles = self.css_mirror.mirror_mut().styles().clone();
        self.style_engine_mirror.mirror_mut().replace_stylesheet(current_styles.clone());
        // Drain DOM-driven style updates first (non-blocking)
        self.style_engine_mirror.try_update_sync()?;
        // Coalesce and recompute dirty styles once per tick after draining updates
        self.style_engine_mirror.mirror_mut().recompute_dirty();

        // Forward current stylesheet and computed styles to layouter only when styles changed
        let style_changed = self.style_engine_mirror.mirror_mut().take_and_clear_style_changed();
        // Drain changed_nodes now to compute effective change
        let mut changed_nodes = Vec::new();
        if style_changed {
            changed_nodes = self.style_engine_mirror.mirror_mut().take_changed_nodes();
            if !changed_nodes.is_empty() {
                // Provide computed styles and stylesheet snapshot only when there are material changes
                let computed_styles = self.style_engine_mirror.mirror_mut().computed_snapshot();
                self.layouter_mirror.mirror_mut().set_stylesheet(current_styles);
                self.layouter_mirror.mirror_mut().set_computed_styles(computed_styles);
                // Mark nodes whose computed styles changed as STYLE-dirty in layouter
                self.layouter_mirror.mirror_mut().mark_nodes_style_dirty(&changed_nodes);
            }
        }
        // Drain layouter updates after DOM broadcast (non-blocking)
        self.layouter_mirror.try_update_sync()?;

        // Record restyled node count for diagnostics (Phase 8)
        self.last_style_restyled_nodes = if style_changed { changed_nodes.len() as u64 } else { 0 };

        Ok(style_changed && !changed_nodes.is_empty())
    }

    /// Compute layout with debouncing logic and forward dirty rectangles to renderer
    fn compute_layout_with_debouncing(&mut self, style_changed: bool) -> Result<(), Error> {
        let _span = info_span!("page.compute_layout_with_debouncing").entered();
        // Determine if layout should run based on actual style or material layouter changes
        let has_material_dirty = self.layouter_mirror.mirror_mut().has_material_dirty();
        let mut should_layout = style_changed || has_material_dirty;
        
        // Optional debounce: coalesce micro-changes if configured
        if should_layout {
            if let Some(debounce_duration) = self.layout_debounce {
                let current_time = std::time::Instant::now();
                match self.layout_debounce_deadline {
                    None => {
                        self.layout_debounce_deadline = Some(current_time + debounce_duration);
                        // Defer layout this tick
                        should_layout = false;
                        trace!("Debouncing layout for {:?}", debounce_duration);
                    }
                    Some(deadline) => {
                        if current_time < deadline {
                            should_layout = false;
                        } else {
                            self.layout_debounce_deadline = None;
                        }
                    }
                }
            }
        }
        
        if should_layout {
            // Respect frame budget: run layout at most once per frame window
            if !self.frame_scheduler.allow() {
                trace!("Layout skipped due to frame budget ({:?})", self.frame_scheduler.budget);
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
            }
        } else {
            trace!("Layout skipped: no DOM/style changes in this tick");
            // Reset last-tick perf counters so observability reflects the no-op
            self.layouter_mirror.mirror_mut().mark_noop_layout_tick();
        }
        
        Ok(())
    }

    pub async fn update(&mut self) -> Result<(), Error> {
        let _span = info_span!("page.update").entered();
        // Drain and execute any pending inline scripts from the parser
        self.execute_pending_scripts();

        // Finalize DOM loading if the loader has finished
        self.finalize_dom_loading_if_needed().await?;

        // Apply any pending DOM updates
        self.dom.update().await?;
        // Keep the DOM index mirror in sync before any JS queries (e.g., getElementById)
        self.dom_index_mirror.try_update_sync()?;

        // Handle DOM content loaded event if needed
        self.handle_dom_content_loaded_if_needed().await?;

        // Process CSS and style updates
        let style_changed = self.process_css_and_styles().await?;
        
        // Compute layout with debouncing and forward dirty rectangles
        self.compute_layout_with_debouncing(style_changed)?;

        // Drain renderer mirror after DOM broadcast so the scene graph stays in sync (non-blocking)
        self.renderer_mirror.try_update_sync()?;
        // Emit optional production telemetry for this tick (env: VALOR_TELEMETRY=1)
        self.emit_perf_telemetry_if_enabled();
        Ok(())
    }

    pub fn create_mirror<T: DOMSubscriber>(&self, mirror: T) -> DOMMirror<T> {
        DOMMirror::new(self.in_updater.clone(), self.dom.subscribe(), mirror)
    }

    /// Drain CSS mirror and return a snapshot clone of the collected stylesheet
    pub fn styles_snapshot(&mut self) -> Result<Stylesheet, Error> {
        // For blocking-thread callers, keep it non-async
        self.css_mirror.try_update_sync()?;
        Ok(self.css_mirror.mirror_mut().styles().clone())
    }

    /// Drain StyleEngine mirror and return a snapshot clone of computed styles per node.
    pub fn computed_styles_snapshot(&mut self) -> Result<std::collections::HashMap<js::NodeKey, style_engine::ComputedStyle>, Error> {
        self.style_engine_mirror.try_update_sync()?;
        Ok(self.style_engine_mirror.mirror_mut().computed_snapshot())
    }

    /// Drain CSS mirror and return a snapshot clone of discovered external stylesheet URLs
    pub fn discovered_stylesheets_snapshot(&mut self) -> Result<Vec<String>, Error> {
        self.css_mirror.try_update_sync()?;
        Ok(self.css_mirror.mirror_mut().discovered_stylesheets().to_vec())
    }

    /// Return a JSON string with key performance counters from the layouter to aid diagnostics (Phase 8).
    pub fn perf_counters_snapshot_string(&mut self) -> String {
        // Best-effort; never fail
        let _ = self.layouter_mirror.try_update_sync();
        let lay = self.layouter_mirror.mirror_mut();
        let nodes_last = lay.perf_nodes_reflowed_last();
        let nodes_total = lay.perf_nodes_reflowed_total();
        let dirty_last = lay.perf_dirty_subtrees_last();
        let time_last = lay.perf_layout_time_last_ms();
        let time_total = lay.perf_layout_time_total_ms();
        let restyled_last = self.last_style_restyled_nodes;
        let spillover = self.frame_scheduler.deferred();
        let line_boxes_last = lay.perf_line_boxes_last();
        let shaped_runs_last = lay.perf_shaped_runs_last();
        let early_outs_last = lay.perf_early_outs_last();
        format!(
            "{{\"nodes_reflowed_last\":{},\"nodes_reflowed_total\":{},\"dirty_subtrees_last\":{},\"layout_time_last_ms\":{},\"layout_time_total_ms\":{},\"restyled_nodes_last\":{},\"spillover_deferred\":{},\"line_boxes_last\":{},\"shaped_runs_last\":{},\"early_outs_last\":{}}}",
            nodes_last, nodes_total, dirty_last, time_last, time_total, restyled_last, spillover, line_boxes_last, shaped_runs_last, early_outs_last
        )
    }

    /// Emit production-friendly telemetry (JSON) when VALOR_TELEMETRY=1 is set in the environment.
    /// This prints a single-line JSON record per tick with core Phase 8 counters.
    /// Intended for external tooling to scrape logs; kept opt-in to avoid overhead.
    pub fn emit_perf_telemetry_if_enabled(&mut self) {
        if std::env::var("VALOR_TELEMETRY").ok().as_deref() == Some("1") {
            let json = self.perf_counters_snapshot_string();
            println!("{}", json);
        }
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
        // Ensure our mirrors have processed pending updates for a consistent snapshot
        self.layouter_mirror.try_update_sync()?;
        // Compute geometry for all nodes
        let rects = self.layouter_mirror.mirror_mut().compute_layout_geometry();
        // Access computed styles (already forwarded to layouter in update())
        let _computed_map = self.layouter_mirror.mirror_mut().computed_styles().clone();
        // Use stable, document-order traversal from the layouter snapshot to avoid flicker
        let snapshot = self.layouter_mirror.mirror_mut().snapshot();

        let mut list: Vec<DrawRect> = Vec::new();
        for (node, kind, _children) in snapshot.into_iter() {
            // Only draw boxes for block elements; skip inline text and document nodes
            if !matches!(kind, layouter::LayoutNodeKind::Block { .. }) { continue; }
            if let Some(rect) = rects.get(&node) {
                // Use white as the default fill until background-color is implemented
                let color = [1.0, 1.0, 1.0];
                list.push(DrawRect {
                    x: rect.x as f32,
                    y: rect.y as f32,
                    width: rect.width as f32,
                    height: rect.height as f32,
                    color,
                });
            }
        }
        Ok(list)
    }

    /// Build a simple text display list from inline text nodes, using their geometry and computed styles.
    /// Build a retained display list combining both rectangle and text items.
    /// Adds basic clipping scopes based on computed overflow (overflow:hidden ⇒ clip to border box).
    /// Traverses the layout tree in document order to emit items and balanced clips.
    /// Build a retained display list combining rectangles, text, and overlays.
    /// If a selection overlay is set via selection_set(), semi-transparent highlight
    /// quads are emitted for intersected inline text boxes. A simple focus ring is
    /// drawn when a focused element exists. Optionally, a small perf HUD text is
    /// added when VALOR_HUD=1.
    pub fn display_list_retained_snapshot(&mut self) -> Result<DisplayList, Error> {
        // Drain mirrors for consistency
        self.layouter_mirror.try_update_sync()?;
        // Gather data we need from layouter and styles
        let rects = self.layouter_mirror.mirror_mut().compute_layout_geometry();
        let snapshot = self.layouter_mirror.mirror_mut().snapshot();
        let computed_map = self.layouter_mirror.mirror_mut().computed_styles().clone();

        // Build maps for quick lookup
        let mut kind_map: std::collections::HashMap<js::NodeKey, layouter::LayoutNodeKind> = std::collections::HashMap::new();
        let mut children_map: std::collections::HashMap<js::NodeKey, Vec<js::NodeKey>> = std::collections::HashMap::new();
        for (key, kind, children) in snapshot.into_iter() {
            kind_map.insert(key, kind);
            children_map.insert(key, children);
        }

        // removed helper; compute per-node background color inline

        fn push_text_item(list: &mut DisplayList, rect: &layouter::LayoutRect, text: &str, font_size: f32, color_rgb: [f32; 3]) {
            let collapsed = layouter::layout::collapse_whitespace(text);
            let display_text = layouter::layout::reorder_bidi_for_display(&collapsed);
            list.push(DisplayItem::Text { x: rect.x as f32, y: rect.y as f32, text: display_text, color: color_rgb, font_size });
        }

        fn recurse(
            list: &mut DisplayList,
            node: js::NodeKey,
            kind_map: &std::collections::HashMap<js::NodeKey, layouter::LayoutNodeKind>,
            children_map: &std::collections::HashMap<js::NodeKey, Vec<js::NodeKey>>,
            rects: &std::collections::HashMap<js::NodeKey, layouter::LayoutRect>,
            computed_map: &std::collections::HashMap<js::NodeKey, style_engine::ComputedStyle>,
        ) {
            let kind = match kind_map.get(&node) { Some(k) => k, None => return };
            match kind {
                layouter::LayoutNodeKind::Document => {
                    // Visit children only
                    if let Some(children) = children_map.get(&node) {
                        for &child in children { recurse(list, child, kind_map, children_map, rects, computed_map); }
                    }
                }
                layouter::LayoutNodeKind::Block { .. } => {
                    if let Some(rect) = rects.get(&node) {
                        // Background fill from computed styles (with alpha)
                        let mut rgba: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
                        if let Some(cs) = computed_map.get(&node) {
                            let bg = cs.background_color;
                            rgba = [
                                bg.red as f32 / 255.0,
                                bg.green as f32 / 255.0,
                                bg.blue as f32 / 255.0,
                                bg.alpha as f32 / 255.0,
                            ];
                        }
                        if rgba[3] > 0.0 {
                            list.push(DisplayItem::Rect {
                                x: rect.x as f32,
                                y: rect.y as f32,
                                width: rect.width as f32,
                                height: rect.height as f32,
                                color: rgba,
                            });
                        }
                        // If overflow is hidden, start a clip scope for descendants
                        let mut opened_clip = false;
                        if let Some(cs) = computed_map.get(&node) {
                            if matches!(cs.overflow, style_engine::Overflow::Hidden) {
                                list.push(DisplayItem::BeginClip { x: rect.x as f32, y: rect.y as f32, width: rect.width as f32, height: rect.height as f32 });
                                opened_clip = true;
                            }
                        }
                        if let Some(children) = children_map.get(&node) {
                            for &child in children { recurse(list, child, kind_map, children_map, rects, computed_map); }
                        }
                        if opened_clip { list.push(DisplayItem::EndClip); }
                    }
                }
                layouter::LayoutNodeKind::InlineText { text } => {
                    if text.trim().is_empty() { return; }
                    if let Some(rect) = rects.get(&node) {
                        let (font_size, color_rgb) = if let Some(cs) = computed_map.get(&node) {
                            let c = cs.color; (cs.font_size, [c.red as f32 / 255.0, c.green as f32 / 255.0, c.blue as f32 / 255.0])
                        } else { (16.0, [0.0, 0.0, 0.0]) };
                        push_text_item(list, rect, text, font_size, color_rgb);
                    }
                }
            }
        }

        let mut list = DisplayList::new();
        recurse(&mut list, js::NodeKey::ROOT, &kind_map, &children_map, &rects, &computed_map);

        // Selection highlight overlay (Phase 6 Visual Fidelity):
        if let Some((x0, y0, x1, y1)) = self.selection_overlay {
            let highlights = self.selection_rects(x0, y0, x1, y1);
            if !highlights.is_empty() {
                let color = [0.2, 0.5, 1.0, 0.35];
                for r in highlights {
                    list.push(DisplayItem::Rect { x: r.x as f32, y: r.y as f32, width: r.width as f32, height: r.height as f32, color });
                }
            }
        }

        // Optional perf HUD (Phase 8 diagnostics kickoff): VALOR_HUD=1
        if std::env::var("VALOR_HUD").ok().as_deref() == Some("1") {
            let lay = self.layouter_mirror.mirror_mut();
            let hud = format!(
                "reflowed:{} restyled:{} dirty:{} last_ms:{} total_ms:{} spill:{} lines:{} runs:{} outs:{}",
                lay.perf_nodes_reflowed_last(),
                self.last_style_restyled_nodes,
                lay.perf_dirty_subtrees_last(),
                lay.perf_layout_time_last_ms(),
                lay.perf_layout_time_total_ms(),
                self.frame_scheduler.deferred(),
                lay.perf_line_boxes_last(),
                lay.perf_shaped_runs_last(),
                lay.perf_early_outs_last()
            );
            list.push(DisplayItem::Text { x: 6.0, y: 14.0, text: hud, color: [0.1, 0.1, 0.1], font_size: 12.0 });
        }

        // Draw a simple focus ring on top if a focused node is present (Phase 7 focus styling)
        if let Some(focused) = self.focused_node {
            if let Some(r) = rects.get(&focused) {
                let x = r.x as f32; let y = r.y as f32; let w = r.width as f32; let h = r.height as f32;
                let c = [0.2, 0.4, 1.0, 1.0];
                let t = 2.0_f32; // thickness
                // top
                list.push(DisplayItem::Rect { x, y, width: w, height: t, color: c });
                // bottom
                list.push(DisplayItem::Rect { x, y: y + h - t, width: w, height: t, color: c });
                // left
                list.push(DisplayItem::Rect { x, y, width: t, height: h, color: c });
                // right
                list.push(DisplayItem::Rect { x: x + w - t, y, width: t, height: h, color: c });
            }
        }
        Ok(list)
    }

    pub fn text_list_snapshot(&mut self) -> Result<Vec<DrawText>, Error> {
        // Drain updates for consistency
        self.layouter_mirror.try_update_sync()?;
        // Gather geometry and snapshot for finding text nodes
        let rects = self.layouter_mirror.mirror_mut().compute_layout_geometry();
        let snapshot = self.layouter_mirror.mirror_mut().snapshot();
        let computed_map = self.layouter_mirror.mirror_mut().computed_styles().clone();

        // Build list in stable snapshot order to avoid flicker
        let mut list: Vec<DrawText> = Vec::new();
        for (key, kind, _children) in snapshot.into_iter() {
            if let layouter::LayoutNodeKind::InlineText { text } = kind {
                if text.trim().is_empty() { continue; }
                if let Some(rect) = rects.get(&key) {
                    let (font_size, color_rgb) = if let Some(cs) = computed_map.get(&key) {
                        let c = cs.color;
                        (cs.font_size, [c.red as f32 / 255.0, c.green as f32 / 255.0, c.blue as f32 / 255.0])
                    } else {
                        (16.0, [0.0, 0.0, 0.0])
                    };
                    // Collapse whitespace to better match inline layout approximation
                    let collapsed = layouter::layout::collapse_whitespace(&text);
                    // Apply bidi reordering for display when shaping is disabled (no-op otherwise)
                    let display_text = layouter::layout::reorder_bidi_for_display(&collapsed);
                    list.push(DrawText {
                        x: rect.x as f32,
                        y: rect.y as f32,
                        text: display_text,
                        color: color_rgb,
                        font_size,
                    });
                }
            }
        }
        Ok(list)
        }

    /// Hit-test screen coordinates against current layout boxes and return the topmost NodeKey.
    /// The hit testing respects overflow:hidden by requiring the point to lie within any
    /// ancestor with overflow hidden. Inline text nodes are considered hittable too.
    pub fn hit_test(&mut self, x: i32, y: i32) -> Option<js::NodeKey> {
        // Drain mirrors to ensure geometry is current
        if self.layouter_mirror.try_update_sync().is_err() { return None; }
        let rects = self.layouter_mirror.mirror_mut().compute_layout_geometry();
        let snapshot = self.layouter_mirror.mirror_mut().snapshot();
        let computed_map = self.layouter_mirror.mirror_mut().computed_styles().clone();
        // Build maps to navigate ancestry and kinds
        let mut parent_by_key: std::collections::HashMap<js::NodeKey, js::NodeKey> = std::collections::HashMap::new();
        for (parent, _kind, children) in snapshot.into_iter() {
            for child in children { parent_by_key.insert(child, parent); }
        }
        // Helper: check if a point lies within node's rect
        let contains_point = |r: &layouter::LayoutRect, px: i32, py: i32| -> bool {
            px >= r.x && py >= r.y && px < r.x + r.width && py < r.y + r.height
        };
        // Helper: ensure the point is visible through overflow:hidden ancestors
        let mut point_visible_through_clips = |node: js::NodeKey| -> bool {
            let mut current = parent_by_key.get(&node).cloned();
            while let Some(ancestor) = current {
                if let Some(cs) = computed_map.get(&ancestor) {
                    if matches!(cs.overflow, style_engine::Overflow::Hidden) {
                        if let Some(ar) = rects.get(&ancestor) {
                            if !contains_point(ar, x, y) { return false; }
                        }
                    }
                }
                current = parent_by_key.get(&ancestor).cloned();
            }
            true
        };
        // We want the topmost match: prefer deeper nodes. We'll scan all keys, compute a depth, and pick max depth.
        let mut best: Option<(usize, js::NodeKey)> = None;
        for (node, rect) in rects.iter() {
            if !contains_point(rect, x, y) { continue; }
            if !point_visible_through_clips(*node) { continue; }
            // Compute depth by walking parents
            let mut depth: usize = 0;
            let mut cur = parent_by_key.get(node).cloned();
            while let Some(p) = cur { depth += 1; cur = parent_by_key.get(&p).cloned(); }
            match best {
                None => best = Some((depth, *node)),
                Some((bd, _)) if depth >= bd => best = Some((depth, *node)),
                _ => {}
            }
        }
        best.map(|(_, k)| k)
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
        let mut focusables: Vec<(i32, js::NodeKey)> = Vec::new();
        let mut natural: Vec<js::NodeKey> = Vec::new();
        for (key, kind, _children) in snapshot.into_iter() {
            // Determine if focusable by tag or tabindex
            let tabindex_opt = attrs.get(&key).and_then(|m| m.get("tabindex")).and_then(|s| s.parse::<i32>().ok());
            let is_focusable_tag = match kind { layouter::LayoutNodeKind::Block { ref tag } => {
                let t = tag.to_ascii_lowercase();
                matches!(t.as_str(), "a" | "button" | "input" | "textarea")
            }, _ => false };
            if let Some(tb) = tabindex_opt { focusables.push((tb, key)); }
            else if is_focusable_tag { natural.push(key); }
        }
        focusables.sort_by_key(|(tb, _)| *tb);
        let order: Vec<js::NodeKey> = if !focusables.is_empty() { focusables.into_iter().map(|(_, k)| k).collect() } else { natural };
        if order.is_empty() { return None; }
        let next = match self.focused_node { None => order[0], Some(cur) => {
            let pos = order.iter().position(|k| *k == cur).unwrap_or(usize::MAX);
            let idx = if pos == usize::MAX || pos + 1 >= order.len() { 0 } else { pos + 1 };
            order[idx]
        }};
        self.focused_node = Some(next);
        Some(next)
    }

    /// Move focus to the previous focusable element.
    pub fn focus_prev(&mut self) -> Option<js::NodeKey> {
        if self.layouter_mirror.try_update_sync().is_err() { return None; }
        let snapshot = self.layouter_mirror.mirror_mut().snapshot();
        let attrs = self.layouter_mirror.mirror_mut().attrs_map();
        let mut focusables: Vec<(i32, js::NodeKey)> = Vec::new();
        let mut natural: Vec<js::NodeKey> = Vec::new();
        for (key, kind, _children) in snapshot.into_iter() {
            let tabindex_opt = attrs.get(&key).and_then(|m| m.get("tabindex")).and_then(|s| s.parse::<i32>().ok());
            let is_focusable_tag = match kind { layouter::LayoutNodeKind::Block { ref tag } => {
                let t = tag.to_ascii_lowercase();
                matches!(t.as_str(), "a" | "button" | "input" | "textarea")
            }, _ => false };
            if let Some(tb) = tabindex_opt { focusables.push((tb, key)); }
            else if is_focusable_tag { natural.push(key); }
        }
        focusables.sort_by_key(|(tb, _)| *tb);
        let order: Vec<js::NodeKey> = if !focusables.is_empty() { focusables.into_iter().map(|(_, k)| k).collect() } else { natural };
        if order.is_empty() { return None; }
        let prev = match self.focused_node { None => order[order.len()-1], Some(cur) => {
            let pos = order.iter().position(|k| *k == cur).unwrap_or(usize::MAX);
            let idx = if pos == usize::MAX || pos == 0 { order.len()-1 } else { pos - 1 };
            order[idx]
        }};
        self.focused_node = Some(prev);
        Some(prev)
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
        let sel_x = x0.min(x1); let sel_y = y0.min(y1); let sel_w = (x0.max(x1) - sel_x).max(0); let sel_h = (y0.max(y1) - sel_y).max(0);
        let selection = layouter::LayoutRect { x: sel_x, y: sel_y, width: sel_w, height: sel_h };
        let mut out: Vec<layouter::LayoutRect> = Vec::new();
        for (key, kind, _children) in snapshot.into_iter() {
            if let layouter::LayoutNodeKind::InlineText { ref text } = kind {
                if text.trim().is_empty() { continue; }
                if let Some(r) = rects.get(&key) {
                    let ix = r.x.max(selection.x);
                    let iy = r.y.max(selection.y);
                    let ix1 = (r.x + r.width).min(selection.x + selection.width);
                    let iy1 = (r.y + r.height).min(selection.y + selection.height);
                    let iw = (ix1 - ix).max(0);
                    let ih = (iy1 - iy).max(0);
                    if iw > 0 && ih > 0 { out.push(layouter::LayoutRect { x: ix, y: iy, width: iw, height: ih }); }
                }
            }
        }
        out
    }

    /// Compute a caret rectangle at the given point: a thin bar within the inline text box, if any.
    pub fn caret_at(&mut self, x: i32, y: i32) -> Option<layouter::LayoutRect> {
        if self.layouter_mirror.try_update_sync().is_err() { return None; }
        let rects = self.layouter_mirror.mirror_mut().compute_layout_geometry();
        let snapshot = self.layouter_mirror.mirror_mut().snapshot();
        let mut parent_by_key: std::collections::HashMap<js::NodeKey, js::NodeKey> = std::collections::HashMap::new();
        for (parent, _kind, children) in snapshot.clone().into_iter() { for child in children { parent_by_key.insert(child, parent); } }
        let hit = self.hit_test(x, y)?;
        // Prefer inline text hits; otherwise find first inline text descendant of the hit node that shares the y row.
        if let Some(r) = rects.get(&hit) {
            // If hit node is inline text, place caret at clamped x within its rect
            if let Some((_k, kind, _)) = snapshot.iter().find(|(k, _, _)| *k == hit) { if let layouter::LayoutNodeKind::InlineText { .. } = kind { 
                let cx = x.clamp(r.x, r.x + r.width);
                return Some(layouter::LayoutRect { x: cx, y: r.y, width: 1, height: r.height });
            }}
        }
        // Fallback: scan inline text rects that contain y and are within the same ancestor chain
        let mut candidate: Option<layouter::LayoutRect> = None;
        for (key, kind, _children) in snapshot.into_iter() {
            if let layouter::LayoutNodeKind::InlineText { .. } = kind {
                if let Some(r) = rects.get(&key) {
                    if y >= r.y && y < r.y + r.height { 
                        let cx = x.clamp(r.x, r.x + r.width);
                        candidate = Some(layouter::LayoutRect { x: cx, y: r.y, width: 1, height: r.height });
                        break;
                    }
                }
            }
        }
        candidate
    }
}




impl HtmlPage {
    /// Return a minimal Accessibility (AX) tree snapshot as JSON.
    /// Roles are inferred from tag names and ARIA attributes; names prefer aria-label or alt; inline text yields a text node.
    pub fn ax_tree_snapshot_string(&mut self) -> String {
        // Best-effort; never fail
        if self.layouter_mirror.try_update_sync().is_err() { return String::from("{\"role\":\"document\"}"); }
        let snapshot = self.layouter_mirror.mirror_mut().snapshot();
        let attrs_map = self.layouter_mirror.mirror_mut().attrs_map();
        // Build maps
        let mut kind_by_key: std::collections::HashMap<js::NodeKey, layouter::LayoutNodeKind> = std::collections::HashMap::new();
        let mut children_by_key: std::collections::HashMap<js::NodeKey, Vec<js::NodeKey>> = std::collections::HashMap::new();
        for (key, kind, children) in snapshot.into_iter() { kind_by_key.insert(key, kind); children_by_key.insert(key, children); }
        fn escape_json(s: &str) -> String { s.replace('\\', "\\\\").replace('"', "\\\"") }
        fn role_for(kind: &layouter::LayoutNodeKind, attrs: &std::collections::HashMap<String, String>) -> &'static str {
            match kind {
                layouter::LayoutNodeKind::Document => "document",
                layouter::LayoutNodeKind::InlineText { .. } => "text",
                layouter::LayoutNodeKind::Block { tag } => {
                    let t = tag.to_ascii_lowercase();
                    if let Some(role) = attrs.get("role") { return Box::leak(role.clone().into_boxed_str()); }
                    match t.as_str() {
                        "a" => "link",
                        "button" => "button",
                        "img" => "img",
                        "input" => "textbox",
                        "textarea" => "textbox",
                        "ul" => "list",
                        "ol" => "list",
                        "li" => "listitem",
                        "h1"|"h2"|"h3"|"h4"|"h5"|"h6" => "heading",
                        _ => "generic",
                    }
                }
            }
        }
        fn name_for(kind: &layouter::LayoutNodeKind, key: js::NodeKey, attrs_map: &std::collections::HashMap<js::NodeKey, std::collections::HashMap<String, String>>) -> String {
            if let Some(attrs) = attrs_map.get(&key) {
                if let Some(v) = attrs.get("aria-label") { return v.clone(); }
                if let Some(v) = attrs.get("alt") { return v.clone(); }
            }
            match kind {
                layouter::LayoutNodeKind::InlineText { text } => {
                    let collapsed = layouter::layout::collapse_whitespace(text);
                    collapsed
                }
                _ => String::new(),
            }
        }
        fn serialize(node: js::NodeKey,
                      kind_by_key: &std::collections::HashMap<js::NodeKey, layouter::LayoutNodeKind>,
                      children_by_key: &std::collections::HashMap<js::NodeKey, Vec<js::NodeKey>>,
                      attrs_map: &std::collections::HashMap<js::NodeKey, std::collections::HashMap<String, String>>) -> String {
            let mut out = String::new();
            let kind = kind_by_key.get(&node).cloned().unwrap_or(layouter::LayoutNodeKind::Document);
            let attrs = attrs_map.get(&node).cloned().unwrap_or_default();
            let role = role_for(&kind, &attrs);
            let name = escape_json(&name_for(&kind, node, attrs_map));
            out.push_str("{\"role\":\""); out.push_str(role); out.push_str("\"");
            if !name.is_empty() { out.push_str(",\"name\":\""); out.push_str(&name); out.push_str("\""); }
            if let Some(children) = children_by_key.get(&node) {
                if !children.is_empty() {
                    out.push_str(",\"children\":[");
                    let mut first = true;
                    for child in children {
                        if !first { out.push(','); } first = false;
                        out.push_str(&serialize(*child, kind_by_key, children_by_key, attrs_map));
                    }
                    out.push(']');
                }
            }
            out.push('}');
            out
        }
        serialize(js::NodeKey::ROOT, &kind_by_key, &children_by_key, &attrs_map)
    }
}
