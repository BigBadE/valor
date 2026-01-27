//! Render tiles for parallel display list generation.
//!
//! Each tile represents a portion of the render tree that can be
//! processed independently in parallel using Rayon work-stealing.

use rewrite_core::NodeId;

/// A tile containing a subset of nodes for parallel rendering.
#[derive(Debug, Clone)]
pub struct RenderTile {
    /// Root node of this tile (typically a stacking context root).
    root: NodeId,

    /// All nodes contained in this tile.
    nodes: Vec<NodeId>,
}

impl RenderTile {
    /// Create a new render tile with the given root.
    pub fn new(root: NodeId) -> Self {
        Self {
            root,
            nodes: vec![root],
        }
    }

    /// Get the root node of this tile.
    #[inline]
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Get all nodes in this tile.
    #[inline]
    pub fn nodes(&self) -> &[NodeId] {
        &self.nodes
    }

    /// Add a node to this tile.
    pub fn add_node(&mut self, node: NodeId) {
        if !self.nodes.contains(&node) {
            self.nodes.push(node);
        }
    }

    /// Remove a node from this tile.
    pub fn remove_node(&mut self, node: NodeId) {
        self.nodes.retain(|&n| n != node);
    }

    /// Check if this tile contains a node.
    #[inline]
    pub fn contains(&self, node: NodeId) -> bool {
        self.nodes.contains(&node)
    }

    /// Get the number of nodes in this tile.
    #[inline]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if the tile is empty (only has root).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.nodes.len() <= 1
    }

    /// Clear all nodes except the root.
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.nodes.push(self.root);
    }
}
