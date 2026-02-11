//! Paint tree traversal in correct CSS paint order.
//!
//! Implements depth-first traversal respecting stacking contexts.

use super::stacking::{StackingContext, StackingLevel};
use std::collections::HashMap;
use std::hash::BuildHasher;

/// Unique identifier for a layout node.
pub type NodeId = u64;

/// Paint order entry for a single node.
#[derive(Debug, Clone)]
pub struct PaintOrder {
    /// Node identifier.
    pub node_id: NodeId,
    /// Stacking context for this node.
    pub stacking_context: StackingContext,
    /// Depth in the tree (for debugging).
    pub depth: u32,
}

/// Paint tree node representing the layout tree structure.
#[derive(Debug, Clone)]
pub struct PaintNode {
    /// Node identifier.
    pub id: NodeId,
    /// Parent node ID.
    pub parent: Option<NodeId>,
    /// Child node IDs.
    pub children: Vec<NodeId>,
    /// Stacking context established by this node.
    pub stacking_context: StackingContext,
}

/// Traverse a paint tree in correct CSS paint order.
///
/// Returns a vec of nodes sorted by paint order (back to front).
/// Nodes are visited depth-first within each stacking level.
///
/// # Arguments
/// * `root` - Root node ID
/// * `nodes` - Map of node ID to paint node
///
/// # Returns
/// Vector of paint order entries sorted from back to front
#[must_use]
pub fn traverse_paint_tree<S: BuildHasher>(
    root: NodeId,
    nodes: &HashMap<NodeId, PaintNode, S>,
) -> Vec<PaintOrder> {
    let mut result = Vec::new();
    traverse_node(root, nodes, 0, &mut result);
    result
}

/// Recursively traverse a node and its children.
fn traverse_node<S: BuildHasher>(
    node_id: NodeId,
    nodes: &HashMap<NodeId, PaintNode, S>,
    depth: u32,
    result: &mut Vec<PaintOrder>,
) {
    let Some(node) = nodes.get(&node_id) else {
        return;
    };

    // If this node establishes a stacking context, we need to:
    // 1. Paint the node's background/borders first
    // 2. Paint children sorted by stacking level
    // 3. Paint the node's foreground last (if any)

    if node.stacking_context.establishes_stacking_context {
        // Paint root background/borders
        result.push(PaintOrder {
            node_id,
            stacking_context: StackingContext::new(
                StackingLevel::RootBackgroundAndBorders,
                node.stacking_context.tree_order,
            ),
            depth,
        });

        // Collect children by stacking level
        let mut children_by_level: Vec<(NodeId, StackingContext)> = node
            .children
            .iter()
            .filter_map(|child_id| {
                nodes
                    .get(child_id)
                    .map(|child| (*child_id, child.stacking_context.clone()))
            })
            .collect();

        // Sort children by stacking context
        children_by_level.sort_by(|first, second| first.1.cmp(&second.1));

        // Traverse children in stacking order
        for (child_id, _) in children_by_level {
            traverse_node(child_id, nodes, depth + 1, result);
        }
    } else {
        // Non-stacking-context nodes paint themselves then children in tree order
        result.push(PaintOrder {
            node_id,
            stacking_context: node.stacking_context.clone(),
            depth,
        });

        // Traverse children in tree order
        for child_id in &node.children {
            traverse_node(*child_id, nodes, depth + 1, result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_node(
        id: NodeId,
        parent: Option<NodeId>,
        children: Vec<NodeId>,
        level: StackingLevel,
        tree_order: u32,
    ) -> (NodeId, PaintNode) {
        (
            id,
            PaintNode {
                id,
                parent,
                children,
                stacking_context: StackingContext::new(level, tree_order),
            },
        )
    }

    /// Test that paint tree traversal respects simple tree order.
    ///
    /// # Panics
    /// Panics if the traversal order is incorrect.
    #[test]
    fn simple_tree_order() {
        let mut nodes = HashMap::new();
        nodes.insert(
            0,
            PaintNode {
                id: 0,
                parent: None,
                children: vec![1, 2],
                stacking_context: StackingContext::root(),
            },
        );
        let (id1, first_child) =
            create_node(1, Some(0), vec![], StackingLevel::BlockDescendants, 0);
        nodes.insert(id1, first_child);
        let (id2, second_child) =
            create_node(2, Some(0), vec![], StackingLevel::BlockDescendants, 1);
        nodes.insert(id2, second_child);

        let order = traverse_paint_tree(0, &nodes);
        assert_eq!(order.len(), 3);
        // Root background, then child 1, then child 2
        assert_eq!(order[0].node_id, 0);
        assert_eq!(order[1].node_id, 1);
        assert_eq!(order[2].node_id, 2);
    }

    /// Test that paint tree traversal correctly orders nodes by z-index.
    ///
    /// # Panics
    /// Panics if z-index ordering is not correct.
    #[test]
    fn z_index_ordering() {
        let mut nodes = HashMap::new();
        nodes.insert(
            0,
            PaintNode {
                id: 0,
                parent: None,
                children: vec![1, 2, 3],
                stacking_context: StackingContext::root(),
            },
        );
        let (id1, positive_z_child) =
            create_node(1, Some(0), vec![], StackingLevel::PositiveZIndex(10), 0);
        nodes.insert(id1, positive_z_child);
        let (id2, negative_z_child) =
            create_node(2, Some(0), vec![], StackingLevel::NegativeZIndex(-5), 1);
        nodes.insert(id2, negative_z_child);
        let (id3, block_child) =
            create_node(3, Some(0), vec![], StackingLevel::BlockDescendants, 2);
        nodes.insert(id3, block_child);

        let order = traverse_paint_tree(0, &nodes);
        assert_eq!(order.len(), 4);
        // Root background, negative z-index, block, positive z-index
        assert_eq!(order[0].node_id, 0); // root
        assert_eq!(order[1].node_id, 2); // z-index: -5
        assert_eq!(order[2].node_id, 3); // normal flow
        assert_eq!(order[3].node_id, 1); // z-index: 10
    }
}
