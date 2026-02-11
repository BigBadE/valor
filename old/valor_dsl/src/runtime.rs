//! Rust-based DSL runtime for Valor
//!
//! This module provides a native Rust runtime for executing Valor DSL code,
//! eliminating the need for a JavaScript engine for DSL-based UIs.

use crate::VirtualDom;
use crate::events::{EventCallbacks, EventContext};
use anyhow::Result;
use js::{DOMUpdate, NodeKey};
use tokio::sync::mpsc;

/// Valor DSL Runtime that manages UI state and rendering
pub struct ValorRuntime {
    /// Virtual DOM for compiling HTML to DOMUpdates
    vdom: VirtualDom,
    /// Event callbacks registry
    callbacks: EventCallbacks,
    /// Channel for sending DOM updates to the page
    dom_sender: mpsc::Sender<Vec<DOMUpdate>>,
}

impl ValorRuntime {
    /// Create a new Valor runtime with the given components
    #[inline]
    #[must_use]
    pub fn new(
        vdom: VirtualDom,
        callbacks: EventCallbacks,
        dom_sender: mpsc::Sender<Vec<DOMUpdate>>,
    ) -> Self {
        Self {
            vdom,
            callbacks,
            dom_sender,
        }
    }

    /// Render HTML to the page
    ///
    /// # Errors
    /// Returns an error if HTML compilation fails or sending updates fails
    pub async fn render(&mut self, html: &str, parent: NodeKey) -> Result<()> {
        let updates = self.vdom.compile_html(html, parent, &self.callbacks)?;
        self.dom_sender.send(updates).await?;
        Ok(())
    }

    /// Handle an event on a node
    ///
    /// # Errors
    /// Returns an error if the event handler execution fails
    pub async fn handle_event(&mut self, node: NodeKey, event_type: &str) -> Result<()> {
        if let Some(handler) = self.vdom.get_handler(node, event_type) {
            let ctx = EventContext {
                node,
                event_type: event_type.to_string(),
                dom_sender: self.dom_sender.clone(),
            };
            handler(&ctx);
        }
        Ok(())
    }

    /// Get a mutable reference to the virtual DOM
    #[inline]
    #[must_use]
    pub fn vdom_mut(&mut self) -> &mut VirtualDom {
        &mut self.vdom
    }

    /// Get a mutable reference to the event callbacks
    #[inline]
    #[must_use]
    pub fn callbacks_mut(&mut self) -> &mut EventCallbacks {
        &mut self.callbacks
    }
}

/// Builder for creating a Valor runtime
pub struct RuntimeBuilder {
    vdom: Option<VirtualDom>,
    callbacks: Option<EventCallbacks>,
    dom_sender: Option<mpsc::Sender<Vec<DOMUpdate>>>,
}

impl RuntimeBuilder {
    /// Create a new runtime builder
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            vdom: None,
            callbacks: None,
            dom_sender: None,
        }
    }

    /// Set the virtual DOM
    #[inline]
    #[must_use]
    pub fn vdom(mut self, vdom: VirtualDom) -> Self {
        self.vdom = Some(vdom);
        self
    }

    /// Set the event callbacks
    #[inline]
    #[must_use]
    pub fn callbacks(mut self, callbacks: EventCallbacks) -> Self {
        self.callbacks = Some(callbacks);
        self
    }

    /// Set the DOM update sender
    #[inline]
    #[must_use]
    pub fn dom_sender(mut self, sender: mpsc::Sender<Vec<DOMUpdate>>) -> Self {
        self.dom_sender = Some(sender);
        self
    }

    /// Build the runtime
    ///
    /// # Errors
    /// Returns an error if required components are missing
    pub fn build(self) -> Result<ValorRuntime> {
        let vdom = self
            .vdom
            .ok_or_else(|| anyhow::anyhow!("VirtualDom is required"))?;
        let callbacks = self
            .callbacks
            .ok_or_else(|| anyhow::anyhow!("EventCallbacks is required"))?;
        let dom_sender = self
            .dom_sender
            .ok_or_else(|| anyhow::anyhow!("DOM sender is required"))?;

        Ok(ValorRuntime::new(vdom, callbacks, dom_sender))
    }
}

impl Default for RuntimeBuilder {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
