//! HTML representation for reactive components

use js::{DOMUpdate, NodeKey};
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicU16, Ordering};

/// Global render epoch counter for generating unique NodeKeys across renders
static RENDER_EPOCH: AtomicU16 = AtomicU16::new(1);

/// Get the next render epoch
pub fn next_render_epoch() -> u16 {
    RENDER_EPOCH.fetch_add(1, Ordering::SeqCst)
}

/// Represents HTML content that can be rendered as DOM updates
#[derive(Clone, Debug)]
pub struct Html {
    /// Direct DOM updates to apply
    pub updates: Vec<DOMUpdate>,
    /// Event handlers mapped by node key and event type
    pub event_handlers: HashMap<NodeKey, HashMap<String, String>>,
    /// The render epoch this HTML was generated in
    pub epoch: u16,
}

impl Html {
    /// Create HTML from DOM updates
    #[inline]
    pub fn new(updates: Vec<DOMUpdate>) -> Self {
        Self {
            updates,
            event_handlers: HashMap::new(),
            epoch: next_render_epoch(),
        }
    }

    /// Create HTML with event handlers
    #[inline]
    pub fn with_handlers(
        updates: Vec<DOMUpdate>,
        event_handlers: HashMap<NodeKey, HashMap<String, String>>,
    ) -> Self {
        Self {
            updates,
            event_handlers,
            epoch: next_render_epoch(),
        }
    }

    /// Create empty HTML
    #[inline]
    pub fn empty() -> Self {
        Self {
            updates: Vec::new(),
            event_handlers: HashMap::new(),
            epoch: 0,
        }
    }

    /// Add an event handler for a node
    pub fn add_event_handler(&mut self, node: NodeKey, event_type: String, handler_name: String) {
        self.event_handlers
            .entry(node)
            .or_default()
            .insert(event_type, handler_name);
    }

    /// Merge another Html into this one
    pub fn merge(&mut self, other: Html) {
        self.updates.extend(other.updates);
        for (node, handlers) in other.event_handlers {
            self.event_handlers
                .entry(node)
                .or_default()
                .extend(handlers);
        }
    }

    /// Prepend global CSS as a <style> element at the beginning
    pub fn prepend_global_styles(&mut self, css: &str) {
        if css.is_empty() {
            return;
        }

        // Create DOM updates for: <style>{css}</style>
        // Use epoch 0xFFFF (max u16) for global styles to avoid conflicts
        let style_node = NodeKey::pack(0xFFFF, 0, 1);
        let text_node = NodeKey::pack(0xFFFF, 0, 2);

        let mut prepended_updates = vec![
            DOMUpdate::InsertElement {
                parent: NodeKey::ROOT,
                node: style_node,
                tag: "style".to_string(),
                pos: 0,
            },
            DOMUpdate::InsertText {
                parent: style_node,
                node: text_node,
                text: css.to_string(),
                pos: 0,
            },
        ];

        // Prepend to existing updates
        prepended_updates.extend(self.updates.drain(..));
        self.updates = prepended_updates;
    }
}

impl fmt::Display for Html {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[Html with {} updates]", self.updates.len())
    }
}
