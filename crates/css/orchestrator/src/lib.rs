//! Core module of the CSS engine, containing the style and layout subsystems.
//! This crate exposes a minimal orchestrator that coordinates style and layout engines.

use anyhow::Result;
pub use js::{DOMSubscriber, DOMUpdate, NodeKey};
use std::collections::HashMap;

// Self-contained core types (kept simple; orchestrator maps to its public types)
mod data;
mod layout;
pub mod layout_model;
pub mod selectors;
mod style;
pub mod style_model;
pub mod types;

pub struct CoreEngine {
    /// Style system that computes computed styles from the stylesheet and DOM.
    style: style::StyleComputer,
    /// Layout engine that computes layout rects and snapshots.
    layout: layout::LayoutEngine,
}

impl Default for CoreEngine {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl CoreEngine {
    #[inline]
    pub fn new() -> Self {
        Self {
            style: style::StyleComputer::new(),
            layout: layout::LayoutEngine::new(),
        }
    }

    /// Apply a `DOMUpdate` to both style and layout subsystems.
    ///
    /// # Errors
    /// Returns an error if either subsystem reports a failure while applying the update.
    #[inline]
    pub fn apply_dom_update(&mut self, update: DOMUpdate) -> Result<()> {
        // Keep both subsystems informed
        self.style.apply_update(update.clone());
        self.layout.apply_update(update);
        Ok(())
    }

    #[inline]
    pub fn replace_stylesheet(&mut self, sheet: types::Stylesheet) {
        self.style.replace_stylesheet(sheet);
    }

    /// Recompute dirty styles and return whether any styles changed.
    #[inline]
    pub fn recompute_styles(&mut self) -> bool {
        self.style.recompute_dirty()
    }

    #[inline]
    pub fn computed_snapshot(&self) -> HashMap<NodeKey, style_model::ComputedStyle> {
        self.style.computed_snapshot()
    }

    #[inline]
    pub fn compute_layout(&mut self) -> HashMap<NodeKey, layout_model::LayoutRect> {
        self.layout.compute_layout()
    }

    #[inline]
    pub fn take_dirty_rects(&mut self) -> Vec<layout_model::LayoutRect> {
        self.layout.take_dirty_rects()
    }

    #[inline]
    pub fn layout_snapshot(&self) -> LayoutSnapshot {
        self.layout.snapshot()
    }
}

/// A typed snapshot of the current layout tree.
pub type LayoutSnapshot = Vec<(NodeKey, layout_model::LayoutNodeKind, Vec<NodeKey>)>;
