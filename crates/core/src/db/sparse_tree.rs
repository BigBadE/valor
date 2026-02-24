//! Sparse property tree.
//!
//! A sparse tree stores CSS properties for only the DOM nodes that
//! participate in a given property group. Nodes with no relevant
//! properties take zero memory.
//!
//! The tree maintains its own parent/child/sibling relationships that
//! shortcut through DOM nodes not present in the tree. Inheritance
//! (for groups like Text) walks these sparse-tree parents instead of
//! the full DOM.

use crate::{NodeId, Specificity};
use boxcar::Vec as BoxcarVec;
use dashmap::DashMap;
use lightningcss::properties::{Property, PropertyId};
use std::sync::atomic::{AtomicU32, Ordering};

/// Sentinel value: no node.
const NO_NODE: u32 = u32::MAX;

/// Index into a `SparseTree`'s internal storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LocalId(pub u32);

/// A single property with its cascade specificity.
#[derive(Clone)]
pub struct SparseEntry {
    pub property: Property<'static>,
    pub specificity: Specificity,
}

/// Relationships for a node within the sparse tree.
pub struct SparseRelationships {
    pub parent: AtomicU32,
    pub first_child: AtomicU32,
    pub next_sibling: AtomicU32,
}

impl Default for SparseRelationships {
    fn default() -> Self {
        Self {
            parent: AtomicU32::new(NO_NODE),
            first_child: AtomicU32::new(NO_NODE),
            next_sibling: AtomicU32::new(NO_NODE),
        }
    }
}

/// A sparse tree for one property group.
///
/// Only DOM nodes that have at least one property in this group are
/// stored. The tree maintains its own relationship pointers that skip
/// over DOM nodes not present in this group.
pub struct SparseTree {
    /// Per-node property storage, indexed by `LocalId`.
    props: BoxcarVec<DashMap<PropertyId<'static>, SparseEntry>>,

    /// Relationship pointers within this sparse tree, indexed by `LocalId`.
    relationships: BoxcarVec<SparseRelationships>,

    /// `NodeId` to `LocalId` mapping (only for nodes present in this tree).
    dom_to_local: DashMap<NodeId, LocalId>,

    /// `LocalId` to `NodeId` reverse mapping, indexed by `LocalId`.
    local_to_dom: BoxcarVec<NodeId>,
}

impl SparseTree {
    /// Create a new empty sparse tree.
    pub fn new() -> Self {
        Self {
            props: BoxcarVec::new(),
            relationships: BoxcarVec::new(),
            dom_to_local: DashMap::new(),
            local_to_dom: BoxcarVec::new(),
        }
    }

    /// Check if a DOM node is present in this tree.
    pub fn contains(&self, node: NodeId) -> bool {
        self.dom_to_local.contains_key(&node)
    }

    /// Get the local ID for a DOM node, if present.
    pub fn local_id(&self, node: NodeId) -> Option<LocalId> {
        self.dom_to_local.get(&node).map(|ref_val| *ref_val)
    }

    /// Get the DOM node ID for a local ID.
    pub fn dom_id(&self, local: LocalId) -> NodeId {
        self.local_to_dom[local.0 as usize]
    }

    /// Insert a DOM node into this sparse tree (without relationships).
    /// Returns the new `LocalId`. If already present, returns existing.
    fn ensure_node(&self, node: NodeId) -> LocalId {
        if let Some(existing) = self.dom_to_local.get(&node) {
            return *existing;
        }
        let idx = self.props.push(DashMap::new()) as u32;
        self.relationships.push(SparseRelationships::default());
        self.local_to_dom.push(node);
        let local = LocalId(idx);
        self.dom_to_local.insert(node, local);
        local
    }

    /// Set a property on a DOM node, inserting the node into the tree
    /// if it isn't already present.
    ///
    /// `find_sparse_parent` is called lazily only when the node needs
    /// to be inserted — it should walk up the DOM tree and return the
    /// nearest ancestor that is already in this sparse tree (if any).
    ///
    /// Returns `true` if the property value changed.
    pub fn set_property(
        &self,
        node: NodeId,
        property: Property<'static>,
        specificity: Specificity,
        find_sparse_parent: impl FnOnce(NodeId) -> Option<NodeId>,
    ) -> bool {
        let prop_id = property.property_id();
        let is_new = !self.contains(node);
        let local = self.ensure_node(node);

        // Wire up relationships if this is a new node in the tree.
        if is_new
            && let Some(parent_dom) = find_sparse_parent(node)
            && let Some(parent_local) = self.local_id(parent_dom)
        {
            self.link_child(parent_local, local);
        }

        let node_props = &self.props[local.0 as usize];

        // Cascade: skip if existing entry has higher specificity.
        if let Some(existing) = node_props.get(&prop_id) {
            if specificity < existing.specificity {
                return false;
            }
            if existing.property == property {
                // Same value, just update specificity if needed.
                drop(existing);
                node_props.insert(
                    prop_id,
                    SparseEntry {
                        property,
                        specificity,
                    },
                );
                return false;
            }
        }

        node_props.insert(
            prop_id,
            SparseEntry {
                property,
                specificity,
            },
        );
        true
    }

    /// Get a property for a DOM node (no inheritance — just this node).
    pub fn get_local(
        &self,
        node: NodeId,
        prop_id: &PropertyId<'static>,
    ) -> Option<Property<'static>> {
        let local = self.local_id(node)?;
        let node_props = &self.props[local.0 as usize];
        node_props.get(prop_id).map(|entry| entry.property.clone())
    }

    /// Get a property with inheritance: check this node, then walk up
    /// sparse-tree parents until found or root is reached.
    ///
    /// For non-inherited groups, just call `get_local` instead.
    pub fn get_inherited(
        &self,
        node: NodeId,
        prop_id: &PropertyId<'static>,
    ) -> Option<Property<'static>> {
        let local = self.local_id(node)?;
        self.walk_up_for_property(local, prop_id)
    }

    /// Get an inherited property for a DOM node that may not be in this
    /// tree, by walking DOM ancestors.
    ///
    /// `dom_ancestors` should yield successive parent `NodeId` values
    /// starting from the node's DOM parent.
    pub fn get_inherited_via_dom(
        &self,
        node: NodeId,
        prop_id: &PropertyId<'static>,
        dom_ancestors: impl Iterator<Item = NodeId>,
    ) -> Option<Property<'static>> {
        // Check the node itself first.
        if let Some(local) = self.local_id(node)
            && let Some(val) = self.walk_up_for_property(local, prop_id)
        {
            return Some(val);
        }

        // Walk DOM ancestors until we find one in this tree.
        for ancestor in dom_ancestors {
            if let Some(ancestor_local) = self.local_id(ancestor) {
                return self.walk_up_for_property(ancestor_local, prop_id);
            }
        }
        None
    }

    /// Fix sparse-tree relationships for a node after its DOM parent
    /// has been set (i.e. after `AppendChild`).
    ///
    /// When a node enters the sparse tree during `CreateNode`, it may
    /// not yet have a DOM parent, so its sparse parent is set to None.
    /// Once the DOM parent link is established, this method:
    /// 1. Finds the correct sparse parent by walking DOM ancestors.
    /// 2. Links the node to that parent.
    /// 3. Adopts any existing root/sibling nodes that are DOM
    ///    descendants of this node.
    ///
    /// `find_sparse_parent` walks DOM ancestors to find the nearest
    /// one in this sparse tree. `is_dom_ancestor(a, b)` returns true
    /// if `a` is a DOM ancestor of `b`.
    pub fn relink_node(
        &self,
        node: NodeId,
        find_sparse_parent: impl FnOnce(NodeId) -> Option<NodeId>,
        is_dom_ancestor: &impl Fn(NodeId, NodeId) -> bool,
    ) {
        let Some(local) = self.local_id(node) else {
            return;
        };

        let current_parent = self.relationships[local.0 as usize]
            .parent
            .load(Ordering::Acquire);

        // Only fix nodes that have no sparse parent. If they already
        // have one, it was correctly set at insertion time.
        if current_parent != NO_NODE {
            return;
        }

        let new_parent_local =
            find_sparse_parent(node).and_then(|parent_dom| self.local_id(parent_dom));

        if let Some(parent) = new_parent_local {
            self.link_child(parent, local);
        }

        // Adopt root nodes (sparse parent == None) that are DOM
        // descendants of this node. This handles the case where
        // this node's children entered the tree before this node
        // got its DOM parent.
        self.adopt_descendants(local, new_parent_local, is_dom_ancestor);
    }

    /// Remove all properties for a DOM node and unlink it from the tree.
    pub fn remove_node(&self, node: NodeId) {
        if let Some((_, local)) = self.dom_to_local.remove(&node) {
            // Re-parent children to this node's parent before removing.
            let my_parent = self.relationships[local.0 as usize]
                .parent
                .load(Ordering::Acquire);

            let mut child_local_id = self.relationships[local.0 as usize]
                .first_child
                .load(Ordering::Acquire);

            while child_local_id != NO_NODE {
                let child_rel = &self.relationships[child_local_id as usize];
                child_rel.parent.store(my_parent, Ordering::Release);
                let next = child_rel.next_sibling.load(Ordering::Acquire);
                if next == NO_NODE && my_parent != NO_NODE {
                    // Last child of removed node — link to parent's children.
                    let parent_rel = &self.relationships[my_parent as usize];
                    let old_first = parent_rel.first_child.swap(
                        self.relationships[local.0 as usize]
                            .first_child
                            .load(Ordering::Acquire),
                        Ordering::AcqRel,
                    );
                    child_rel.next_sibling.store(old_first, Ordering::Release);
                }
                child_local_id = next;
            }

            // Clear the removed node's storage.
            self.props[local.0 as usize].clear();
        }
    }

    /// Walk from `local` upward through sparse-tree parents looking for `prop_id`.
    fn walk_up_for_property(
        &self,
        local: LocalId,
        prop_id: &PropertyId<'static>,
    ) -> Option<Property<'static>> {
        if let Some(val) = self.get_at_local(local, prop_id) {
            return Some(val);
        }
        let mut current = self.sparse_parent(local);
        while let Some(parent_local) = current {
            if let Some(val) = self.get_at_local(parent_local, prop_id) {
                return Some(val);
            }
            current = self.sparse_parent(parent_local);
        }
        None
    }

    /// Get the sparse-tree parent's `LocalId`.
    fn sparse_parent(&self, local: LocalId) -> Option<LocalId> {
        let parent_raw = self.relationships[local.0 as usize]
            .parent
            .load(Ordering::Acquire);
        if parent_raw == NO_NODE {
            None
        } else {
            Some(LocalId(parent_raw))
        }
    }

    /// Get a property at a specific local ID.
    fn get_at_local(
        &self,
        local: LocalId,
        prop_id: &PropertyId<'static>,
    ) -> Option<Property<'static>> {
        self.props[local.0 as usize]
            .get(prop_id)
            .map(|entry| entry.property.clone())
    }

    /// Re-parent children of `old_parent` that are DOM descendants of
    /// the newly inserted node `new_node`.
    ///
    /// When a new node is inserted into the sparse tree, some existing
    /// nodes that were children of the new node's sparse parent may
    /// actually be DOM descendants of the new node. Those nodes should
    /// become children of the new node instead.
    ///
    /// `old_parent` is the sparse parent of `new_node` (None if root).
    /// `is_dom_ancestor(ancestor, descendant)` returns true if
    /// `ancestor` is a DOM ancestor of `descendant`.
    fn adopt_descendants(
        &self,
        new_node: LocalId,
        old_parent: Option<LocalId>,
        is_dom_ancestor: &impl Fn(NodeId, NodeId) -> bool,
    ) {
        let new_dom = self.dom_id(new_node);
        let to_adopt = old_parent.map_or_else(
            || self.collect_orphan_descendants(new_node, new_dom, is_dom_ancestor),
            |parent| self.steal_children_from(parent, new_node, new_dom, is_dom_ancestor),
        );

        for child in to_adopt {
            self.link_child(new_node, child);
        }
    }

    /// Scan `parent`'s child list and unlink any child that is a DOM
    /// descendant of `new_dom`, returning them for re-parenting.
    /// Skips `skip` (the newly inserted node itself).
    fn steal_children_from(
        &self,
        parent: LocalId,
        skip: LocalId,
        new_dom: NodeId,
        is_dom_ancestor: &impl Fn(NodeId, NodeId) -> bool,
    ) -> Vec<LocalId> {
        let mut stolen = Vec::new();
        let parent_rel = &self.relationships[parent.0 as usize];
        let mut prev: Option<LocalId> = None;
        let mut current_raw = parent_rel.first_child.load(Ordering::Acquire);

        while current_raw != NO_NODE {
            let current = LocalId(current_raw);
            let next_raw = self.relationships[current_raw as usize]
                .next_sibling
                .load(Ordering::Acquire);

            if current == skip {
                prev = Some(current);
                current_raw = next_raw;
                continue;
            }

            let current_dom = self.dom_id(current);
            if is_dom_ancestor(new_dom, current_dom) {
                self.unlink_sibling(parent_rel, prev, current, next_raw);
                stolen.push(current);
            } else {
                prev = Some(current);
            }

            current_raw = next_raw;
        }

        stolen
    }

    /// Scan all root nodes (sparse parent == None) for DOM descendants
    /// of `new_dom`, returning them for re-parenting.
    fn collect_orphan_descendants(
        &self,
        new_node: LocalId,
        new_dom: NodeId,
        is_dom_ancestor: &impl Fn(NodeId, NodeId) -> bool,
    ) -> Vec<LocalId> {
        let mut found = Vec::new();
        for idx in 0..self.relationships.count() {
            let local = LocalId(idx as u32);
            if local == new_node {
                continue;
            }
            let parent_raw = self.relationships[idx].parent.load(Ordering::Acquire);
            if parent_raw != NO_NODE {
                continue;
            }
            let current_dom = self.dom_id(local);
            if is_dom_ancestor(new_dom, current_dom) {
                found.push(local);
            }
        }
        found
    }

    /// Remove `current` from its sibling list by linking `prev` (or the
    /// parent's `first_child`) to `next_raw`.
    fn unlink_sibling(
        &self,
        parent_rel: &SparseRelationships,
        prev: Option<LocalId>,
        current: LocalId,
        next_raw: u32,
    ) {
        if let Some(prev_local) = prev {
            self.relationships[prev_local.0 as usize]
                .next_sibling
                .store(next_raw, Ordering::Release);
        } else {
            parent_rel.first_child.store(next_raw, Ordering::Release);
        }
        self.relationships[current.0 as usize]
            .next_sibling
            .store(NO_NODE, Ordering::Release);
    }

    /// Link `child` as a child of `parent` within the sparse tree.
    fn link_child(&self, parent: LocalId, child: LocalId) {
        let child_rel = &self.relationships[child.0 as usize];
        child_rel.parent.store(parent.0, Ordering::Release);

        let parent_rel = &self.relationships[parent.0 as usize];
        let old_first = parent_rel.first_child.swap(child.0, Ordering::AcqRel);
        if old_first != NO_NODE {
            child_rel.next_sibling.store(old_first, Ordering::Release);
        }
    }

    /// Collect the sparse-tree neighbors of a node: parent, children,
    /// and siblings. Returns only nodes present in this sparse tree.
    pub fn neighbors(&self, node: NodeId) -> Vec<NodeId> {
        let Some(local) = self.local_id(node) else {
            return Vec::new();
        };

        let mut result = Vec::new();
        let rel = &self.relationships[local.0 as usize];

        // Parent
        let parent_raw = rel.parent.load(Ordering::Acquire);
        if parent_raw != NO_NODE {
            result.push(self.dom_id(LocalId(parent_raw)));
        }

        // Children
        let mut child_raw = rel.first_child.load(Ordering::Acquire);
        while child_raw != NO_NODE {
            result.push(self.dom_id(LocalId(child_raw)));
            child_raw = self.relationships[child_raw as usize]
                .next_sibling
                .load(Ordering::Acquire);
        }

        // Siblings (walk parent's children, skip self)
        if parent_raw != NO_NODE {
            let parent_rel = &self.relationships[parent_raw as usize];
            let mut sib_raw = parent_rel.first_child.load(Ordering::Acquire);
            while sib_raw != NO_NODE {
                if sib_raw != local.0 {
                    result.push(self.dom_id(LocalId(sib_raw)));
                }
                sib_raw = self.relationships[sib_raw as usize]
                    .next_sibling
                    .load(Ordering::Acquire);
            }
        }

        result
    }
}

impl Default for SparseTree {
    fn default() -> Self {
        Self::new()
    }
}
