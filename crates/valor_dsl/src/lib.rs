//! Valor DSL - Declarative HTML/CSS UI framework for Valor browser engine
//!
//! This crate provides a declarative way to build UIs using HTML/CSS that compiles
//! to `DOMUpdate` messages. It includes optional Bevy ECS integration for game UIs.

use anyhow::Result;
use js::{DOMUpdate, NodeKey, NodeKeyManager};
use std::collections::HashMap;

pub mod events;
pub mod macros;
mod parser;
pub mod runtime;

#[cfg(feature = "bevy_integration")]
pub mod bevy_integration;

#[cfg(feature = "bevy_integration")]
pub mod bevy_events;

#[cfg(feature = "bevy_integration")]
pub mod html_macro;

#[cfg(feature = "bevy_integration")]
pub mod reactive;
#[cfg(feature = "bevy_integration")]
pub mod standalone;

#[cfg(feature = "bevy_integration")]
pub mod reactive_html_macro;

#[cfg(feature = "bevy_integration")]
pub mod styling;

// Re-export the JSX-like jsx! macro
#[cfg(feature = "bevy_integration")]
pub use valor_dsl_macros::jsx;

/// Virtual DOM that compiles HTML to `DOMUpdate` messages
pub struct VirtualDom {
    key_manager: NodeKeyManager<usize>,
    next_id: usize,
    event_handlers: HashMap<NodeKey, HashMap<String, events::EventHandler>>,
}

impl VirtualDom {
    /// Create a new `VirtualDom` with the given key manager
    #[inline]
    pub fn new(key_manager: NodeKeyManager<usize>) -> Self {
        Self {
            key_manager,
            next_id: 0,
            event_handlers: HashMap::new(),
        }
    }

    /// Compile HTML string to `DOMUpdate` messages
    ///
    /// # Errors
    /// Returns an error if HTML parsing fails
    pub fn compile_html(
        &mut self,
        html_str: &str,
        parent: NodeKey,
        callbacks: &events::EventCallbacks,
    ) -> Result<Vec<DOMUpdate>> {
        let result = parser::parse_html_to_updates(
            html_str,
            parent,
            &mut self.key_manager,
            &mut self.next_id,
        )?;

        // Extract event handlers from on:* attributes
        for (node_key, attrs) in &result.attributes {
            for (name, value) in attrs {
                if let Some(event_name) = name.strip_prefix("on:")
                    && let Some(handler) = callbacks.get(value)
                {
                    self.event_handlers
                        .entry(*node_key)
                        .or_default()
                        .insert(event_name.to_string(), handler);
                }
            }
        }

        Ok(result.updates)
    }

    /// Get event handler for a node and event type
    #[inline]
    #[must_use]
    pub fn get_handler(&self, node: NodeKey, event_type: &str) -> Option<&events::EventHandler> {
        let handlers = self.event_handlers.get(&node)?;
        handlers.get(event_type)
    }

    /// Get all event handlers
    #[inline]
    #[must_use]
    pub const fn event_handlers(&self) -> &HashMap<NodeKey, HashMap<String, events::EventHandler>> {
        &self.event_handlers
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use js::KeySpace;

    /// Tests simple HTML compilation
    ///
    /// # Panics
    /// Panics if HTML parsing fails
    #[test]
    fn test_simple_html_compilation() {
        let mut keyspace = KeySpace::new();
        let key_manager = keyspace.register_manager();
        let mut vdom = VirtualDom::new(key_manager);

        let html = r"<div><h1>Hello</h1></div>";
        let callbacks = events::EventCallbacks::new();
        let updates = vdom
            .compile_html(html, NodeKey::ROOT, &callbacks)
            .unwrap_or_else(|_error| {
                // Failed to compile HTML
                Vec::new()
            });

        assert!(!updates.is_empty());
    }

    /// Tests HTML with attributes
    ///
    /// # Panics
    /// Panics if HTML parsing fails or attributes are not set correctly
    #[test]
    fn test_html_with_attributes() {
        let mut keyspace = KeySpace::new();
        let key_manager = keyspace.register_manager();
        let mut vdom = VirtualDom::new(key_manager);

        let html = r#"<div class="container" style="color: red;">Test</div>"#;
        let callbacks = events::EventCallbacks::new();
        let updates = vdom
            .compile_html(html, NodeKey::ROOT, &callbacks)
            .unwrap_or_else(|_error| {
                // Failed to compile HTML with attributes
                Vec::new()
            });

        assert!(!updates.is_empty());
        let has_class = updates.iter().any(|update| {
            matches!(update, DOMUpdate::SetAttr { name, value, .. }
                if name == "class" && value == "container")
        });
        assert!(has_class);
    }
}
