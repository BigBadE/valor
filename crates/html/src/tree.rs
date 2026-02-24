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

    /// Get the index of a node among its siblings (0-based, DOM order).
    pub fn sibling_index(&self, node: NodeId) -> usize {
        let Some(parent) = self.parent(node) else {
            return 0;
        };
        // children() is in reverse DOM order, so count how many
        // siblings come AFTER this node in the iterator (= before in DOM).
        let all: Vec<NodeId> = self.children(parent).collect();
        let rev_pos = all.iter().position(|&n| n == node).unwrap_or(0);
        all.len().saturating_sub(1) - rev_pos
    }

    /// Iterate over previous siblings of a node in DOM order (closest first).
    ///
    /// Note: `children()` iterates in reverse insertion order (last-appended
    /// first), so DOM-previous siblings are those that appear AFTER `node`
    /// in the `children()` iterator.
    pub fn prev_siblings(&self, node: NodeId) -> PrevSiblingsIter {
        let Some(parent) = self.parent(node) else {
            return PrevSiblingsIter {
                siblings: Vec::new(),
                index: 0,
            };
        };
        // children() yields reverse DOM order. Skip until we find `node`,
        // then everything after it in the iterator is a DOM-previous sibling.
        let siblings: Vec<NodeId> = self
            .children(parent)
            .skip_while(|&n| n != node)
            .skip(1) // skip `node` itself
            .collect();
        let index = siblings.len();
        PrevSiblingsIter { siblings, index }
    }

    /// Get the node data for a node by ID.
    pub fn get_node(&self, node: NodeId) -> Option<&NodeData> {
        let idx = node.0 as usize;
        if idx < self.nodes.count() {
            Some(&self.nodes[idx])
        } else {
            None
        }
    }

    /// Get the text content of a node, if it is a text node.
    pub fn text_content(&self, node: NodeId) -> Option<&str> {
        match self.get_node(node)? {
            NodeData::Text(text) => Some(text),
            _ => None,
        }
    }

    /// Iterate over next siblings of a node in DOM order.
    ///
    /// Note: `children()` iterates in reverse insertion order, so
    /// DOM-next siblings are those that appear BEFORE `node` in
    /// the `children()` iterator (collected and reversed).
    pub fn next_siblings(&self, node: NodeId) -> NextSiblingsIter {
        let Some(parent) = self.parent(node) else {
            return NextSiblingsIter {
                siblings: Vec::new(),
                index: 0,
            };
        };
        // children() yields reverse DOM order. Nodes before `node`
        // in the iterator are DOM-later siblings.
        let mut siblings: Vec<NodeId> = self.children(parent).take_while(|&n| n != node).collect();
        siblings.reverse(); // reverse so closest-next comes first
        let index = 0;
        NextSiblingsIter { siblings, index }
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
pub struct NextSiblingsIter {
    siblings: Vec<NodeId>,
    index: usize,
}

impl Iterator for NextSiblingsIter {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.siblings.len() {
            return None;
        }
        let node = self.siblings[self.index];
        self.index += 1;
        Some(node)
    }
}

impl rewrite_core::TreeAccess for DomTree {
    fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.parent(node)
    }

    fn children(&self, node: NodeId) -> Vec<NodeId> {
        self.children(node).collect()
    }
}
