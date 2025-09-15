//! Minimal style system stub used by the core engine.
//! Maintains a `Stylesheet` and a small computed styles map for the root node.

use anyhow::Result;
use std::collections::HashMap;

use crate::{style_model, types};
use js::{DOMUpdate, NodeKey};

/// Tracks stylesheet state and a tiny computed styles cache.
pub struct StyleComputer {
    /// The active stylesheet applied to the document.
    sheet: types::Stylesheet,
    /// Snapshot of computed styles (currently only the root is populated).
    computed: HashMap<NodeKey, style_model::ComputedStyle>,
    /// Whether the last recompute changed any styles.
    style_changed: bool,
    /// Nodes whose styles changed in the last recompute.
    changed_nodes: Vec<NodeKey>,
}

impl StyleComputer {
    /// Create a new style computer with an empty stylesheet and cache.
    #[inline]
    pub fn new() -> Self {
        Self {
            sheet: types::Stylesheet::default(),
            computed: HashMap::new(),
            style_changed: false,
            changed_nodes: Vec::new(),
        }
    }

    /// Replace the active stylesheet.
    #[inline]
    pub fn replace_stylesheet(&mut self, sheet: types::Stylesheet) {
        self.sheet = sheet;
    }

    /// Recompute dirty styles and return whether styles changed.
    #[inline]
    pub fn recompute_dirty(&mut self) -> bool {
        if self.computed.is_empty() {
            self.computed.insert(
                NodeKey::ROOT,
                style_model::ComputedStyle {
                    font_size: 16.0,
                    ..Default::default()
                },
            );
            self.style_changed = true;
            self.changed_nodes = vec![NodeKey::ROOT];
        } else {
            self.style_changed = false;
            self.changed_nodes.clear();
        }
        self.style_changed
    }

    /// Return a shallow copy of the current computed styles map.
    #[inline]
    pub fn computed_snapshot(&self) -> HashMap<NodeKey, style_model::ComputedStyle> {
        self.computed.clone()
    }

    /// Apply a DOM update to the style system.
    /// Marks styles as dirty so a subsequent recompute can refresh caches.
    #[inline]
    pub fn apply_update(&mut self, _update: DOMUpdate) {
        self.style_changed = true;
    }
}
