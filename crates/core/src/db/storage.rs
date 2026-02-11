//! Database for CSS property storage.
//!
//! Properties are stored in sparse trees grouped by domain (text,
//! background, box model, layout mode, position). Only DOM nodes
//! that have at least one property in a group appear in that group's
//! tree, so unused nodes take zero memory.

use crate::db::property_group::{PropertyGroup, classify};
use crate::db::sparse_tree::SparseTree;
use crate::db::tree_access::TreeAccess;
use crate::{NodeId, Specificity};
use lightningcss::properties::{Property, PropertyId};
use std::sync::Arc;

/// Central database for CSS property storage.
///
/// Five sparse trees hold properties by domain group. The database
/// owns a reference to the DOM tree for ancestor lookups during
/// insertion and inherited-property queries.
pub struct Database {
    /// Text properties (font, color, text-align, …). Inherited.
    pub text: SparseTree,
    /// Background / visual properties (background-color, border-color, opacity, …).
    pub background: SparseTree,
    /// Box-model properties (width, height, margin, padding, border-width, …).
    pub box_model: SparseTree,
    /// Layout-mode properties (display, flex-*, grid-*, gap, …).
    pub layout: SparseTree,
    /// Positioning properties (position, top/right/bottom/left, z-index).
    pub position: SparseTree,
    /// DOM tree access for parent lookups.
    tree: Arc<dyn TreeAccess>,
}

impl Database {
    /// Create a new database backed by the given DOM tree.
    pub fn new(tree: Arc<dyn TreeAccess>) -> Self {
        Self {
            text: SparseTree::new(),
            background: SparseTree::new(),
            box_model: SparseTree::new(),
            layout: SparseTree::new(),
            position: SparseTree::new(),
            tree,
        }
    }

    /// Get the sparse tree for a property group.
    pub fn tree_for_group(&self, group: PropertyGroup) -> &SparseTree {
        match group {
            PropertyGroup::Text => &self.text,
            PropertyGroup::Background => &self.background,
            PropertyGroup::BoxModel => &self.box_model,
            PropertyGroup::Layout => &self.layout,
            PropertyGroup::Position => &self.position,
        }
    }

    /// Set a property for a node, routing it to the appropriate sparse
    /// tree based on its property group.
    ///
    /// Automatically finds the sparse-tree parent by walking DOM
    /// ancestors when a node is first inserted into a tree.
    ///
    /// Returns `true` if the property value changed.
    pub fn set_property(
        &self,
        node: NodeId,
        property: Property<'static>,
        specificity: Specificity,
    ) -> bool {
        let prop_id = property.property_id();

        let Some(group) = classify(&prop_id) else {
            return false;
        };

        let tree = self.tree_for_group(group);
        let dom_tree = &self.tree;

        tree.set_property(node, property, specificity, |node_id| {
            // Walk up DOM ancestors to find the nearest one already in this sparse tree.
            let mut current = dom_tree.parent(node_id);
            while let Some(ancestor) = current {
                if tree.contains(ancestor) {
                    return Some(ancestor);
                }
                current = dom_tree.parent(ancestor);
            }
            None
        })
    }

    /// Get a property for a node by `PropertyId`.
    ///
    /// For inherited groups (Text), walks up DOM ancestors via the
    /// sparse tree until a value is found. For non-inherited groups,
    /// returns only the value explicitly set on this node.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "matches lightningcss property_id() API"
    )]
    pub fn get_property(
        &self,
        node: NodeId,
        prop_id: PropertyId<'static>,
    ) -> Option<Property<'static>> {
        let group = classify(&prop_id)?;
        let tree = self.tree_for_group(group);

        if group.is_inherited() {
            let dom_tree = &self.tree;
            let ancestors = DomAncestors {
                current: dom_tree.parent(node),
                tree: dom_tree,
            };
            tree.get_inherited_via_dom(node, &prop_id, ancestors)
        } else {
            tree.get_local(node, &prop_id)
        }
    }

    /// Remove all properties for a node from every tree.
    pub fn clear_node(&self, node: NodeId) {
        self.text.remove_node(node);
        self.background.remove_node(node);
        self.box_model.remove_node(node);
        self.layout.remove_node(node);
        self.position.remove_node(node);
    }
}

/// Iterator over DOM ancestors using a `TreeAccess` reference.
struct DomAncestors<'tree> {
    current: Option<NodeId>,
    tree: &'tree Arc<dyn TreeAccess>,
}

impl Iterator for DomAncestors<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.current?;
        self.current = self.tree.parent(node);
        Some(node)
    }
}
