//! Core module of the CSS engine, containing the style subsystem.
//! This crate exposes a minimal orchestrator that coordinates style computation.
//!
//! Layout is handled separately by `css_core::LayoutEngine`.

use anyhow::Result;
pub use js::{DOMSubscriber, DOMUpdate, NodeKey};
use std::collections::HashMap;

// Self-contained core types (kept simple; orchestrator maps to its public types)
mod data;
pub mod layout_model;
pub mod queries;
pub mod selectors;
mod style;
mod style_database;
pub mod style_model;
pub mod types;

pub use style_database::StyleDatabase;

pub struct CoreEngine {
    /// Style system that computes computed styles from the stylesheet and DOM.
    style: style::StyleComputer,
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
        }
    }

    /// Apply a `DOMUpdate` to the style subsystem.
    ///
    /// # Errors
    /// Returns an error if the style subsystem reports a failure while applying the update.
    #[inline]
    pub fn apply_dom_update(&mut self, update: DOMUpdate) -> Result<()> {
        self.style.apply_update(update);
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
}
