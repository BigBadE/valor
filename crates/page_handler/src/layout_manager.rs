//! Layout manager that bridges DOM updates to layout computation.

use anyhow::Result;
use css::style_types::ComputedStyle;
use css_core::{LayoutRect, Layouter};
use js::{DOMSubscriber, DOMUpdate, NodeKey};
use std::collections::HashMap;

/// Snapshot type for layout tree structure.
pub type SnapshotVec = Vec<(NodeKey, LayoutNodeKind, Vec<NodeKey>)>;

/// Layout manager that subscribes to DOM updates and computes layout on-demand.
pub struct LayoutManager {
    /// The layouter that performs layout computation
    layouter: Layouter,
    /// Cached computed styles
    styles: HashMap<NodeKey, ComputedStyle>,
}

impl LayoutManager {
    /// Create a new layout manager.
    pub fn new() -> Self {
        Self {
            layouter: Layouter::new(),
            styles: HashMap::new(),
        }
    }

    /// Get layout rects by computing layout geometry.
    pub fn rects(&mut self) -> HashMap<NodeKey, LayoutRect> {
        self.layouter.compute_layout_geometry()
    }

    /// Get attrs map.
    pub fn attrs_map(&self) -> HashMap<NodeKey, HashMap<String, String>> {
        self.layouter.attrs_map()
    }

    /// Get computed styles.
    pub fn computed_styles(&self) -> HashMap<NodeKey, ComputedStyle> {
        self.styles.clone()
    }

    /// Set viewport dimensions (initial containing block).
    ///
    /// This must be called before layout computation to match the browser viewport.
    /// The dimensions should account for scrollbars if present.
    pub fn set_viewport(&mut self, _width: i32, _height: i32) {
        // Viewport is handled internally by the layouter
    }

    /// Set computed styles and trigger layout computation.
    pub fn set_computed_styles(&mut self, styles: HashMap<NodeKey, ComputedStyle>) {
        self.styles = styles.clone();
        self.layouter.set_computed_styles(styles);
        self.compute_layout();
    }

    /// Compute layout using cached styles.
    pub fn compute_layout(&mut self) {
        self.layouter.compute_layout();
    }

    /// Get snapshot of layout tree structure.
    pub fn snapshot(&self) -> SnapshotVec {
        self.layouter.snapshot()
    }
}

impl Default for LayoutManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DOMSubscriber for LayoutManager {
    fn apply_update(&mut self, update: DOMUpdate) -> Result<()> {
        self.layouter.apply_update(update)
    }
}

/// Layout node kind for snapshot.
#[derive(Debug, Clone)]
pub enum LayoutNodeKind {
    /// Document root
    Document,
    /// Block-level element
    Block { tag: String },
    /// Inline text node
    InlineText { text: String },
}
