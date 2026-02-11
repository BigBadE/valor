//! DOM tree structure using boxcar::Vec with atomic relationships.

use crate::types::{DomUpdate, NodeData};
use lasso::ThreadedRodeo;
use rewrite_core::NodeId;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

/// Sentinel value indicating no node (used for parent of root).
const NO_NODE: u32 = u32::MAX;

/// Relationships for a single node.
pub struct NodeRelationships {
    pub parent: AtomicU32,
    pub first_child: AtomicU32,
    pub next_sibling: AtomicU32,
}

impl Default for NodeRelationships {
    fn default() -> Self {
        Self {
            parent: AtomicU32::new(NO_NODE),
            first_child: AtomicU32::new(NO_NODE),
            next_sibling: AtomicU32::new(NO_NODE),
        }
    }
}

/// DOM tree using boxcar::Vec for lock-free concurrent appends.
pub struct DomTree {
    pub nodes: boxcar::Vec<NodeData>,
    pub relationships: boxcar::Vec<NodeRelationships>,
    pub interner: Arc<ThreadedRodeo>,
}

impl DomTree {
    pub fn new(interner: Arc<ThreadedRodeo>) -> Self {
        Self {
            nodes: boxcar::Vec::new(),
            relationships: boxcar::Vec::new(),
            interner,
        }
    }

    pub fn apply_update(&self, update: DomUpdate) -> NodeId {
        match update {
            DomUpdate::CreateNode(data) => {
                let idx = self.nodes.push(data);
                self.relationships.push(Default::default());
                NodeId(idx as u32)
            }
            DomUpdate::AppendChild { parent, child } => {
                let child_rel = &self.relationships[child.0 as usize];
                child_rel.parent.store(parent.0, Ordering::Release);

                let parent_rel = &self.relationships[parent.0 as usize];
                let old_first = parent_rel.first_child.swap(child.0, Ordering::AcqRel);
                if old_first != NO_NODE {
                    child_rel.next_sibling.store(old_first, Ordering::Release);
                }
                child
            }
        }
    }

    /// Get the parent of a node, if it has one.
    pub fn parent(&self, node: NodeId) -> Option<NodeId> {
        let parent_id = self.relationships[node.0 as usize]
            .parent
            .load(Ordering::Acquire);
        if parent_id == NO_NODE || parent_id == node.0 {
            None
        } else {
            Some(NodeId(parent_id))
        }
    }

    /// Get the first child of a node, if it has one.
    pub fn first_child(&self, node: NodeId) -> Option<NodeId> {
        let fc = self.relationships[node.0 as usize]
            .first_child
            .load(Ordering::Acquire);
        if fc == NO_NODE {
            None
        } else {
            Some(NodeId(fc))
        }
    }

    /// Get the next sibling of a node, if it has one.
    pub fn next_sibling(&self, node: NodeId) -> Option<NodeId> {
        let ns = self.relationships[node.0 as usize]
            .next_sibling
            .load(Ordering::Acquire);
        if ns == NO_NODE {
            None
        } else {
            Some(NodeId(ns))
        }
    }

    /// Iterate over all children of a node.
    pub fn children(&self, node: NodeId) -> ChildrenIter<'_> {
        ChildrenIter {
            tree: self,
            current: self.first_child(node),
        }
    }

    /// Get the index of a node among its siblings (0-based).
    pub fn sibling_index(&self, node: NodeId) -> usize {
        let Some(parent) = self.parent(node) else {
            return 0;
        };
        self.children(parent).position(|n| n == node).unwrap_or(0)
    }

    /// Iterate over previous siblings of a node (in reverse order, closest first).
    pub fn prev_siblings(&self, node: NodeId) -> PrevSiblingsIter {
        let Some(parent) = self.parent(node) else {
            return PrevSiblingsIter {
                siblings: Vec::new(),
                index: 0,
            };
        };
        // Collect all siblings before this node
        let siblings: Vec<NodeId> = self.children(parent).take_while(|&n| n != node).collect();
        let index = siblings.len();
        PrevSiblingsIter { siblings, index }
    }

    /// Iterate over next siblings of a node.
    pub fn next_siblings(&self, node: NodeId) -> NextSiblingsIter<'_> {
        // Find this node's next sibling and iterate from there
        NextSiblingsIter {
            tree: self,
            current: self.next_sibling(node),
        }
    }
}

/// Iterator over children of a node.
pub struct ChildrenIter<'a> {
    tree: &'a DomTree,
    current: Option<NodeId>,
}

impl Iterator for ChildrenIter<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.current?;
        self.current = self.tree.next_sibling(node);
        Some(node)
    }
}

/// Iterator over previous siblings (closest first).
pub struct PrevSiblingsIter {
    siblings: Vec<NodeId>,
    index: usize,
}

impl Iterator for PrevSiblingsIter {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index == 0 {
            return None;
        }
        self.index -= 1;
        Some(self.siblings[self.index])
    }
}

/// Iterator over next siblings.
pub struct NextSiblingsIter<'a> {
    tree: &'a DomTree,
    current: Option<NodeId>,
}

impl Iterator for NextSiblingsIter<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.current?;
        self.current = self.tree.next_sibling(node);
        Some(node)
    }
}

impl rewrite_core::TreeAccess for DomTree {
    fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.parent(node)
    }
}
