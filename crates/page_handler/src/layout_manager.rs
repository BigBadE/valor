//! Layout manager that bridges DOM updates to constraint-based layout.

use anyhow::Result;
use css::style_types::ComputedStyle;
use css_core::{ConstraintLayoutTree, LayoutRect, LayoutUnit, layout_tree};
use js::{DOMSubscriber, DOMUpdate, NodeKey};
use std::collections::HashMap;

/// Snapshot type for layout tree structure.
pub type SnapshotVec = Vec<(NodeKey, LayoutNodeKind, Vec<NodeKey>)>;

/// Layout manager that subscribes to DOM updates and computes layout on-demand.
pub struct LayoutManager {
    /// Tags from DOM (element name)
    tags: HashMap<NodeKey, String>,
    /// Children relationships from DOM
    children: HashMap<NodeKey, Vec<NodeKey>>,
    /// Text content from text nodes
    text_nodes: HashMap<NodeKey, String>,
    /// Element attributes
    attrs: HashMap<NodeKey, HashMap<String, String>>,
    /// Cached layout results (border-box rects)
    rects: HashMap<NodeKey, LayoutRect>,
    /// Cached computed styles
    styles: HashMap<NodeKey, ComputedStyle>,
    /// ICB dimensions
    icb_width: i32,
    icb_height: i32,
    /// Root element
    root: Option<NodeKey>,
}

impl LayoutManager {
    /// Create a new layout manager.
    pub fn new() -> Self {
        Self {
            tags: HashMap::new(),
            children: HashMap::new(),
            text_nodes: HashMap::new(),
            attrs: HashMap::new(),
            rects: HashMap::new(),
            styles: HashMap::new(),
            icb_width: 1024,
            icb_height: 768,
            root: None,
        }
    }

    /// Get layout rects.
    pub fn rects(&self) -> &HashMap<NodeKey, LayoutRect> {
        &self.rects
    }

    /// Get attrs map.
    pub fn attrs_map(&self) -> &HashMap<NodeKey, HashMap<String, String>> {
        &self.attrs
    }

    /// Get computed styles.
    pub fn computed_styles(&self) -> HashMap<NodeKey, ComputedStyle> {
        self.styles.clone()
    }

    /// Set viewport dimensions (initial containing block).
    ///
    /// This must be called before layout computation to match the browser viewport.
    /// The dimensions should account for scrollbars if present.
    /// Dimensions are in pixels and will be converted to 1/64px units internally.
    pub fn set_viewport(&mut self, width: i32, height: i32) {
        self.icb_width = width * 64;
        self.icb_height = height * 64;
    }

    /// Set computed styles and trigger layout computation.
    pub fn set_computed_styles(&mut self, styles: HashMap<NodeKey, ComputedStyle>) {
        self.styles = styles;
        self.compute_layout();
    }

    /// Compute layout using cached styles.
    pub fn compute_layout(&mut self) {
        if self.root.is_none() {
            return;
        }

        let mut tree = ConstraintLayoutTree::new(
            LayoutUnit::from_raw(self.icb_width),
            LayoutUnit::from_raw(self.icb_height),
        );
        tree.styles.clone_from(&self.styles);
        tree.children.clone_from(&self.children);
        tree.text_nodes.clone_from(&self.text_nodes);
        tree.tags.clone_from(&self.tags);
        tree.attrs.clone_from(&self.attrs);

        if let Some(root_node) = self.root {
            layout_tree(&mut tree, root_node);

            // Convert LayoutResults to LayoutRects
            self.rects.clear();
            for (node, result) in &tree.layout_results {
                self.rects
                    .insert(*node, LayoutRect::from_layout_result(result));
            }
        }
    }

    /// Get snapshot of layout tree structure.
    pub fn snapshot(&self) -> SnapshotVec {
        let mut result = Vec::new();

        // Add root if it exists
        if let Some(root_key) = self.root {
            self.build_snapshot_recursive(root_key, &mut result);
        }

        result
    }

    fn build_snapshot_recursive(&self, node: NodeKey, result: &mut SnapshotVec) {
        let children = self.children.get(&node).cloned().unwrap_or_default();

        let kind = self.tags.get(&node).map_or_else(
            || {
                self.text_nodes.get(&node).map_or_else(
                    || {
                        if node == NodeKey::ROOT {
                            LayoutNodeKind::Document
                        } else {
                            LayoutNodeKind::Block {
                                tag: String::from("div"),
                            }
                        }
                    },
                    |text| LayoutNodeKind::InlineText { text: text.clone() },
                )
            },
            |tag| LayoutNodeKind::Block { tag: tag.clone() },
        );

        result.push((node, kind, children.clone()));

        for child in children {
            self.build_snapshot_recursive(child, result);
        }
    }
}

impl Default for LayoutManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DOMSubscriber for LayoutManager {
    fn apply_update(&mut self, update: DOMUpdate) -> Result<()> {
        match update {
            DOMUpdate::InsertElement {
                parent, tag, node, ..
            } => {
                self.tags.insert(node, tag);
                self.children.entry(parent).or_default().push(node);
                if parent == NodeKey::ROOT && self.root.is_none() {
                    self.root = Some(node);
                }
            }
            DOMUpdate::InsertText {
                parent, text, node, ..
            } => {
                self.text_nodes.insert(node, text);
                self.children.entry(parent).or_default().push(node);
            }
            DOMUpdate::SetAttr { node, name, value } => {
                self.attrs.entry(node).or_default().insert(name, value);
            }
            DOMUpdate::RemoveNode { node } => {
                self.tags.remove(&node);
                self.text_nodes.remove(&node);
                self.attrs.remove(&node);
                self.rects.remove(&node);
                // Remove from parent's children
                for children in self.children.values_mut() {
                    children.retain(|child| *child != node);
                }
            }
            DOMUpdate::EndOfDocument => {
                // Layout will be computed when styles are available
            }
        }
        Ok(())
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
