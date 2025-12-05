//! Simple integration test without Bevy dependency

use valor_dsl::*;
use valor_dsl::events::{EventCallbacks, EventContext};
use js::{KeySpace, NodeKey};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

#[test]
fn test_html_parsing_works() {
    let mut keyspace = KeySpace::new();
    let key_manager = keyspace.register_manager();
    let mut vdom = VirtualDom::new(key_manager);

    let html = r#"<div class="test"><p>Hello World</p></div>"#;
    let callbacks = EventCallbacks::new();

    let result = vdom.compile_html(html, NodeKey::ROOT, callbacks);
    assert!(result.is_ok(), "HTML should compile");

    let updates = result.unwrap();
    assert!(!updates.is_empty(), "Should generate updates");
}

#[test]
fn test_event_callback_fires() {
    let mut keyspace = KeySpace::new();
    let key_manager = keyspace.register_manager();
    let mut vdom = VirtualDom::new(key_manager);

    let html = r#"<button on:click="test_click">Click</button>"#;

    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = counter.clone();

    let mut callbacks = EventCallbacks::new();
    callbacks.register("test_click", move |_ctx: &EventContext| {
        counter_clone.fetch_add(1, Ordering::SeqCst);
    });

    let result = vdom.compile_html(html, NodeKey::ROOT, callbacks);
    assert!(result.is_ok());

    // Find the button node
    let updates = result.unwrap();
    let button_node = updates.iter().find_map(|u| {
        if let js::DOMUpdate::InsertElement { node, tag, .. } = u {
            if tag == "button" {
                return Some(*node);
            }
        }
        None
    });

    assert!(button_node.is_some(), "Should find button node");

    // Get the handler and call it
    let handler = vdom.get_handler(button_node.unwrap(), "click");
    assert!(handler.is_some(), "Should have click handler");

    // Simulate the event
    use tokio::sync::mpsc;
    let (tx, _rx) = mpsc::channel(10);
    let ctx = EventContext {
        node: button_node.unwrap(),
        event_type: "click".to_string(),
        dom_sender: tx,
    };

    handler.unwrap()(&ctx);

    assert_eq!(counter.load(Ordering::SeqCst), 1, "Handler should have been called");
}

#[test]
fn test_multiple_elements() {
    let mut keyspace = KeySpace::new();
    let key_manager = keyspace.register_manager();
    let mut vdom = VirtualDom::new(key_manager);

    let html = r#"
        <div>
            <h1>Title</h1>
            <p>Paragraph</p>
            <button>Button</button>
        </div>
    "#;

    let callbacks = EventCallbacks::new();
    let result = vdom.compile_html(html, NodeKey::ROOT, callbacks);
    assert!(result.is_ok());

    let updates = result.unwrap();

    // Count element insertions
    let element_count = updates.iter().filter(|u| {
        matches!(u, js::DOMUpdate::InsertElement { .. })
    }).count();

    assert!(element_count >= 4, "Should have at least 4 elements (div, h1, p, button)");
}

#[test]
fn test_attributes() {
    let mut keyspace = KeySpace::new();
    let key_manager = keyspace.register_manager();
    let mut vdom = VirtualDom::new(key_manager);

    let html = r#"<input type="text" id="test-input" placeholder="Enter text" />"#;

    let callbacks = EventCallbacks::new();
    let result = vdom.compile_html(html, NodeKey::ROOT, callbacks);
    assert!(result.is_ok());

    let updates = result.unwrap();

    // Check for SetAttr updates
    let has_type = updates.iter().any(|u| {
        matches!(u, js::DOMUpdate::SetAttr { name, value, .. }
            if name == "type" && value == "text")
    });

    let has_id = updates.iter().any(|u| {
        matches!(u, js::DOMUpdate::SetAttr { name, value, .. }
            if name == "id" && value == "test-input")
    });

    assert!(has_type, "Should have type attribute");
    assert!(has_id, "Should have id attribute");
}
