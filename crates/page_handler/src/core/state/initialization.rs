//! Page initialization helpers.

use crate::core::incremental_layout::IncrementalLayoutEngine;
use crate::core::pipeline::{Pipeline, PipelineConfig};
use crate::internal::url::stream_url;
use crate::utilities::config::ValorConfig;
use crate::utilities::scheduler::FrameScheduler;
use anyhow::{Error, anyhow};
use core::sync::atomic::AtomicU64;
use css::{CSSMirror, Orchestrator};
use html::dom::DOM;
use html::parser::{HTMLParser, ParseInputs, ScriptJob};

use js::{DOMMirror, DOMUpdate, DomIndex, NodeKey, NodeKeyManager, SharedDomIndex};

#[cfg(feature = "js")]
use js::bindings::{FetchRegistry, StorageRegistry};
#[cfg(feature = "js")]
use js::{ConsoleLogger, HostContext, build_default_bindings};
#[cfg(feature = "js")]
use js_engine_v8::V8Engine;

use renderer::Renderer;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::runtime::Handle;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use tokio::sync::{broadcast, mpsc};
use url::Url;

/// Helper to create DOM mirrors for the page.
pub(super) struct DomMirrors {
    /// CSS mirror for observing DOM updates.
    pub css_mirror: DOMMirror<CSSMirror>,
    /// Orchestrator mirror that computes styles using the css engine.
    pub orchestrator_mirror: DOMMirror<Orchestrator>,
    /// Renderer mirror for scene graph management.
    pub renderer_mirror: DOMMirror<Renderer>,
    /// DOM index mirror for JS queries.
    pub dom_index_mirror: DOMMirror<DomIndex>,
    /// Shared DOM index for synchronous lookups.
    pub dom_index_shared: SharedDomIndex,
}

/// Helper to create JS engine context.
#[cfg(feature = "js")]
pub(super) struct JsContext {
    /// JavaScript engine instance.
    pub js_engine: V8Engine,
    /// Host context for JS bindings.
    pub host_context: HostContext,
}

/// Create DOM mirrors for observing DOM updates.
///
/// # Errors
///
/// Returns an error if StyleDatabase initialization fails.
pub(super) fn create_dom_mirrors(
    in_updater: &mpsc::Sender<Vec<DOMUpdate>>,
    dom: &DOM,
    url: &Url,
) -> Result<DomMirrors, Error> {
    let css_mirror = DOMMirror::new(
        in_updater.clone(),
        dom.subscribe(),
        CSSMirror::with_base(url.clone()),
    );
    let orchestrator_mirror =
        DOMMirror::new(in_updater.clone(), dom.subscribe(), Orchestrator::new());
    let renderer_mirror = DOMMirror::new(in_updater.clone(), dom.subscribe(), Renderer::new());
    let (dom_index_sub, dom_index_shared) = DomIndex::new();
    let dom_index_mirror = DOMMirror::new(in_updater.clone(), dom.subscribe(), dom_index_sub);

    Ok(DomMirrors {
        css_mirror,
        orchestrator_mirror,
        renderer_mirror,
        dom_index_mirror,
        dom_index_shared,
    })
}

/// Build page origin string for same-origin checks.
pub(super) fn build_page_origin(url: &Url) -> String {
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
#[cfg(feature = "js")]
pub(super) fn create_js_context(
    in_updater: &mpsc::Sender<Vec<DOMUpdate>>,
    js_keyman: NodeKeyManager<u64>,
    dom_index_shared: &SharedDomIndex,
    handle: &Handle,
    url: &Url,
) -> Result<JsContext, Error> {
    let mut js_engine = V8Engine::new().map_err(|err| anyhow!("failed to init V8Engine: {err}"))?;
    let logger = Arc::new(ConsoleLogger);
    let js_node_keys = Arc::new(Mutex::new(js_keyman));
    let js_local_id_counter = Arc::new(AtomicU64::new(0));
    let js_created_nodes = Arc::new(Mutex::new(HashMap::new()));
    let page_origin = build_page_origin(url);
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

/// Initialize a blank page without loading from a URL.
/// This is useful for pages that will be populated entirely through `DOMUpdates`.
///
/// # Errors
///
/// Returns an error if page initialization fails.
pub(super) fn initialize_blank_page(
    handle: &Handle,
    config: ValorConfig,
    enable_js: bool,
) -> Result<PageComponents, Error> {
    let (out_updater, _out_receiver) = broadcast::channel(128);
    let (in_updater, in_receiver) = mpsc::channel(128);

    let mut dom = DOM::new(out_updater, in_receiver);
    let js_keyman = dom.register_manager::<u64>();

    #[cfg(feature = "js")]
    let script_rx = if enable_js {
        Some(unbounded_channel::<ScriptJob>().1)
    } else {
        None
    };

    // Use a dummy file:// URL for blank pages
    let blank_url =
        Url::parse("file:///blank").map_err(|err| anyhow!("Failed to parse blank URL: {err}"))?;

    let mirrors = create_dom_mirrors(&in_updater, &dom, &blank_url)?;

    #[cfg(feature = "js")]
    let js_ctx = if enable_js {
        Some(create_js_context(
            &in_updater,
            js_keyman,
            &mirrors.dom_index_shared,
            handle,
            &blank_url,
        )?)
    } else {
        None
    };
    let frame_scheduler = FrameScheduler::new(config.frame_budget());

    let pipeline = Pipeline::new(PipelineConfig::default());

    // Create incremental layout engine with viewport dimensions from config
    let incremental_layout =
        IncrementalLayoutEngine::new(config.viewport_width, config.viewport_height);

    // Create basic HTML document structure (html > body)
    // This ensures body styles work correctly
    // Use high NodeKey values to avoid collision with JSX-generated keys
    // JSX starts from 1, so we use high numbers that won't collide
    let html_node = NodeKey::pack(0, 0, 0xFFFF_FF00); // Very high ID
    let body_node = NodeKey::pack(0, 0, 0xFFFF_FF01); // Very high ID

    let structure_updates = vec![
        DOMUpdate::InsertElement {
            parent: NodeKey::ROOT,
            node: html_node,
            tag: "html".to_string(),
            pos: 0,
        },
        DOMUpdate::InsertElement {
            parent: html_node,
            node: body_node,
            tag: "body".to_string(),
            pos: 0,
        },
    ];

    // Apply the basic structure
    if let Err(err) = in_updater.blocking_send(structure_updates) {
        return Err(anyhow!("Failed to send initial document structure: {err}"));
    }

    Ok(PageComponents {
        dom,
        loader: None, // No HTML parser for blank pages
        mirrors,
        #[cfg(feature = "js")]
        js_ctx,
        in_updater,
        #[cfg(feature = "js")]
        script_rx,
        frame_scheduler,
        pipeline,
        incremental_layout,
        telemetry_enabled: config.telemetry_enabled,
    })
}

/// Initialize a new HTML page with all subsystems.
///
/// # Errors
///
/// Returns an error if page initialization fails.
pub(super) async fn initialize_page(
    handle: &Handle,
    url: Url,
    config: ValorConfig,
    enable_js: bool,
) -> Result<PageComponents, Error> {
    let (out_updater, out_receiver) = broadcast::channel(128);
    let (in_updater, in_receiver) = mpsc::channel(128);

    let mut dom = DOM::new(out_updater, in_receiver);
    let keyman = dom.register_parser_manager();
    let js_keyman = dom.register_manager::<u64>();

    #[cfg(feature = "js")]
    let (script_tx, script_rx) = if enable_js {
        let (tx, rx) = unbounded_channel::<ScriptJob>();
        (tx, Some(rx))
    } else {
        (unbounded_channel::<ScriptJob>().0, None)
    };
    #[cfg(not(feature = "js"))]
    let script_tx = unbounded_channel::<ScriptJob>().0;

    let inputs = ParseInputs {
        in_updater: in_updater.clone(),
        keyman,
        byte_stream: stream_url(&url).await?,
        dom_updates: out_receiver,
        script_tx,
        base_url: url.clone(),
    };
    let loader = HTMLParser::parse(handle, inputs);

    let mirrors = create_dom_mirrors(&in_updater, &dom, &url)?;

    #[cfg(feature = "js")]
    let js_ctx = if enable_js {
        Some(create_js_context(
            &in_updater,
            js_keyman,
            &mirrors.dom_index_shared,
            handle,
            &url,
        )?)
    } else {
        None
    };
    let frame_scheduler = FrameScheduler::new(config.frame_budget());

    let pipeline = Pipeline::new(PipelineConfig::default());

    // Create incremental layout engine with viewport dimensions from config
    let incremental_layout =
        IncrementalLayoutEngine::new(config.viewport_width, config.viewport_height);

    drop(url); // url was previously part of PageComponents but is no longer used
    Ok(PageComponents {
        dom,
        loader: Some(loader),
        mirrors,
        #[cfg(feature = "js")]
        js_ctx,
        in_updater,
        #[cfg(feature = "js")]
        script_rx,
        frame_scheduler,
        pipeline,
        incremental_layout,
        telemetry_enabled: config.telemetry_enabled,
    })
}

/// All components needed to construct an `HtmlPage`.
pub(super) struct PageComponents {
    pub dom: DOM,
    pub loader: Option<HTMLParser>,
    pub mirrors: DomMirrors,
    #[cfg(feature = "js")]
    pub js_ctx: Option<JsContext>,
    pub in_updater: mpsc::Sender<Vec<DOMUpdate>>,
    #[cfg(feature = "js")]
    pub script_rx: Option<UnboundedReceiver<ScriptJob>>,
    pub frame_scheduler: FrameScheduler,
    pub pipeline: Pipeline,
    pub incremental_layout: IncrementalLayoutEngine,
    pub telemetry_enabled: bool,
}
