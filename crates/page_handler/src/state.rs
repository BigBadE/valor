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
use wgpu_renderer::{Renderer, DrawRect, DrawText};
use std::sync::Arc;

pub struct HtmlPage {
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
            dom_index: dom_index_shared,
        };
        let bindings = js::build_default_bindings();
        let _ = js_engine.install_bindings(host_context, &bindings);

        Ok(Self {
            loader: Some(loader),
            dom,
            css_mirror,
            style_engine_mirror,
            layouter_mirror,
            renderer_mirror,
            dom_index_mirror,
            in_updater,
            js_engine,
            script_rx,
            script_counter: 0,
            url,
            layout_debounce,
            layout_debounce_deadline: None,
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
            // Process any DOM changes made by DOMContentLoaded listeners immediately
            self.dom.update().await?;
            // Keep the DOM index mirror in sync after listener-driven changes
            self.dom_index_mirror.update().await?;
        }
        Ok(())
    }

    /// Process CSS and style updates, returning whether styles have changed
    async fn process_css_and_styles(&mut self) -> Result<bool, Error> {
        // Drain DOM index mirror to keep getElement* lookups up-to-date
        self.dom_index_mirror.update().await?;
        // Drain CSS mirror after DOM broadcast
        self.css_mirror.update().await?;

        // Forward current stylesheet to style engine and drain it
        let current_styles = self.css_mirror.mirror_mut().styles().clone();
        self.style_engine_mirror.mirror_mut().replace_stylesheet(current_styles.clone());
        // Drain DOM-driven style updates first
        self.style_engine_mirror.update().await?;
        // Coalesce and recompute dirty styles once per tick after draining updates
        self.style_engine_mirror.mirror_mut().recompute_dirty();

        // Forward current stylesheet and computed styles to layouter only when styles changed
        let style_changed = self.style_engine_mirror.mirror_mut().take_and_clear_style_changed();
        if style_changed {
            // Provide computed styles and stylesheet snapshot
            let computed_styles = self.style_engine_mirror.mirror_mut().computed_snapshot();
            self.layouter_mirror.mirror_mut().set_stylesheet(current_styles);
            self.layouter_mirror.mirror_mut().set_computed_styles(computed_styles);
            // Mark nodes whose computed styles changed as STYLE-dirty in layouter
            let changed_nodes = self.style_engine_mirror.mirror_mut().take_changed_nodes();
            if !changed_nodes.is_empty() {
                self.layouter_mirror.mirror_mut().mark_nodes_style_dirty(&changed_nodes);
            }
        }
        // Drain layouter updates after DOM broadcast
        self.layouter_mirror.update().await?;

        Ok(style_changed)
    }

    /// Compute layout with debouncing logic and forward dirty rectangles to renderer
    fn compute_layout_with_debouncing(&mut self, style_changed: bool) -> Result<(), Error> {
        // Determine if layout should run based on style or DOM changes
        let mut should_layout = style_changed || self.layouter_mirror.mirror_mut().take_and_clear_layout_dirty();
        
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
        }
        
        Ok(())
    }

    pub async fn update(&mut self) -> Result<(), Error> {
        // Drain and execute any pending inline scripts from the parser
        self.execute_pending_scripts();

        // Finalize DOM loading if the loader has finished
        self.finalize_dom_loading_if_needed().await?;

        // Apply any pending DOM updates
        self.dom.update().await?;
        // Keep the DOM index mirror in sync before any JS queries (e.g., getElementById)
        self.dom_index_mirror.update().await?;

        // Handle DOM content loaded event if needed
        self.handle_dom_content_loaded_if_needed().await?;

        // Process CSS and style updates
        let style_changed = self.process_css_and_styles().await?;
        
        // Compute layout with debouncing and forward dirty rectangles
        self.compute_layout_with_debouncing(style_changed)?;

        // Drain renderer mirror after DOM broadcast so the scene graph stays in sync
        self.renderer_mirror.update().await?;
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

    /// Return a JSON snapshot of the current DOM tree (deterministic schema for comparison)
    pub fn dom_json_snapshot_string(&self) -> String {
        self.dom.to_json_string()
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
                    list.push(DrawText {
                        x: rect.x as f32,
                        y: rect.y as f32,
                        text,
                        color: color_rgb,
                        font_size,
                    });
                }
            }
        }
        Ok(list)
    }
}


