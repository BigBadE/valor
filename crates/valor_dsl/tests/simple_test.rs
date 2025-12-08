//! Simple integration test without Bevy dependency

use js::{DOMUpdate, KeySpace, NodeKey};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::mpsc;
use valor_dsl::VirtualDom;
use valor_dsl::events::{EventCallbacks, EventContext};

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests HTML parsing works correctly
    ///
    /// # Panics
    /// Panics if HTML parsing fails or no updates are generated
    #[test]
    fn test_html_parsing_works() {
        let mut keyspace = KeySpace::new();
        let key_manager = keyspace.register_manager();
        let mut vdom = VirtualDom::new(key_manager);

        let html = r#"<div class="test"><p>Hello World</p></div>"#;
        let callbacks = EventCallbacks::new();

        let result = vdom.compile_html(html, NodeKey::ROOT, &callbacks);
        assert!(result.is_ok(), "HTML should compile");

        if let Ok(updates) = result {
            assert!(!updates.is_empty(), "Should generate updates");
        }
    }

    /// Tests event callbacks fire correctly
    ///
    /// # Panics
    /// Panics if event callback registration or execution fails
    #[test]
    fn test_event_callback_fires() {
        let mut keyspace = KeySpace::new();
        let key_manager = keyspace.register_manager();
        let mut vdom = VirtualDom::new(key_manager);

        let html = r#"<button on:click="test_click">Click</button>"#;

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = Arc::clone(&counter);

        let mut callbacks = EventCallbacks::new();
        callbacks.register("test_click", move |_ctx: &EventContext| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        let result = vdom.compile_html(html, NodeKey::ROOT, &callbacks);
        assert!(result.is_ok());

        // Find the button node
        if let Ok(updates) = result {
            let button_node = updates.iter().find_map(|update| {
                if let DOMUpdate::InsertElement { node, tag, .. } = update
                    && tag == "button"
                {
                    Some(*node)
                } else {
                    None
                }
            });

            assert!(button_node.is_some(), "Should find button node");

            if let Some(node) = button_node {
                // Get the handler and call it
                let handler = vdom.get_handler(node, "click");
                assert!(handler.is_some(), "Should have click handler");

                if let Some(handler_fn) = handler {
                    // Simulate the event
                    let (sender, _receiver) = mpsc::channel(10);
                    let ctx = EventContext {
                        node,
                        event_type: "click".to_string(),
                        dom_sender: sender,
                    };

                    handler_fn(&ctx);

                    assert_eq!(
                        counter.load(Ordering::SeqCst),
                        1,
                        "Handler should have been called"
                    );
                }
            }
        }
    }

    /// Tests multiple elements are parsed correctly
    ///
    /// # Panics
    /// Panics if HTML parsing fails or element count is incorrect
    #[test]
    fn test_multiple_elements() {
        let mut keyspace = KeySpace::new();
        let key_manager = keyspace.register_manager();
        let mut vdom = VirtualDom::new(key_manager);

        let html = r"
            <div>
                <h1>Title</h1>
                <p>Paragraph</p>
                <button>Button</button>
            </div>
        ";

        let callbacks = EventCallbacks::new();
        let result = vdom.compile_html(html, NodeKey::ROOT, &callbacks);
        assert!(result.is_ok());

        if let Ok(updates) = result {
            // Count element insertions
            let element_count = updates
                .iter()
                .filter(|update| matches!(update, DOMUpdate::InsertElement { .. }))
                .count();

            assert!(
                element_count >= 4,
                "Should have at least 4 elements (div, h1, p, button)"
            );
        }
    }

    /// Tests HTML attributes are set correctly
    ///
    /// # Panics
    /// Panics if attribute parsing fails or required attributes are missing
    #[test]
    fn test_attributes() {
        let mut keyspace = KeySpace::new();
        let key_manager = keyspace.register_manager();
        let mut vdom = VirtualDom::new(key_manager);

        let html = r#"<input type="text" id="test-input" placeholder="Enter text" />"#;

        let callbacks = EventCallbacks::new();
        let result = vdom.compile_html(html, NodeKey::ROOT, &callbacks);
        assert!(result.is_ok());

        if let Ok(updates) = result {
            // Check for SetAttr updates
            let has_type = updates.iter().any(|update| {
                matches!(update, DOMUpdate::SetAttr { name, value, .. }
                    if name == "type" && value == "text")
            });

            let has_id = updates.iter().any(|update| {
                matches!(update, DOMUpdate::SetAttr { name, value, .. }
                    if name == "id" && value == "test-input")
            });

            assert!(has_type, "Should have type attribute");
            assert!(has_id, "Should have id attribute");
        }
    }
}
