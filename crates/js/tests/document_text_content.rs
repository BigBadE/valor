//! Tests for the engine-agnostic JS bindings in the `js` crate.
//!
//! This suite validates that the `document` namespace functions emit the
//! expected DOMUpdate batches so changes are reflected to the runtime DOM.

use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use js::{
    bindings::{build_document_namespace, HostFnKind, HostNamespace, JSValue, HostContext},
    dom_index::DomIndexState,
    NodeKey, NodeKeyManager, KeySpace, DOMUpdate,
    ConsoleLogger,
};

/// Helper: build a HostContext wired with a sender/receiver pair and a shared DomIndexState.
fn make_host_context(sender: mpsc::Sender<Vec<DOMUpdate>>, dom_index: Arc<Mutex<DomIndexState>>) -> HostContext {
    // Create a fresh NodeKeyManager shard for JS-created nodes
    let mut key_space = KeySpace::new();
    let js_keyman: NodeKeyManager<u64> = key_space.register_manager();
    HostContext {
        page_id: None,
        logger: Arc::new(ConsoleLogger),
        dom_sender: sender,
        js_node_keys: Arc::new(Mutex::new(js_keyman)),
        js_local_id_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        js_created_nodes: Arc::new(Mutex::new(std::collections::HashMap::new())),
        dom_index,
    }
}

/// Extract a synchronous host function by name from a HostNamespace.
fn get_sync(ns: &HostNamespace, name: &str) -> Arc<js::bindings::HostFnSync> {
    match ns.functions.get(name).expect("function exists") {
        HostFnKind::Sync(f) => f.clone(),
    }
}

/// Ensure that calling document.setTextContent(elementKey, text) emits a batch of DOMUpdate values
/// that first removes all current children of the element, then inserts a new text node at position 0.
#[test]
fn set_text_content_emits_remove_and_insert() {
    // Prepare a shared DOM index state with a parent element having two children
    let dom_index = Arc::new(Mutex::new(DomIndexState::default()));
    let parent = NodeKey(42);
    let child_a = NodeKey(1001);
    let child_b = NodeKey(1002);
    {
        let mut guard = dom_index.lock().unwrap();
        guard.children_by_parent.insert(parent, vec![child_a, child_b]);
        guard.parent_by_child.insert(child_a, parent);
        guard.parent_by_child.insert(child_b, parent);
    }

    // Channel to observe DOM updates emitted by the host function
    let (tx, mut rx) = mpsc::channel::<Vec<DOMUpdate>>(8);
    let context = make_host_context(tx, dom_index.clone());

    // Obtain the setTextContent host function
    let ns = build_document_namespace();
    let set_text_content = get_sync(&ns, "setTextContent");

    // Invoke: set text to "Hello, Valor!"
    let result = set_text_content(
        &context,
        vec![JSValue::String(parent.0.to_string()), JSValue::String("Hello, Valor!".to_string())],
    );
    assert!(result.is_ok(), "host call failed: {:?}", result.err());

    // Drain the emitted batch
    let batch = rx.try_recv().expect("expected a DOM update batch");
    // Expect exactly 3 updates: Remove child A, Remove child B, InsertText at pos 0 under parent
    assert_eq!(batch.len(), 3, "unexpected batch length: {:?}", batch);

    // Validate order and contents
    use js::DOMUpdate::*;
    match &batch[0] { RemoveNode { node } => assert_eq!(*node, child_a), other => panic!("unexpected first update: {:?}", other) }
    match &batch[1] { RemoveNode { node } => assert_eq!(*node, child_b), other => panic!("unexpected second update: {:?}", other) }
    match &batch[2] {
        InsertText { parent: p, node: text_key, text, pos } => {
            assert_eq!(*p, parent, "insert parent mismatch");
            assert_eq!(*pos, 0, "text should be inserted at position 0");
            assert_eq!(text, "Hello, Valor!");
            assert_ne!(text_key.0, 0, "new text node key should be non-zero");
            assert_ne!(*text_key, parent, "text node key must differ from parent");
        }
        other => panic!("unexpected third update: {:?}", other),
    }
}
