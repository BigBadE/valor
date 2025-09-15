//! Minimal layout engine stub that produces a fixed root rect and snapshots.
use core::mem::take;
use std::collections::{HashMap, hash_map::Entry};

use crate::layout_model;
use js::{DOMUpdate, NodeKey};

/// Module-scoped type alias for snapshots of the layout tree.
pub type LayoutSnapshot = Vec<(NodeKey, layout_model::LayoutNodeKind, Vec<NodeKey>)>;

#[derive(Default)]
/// Minimal layout engine state holder.
pub struct LayoutEngine {
    /// Map of node keys to their computed layout rectangles.
    rects: HashMap<NodeKey, layout_model::LayoutRect>,
    /// Rectangles that became dirty since last layout pass.
    dirty_rects: Vec<layout_model::LayoutRect>,
    /// Cached snapshot of the layout tree structure.
    snapshot_nodes: LayoutSnapshot,
}

impl LayoutEngine {
    /// Create a new layout engine with empty state.
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Compute a very simple layout model (fixed root rect for now).
    #[inline]
    pub fn compute_layout(&mut self) -> HashMap<NodeKey, layout_model::LayoutRect> {
        if let Entry::Vacant(vacant_entry) = self.rects.entry(NodeKey::ROOT) {
            vacant_entry.insert(layout_model::LayoutRect {
                x: 0,
                y: 0,
                width: 800,
                height: 600,
            });
            self.snapshot_nodes = vec![(
                NodeKey::ROOT,
                layout_model::LayoutNodeKind::Document,
                vec![],
            )];
        }
        self.rects.clone()
    }

    /// Take and clear the list of dirty rectangles.
    #[inline]
    pub fn take_dirty_rects(&mut self) -> Vec<layout_model::LayoutRect> {
        take(&mut self.dirty_rects)
    }

    /// Return a copy of the current layout snapshot.
    #[inline]
    pub fn snapshot(&self) -> LayoutSnapshot {
        self.snapshot_nodes.clone()
    }

    /// Apply a `DOMUpdate` to the layout engine.
    /// Currently marks layout as needing recomputation by clearing cached snapshot.
    #[inline]
    pub fn apply_update(&mut self, _update: DOMUpdate) {
        // Touch state so future compute_layout() recomputes as needed.
        self.snapshot_nodes.clear();
    }
}
