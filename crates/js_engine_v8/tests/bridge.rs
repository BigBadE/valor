use js::JsEngine;
use js::{build_default_bindings, HostContext, ConsoleLogger, DOMUpdate, KeySpace, NodeKeyManager};
use js_engine_v8::V8Engine;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

#[test]
fn v8_domcontentloaded_bindings_work() {
    // Initialize engine
    let mut engine = V8Engine::new().expect("v8 engine");

    // Prepare HostContext similar to page wiring
    let (tx, _rx) = mpsc::channel::<Vec<DOMUpdate>>(8);
    let mut key_space = KeySpace::new();
    let js_keyman: NodeKeyManager<u64> = key_space.register_manager();
    // Create a Tokio runtime to supply a Handle required by HostContext
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let ctx = HostContext {
        page_id: None,
        logger: Arc::new(ConsoleLogger),
        dom_sender: tx,
        js_node_keys: Arc::new(Mutex::new(js_keyman)),
        js_local_id_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        js_created_nodes: Arc::new(Mutex::new(std::collections::HashMap::new())),
        dom_index: Arc::new(Mutex::new(js::dom_index::DomIndexState::default())),
        tokio_handle: rt.handle().clone(),
        page_origin: String::from("file://"),
        fetch_registry: Arc::new(Mutex::new(js::bindings::FetchRegistry::default())),
        performance_start: std::time::Instant::now(),
        storage_local: Arc::new(Mutex::new(js::bindings::StorageRegistry::default())),
        storage_session: Arc::new(Mutex::new(js::bindings::StorageRegistry::default())),
        chrome_host_tx: None,
    };

    let bindings = build_default_bindings();
    engine.install_bindings(ctx, &bindings).expect("install bindings");

    // Ensure stubs are set up
    engine.eval_script("void 0;", "about:blank").unwrap();

    // Register a DOMContentLoaded listener and dispatch it; should not panic.
    engine
        .eval_script(
            "document.addEventListener('DOMContentLoaded', function(){ console.log('ok'); }); document.__valorDispatchDOMContentLoaded();",
            "test://bridge",
        )
        .unwrap();
}
