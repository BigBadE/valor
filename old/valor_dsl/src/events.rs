//! Event handling system for Valor DSL

use js::{DOMUpdate, NodeKey};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Context passed to event handlers
pub struct EventContext {
    /// The node that triggered the event
    pub node: NodeKey,
    /// The event type (e.g., "click", "input")
    pub event_type: String,
    /// Sender for DOM updates
    pub dom_sender: mpsc::Sender<Vec<DOMUpdate>>,
}

/// Type-erased event handler
pub type EventHandler = Arc<dyn Fn(&EventContext) + Send + Sync>;

/// Registry of event callbacks
pub struct EventCallbacks {
    callbacks: HashMap<String, EventHandler>,
}

impl EventCallbacks {
    /// Create a new empty callback registry
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            callbacks: HashMap::new(),
        }
    }

    /// Register a callback with a name
    #[inline]
    pub fn register<F>(&mut self, name: impl Into<String>, handler: F)
    where
        F: Fn(&EventContext) + Send + Sync + 'static,
    {
        self.callbacks.insert(name.into(), Arc::new(handler));
    }

    /// Get a callback by name
    #[inline]
    #[must_use]
    pub fn get(&self, name: &str) -> Option<EventHandler> {
        self.callbacks.get(name).cloned()
    }
}

impl Default for EventCallbacks {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
