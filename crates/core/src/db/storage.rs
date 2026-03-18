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
use lightningcss::properties::border::LineStyle;
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
    /// Per-node style fingerprint for detecting identical siblings.
    /// Nodes with the same fingerprint have the same set of non-inherited
    /// CSS properties (box-model + layout + position + background).
    /// Updated on each `set_property` call.
    fingerprints: boxcar::Vec<std::sync::atomic::AtomicU64>,
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
            fingerprints: boxcar::Vec::new(),
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

        let changed = tree.set_property(node, property.clone(), specificity, |node_id| {
            // Walk up DOM ancestors to find the nearest one already in this sparse tree.
            let mut current = dom_tree.parent(node_id);
            while let Some(ancestor) = current {
                if tree.contains(ancestor) {
                    return Some(ancestor);
                }
                current = dom_tree.parent(ancestor);
            }
            None
        });

        // Update style fingerprint when a non-inherited property changes.
        if changed && !group.is_inherited() {
            self.update_fingerprint(node, &prop_id);
        }

        changed
    }

    /// Get the style fingerprint for a node.
    ///
    /// Nodes with the same fingerprint have identical non-inherited CSS
    /// properties. Used by the resolver to detect uniform sibling groups
    /// and batch their layout computation.
    pub fn style_fingerprint(&self, node: NodeId) -> u64 {
        let idx = node.0 as usize;
        if idx < self.fingerprints.count() {
            self.fingerprints[idx].load(std::sync::atomic::Ordering::Relaxed)
        } else {
            0
        }
    }

    /// Update the fingerprint for a node by mixing in a property ID.
    fn update_fingerprint(&self, node: NodeId, prop_id: &PropertyId<'static>) {
        let idx = node.0 as usize;
        // Ensure storage exists.
        while self.fingerprints.count() <= idx {
            self.fingerprints
                .push(std::sync::atomic::AtomicU64::new(0));
        }
        // Mix in the property ID discriminant using FNV-1a-like hashing.
        let prop_hash = {
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            let disc = std::mem::discriminant(prop_id);
            let bytes: [u8; std::mem::size_of::<std::mem::Discriminant<PropertyId<'static>>>()]  =
                unsafe { std::mem::transmute_copy(&disc) };
            for byte in bytes {
                h ^= u64::from(byte);
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
            h
        };
        // XOR into existing fingerprint (order-independent).
        self.fingerprints[idx].fetch_xor(prop_hash, std::sync::atomic::Ordering::Relaxed);
    }

    /// Fix sparse-tree relationships for a node after its DOM parent
    /// has been set.
    ///
    /// Call this after `AppendChild` so that nodes which entered a
    /// sparse tree during `CreateNode` (before having a DOM parent)
    /// get their sparse parent pointers corrected.
    pub fn relink_node(&self, node: NodeId) {
        let dom_tree = &self.tree;

        let find_parent = |tree: &SparseTree, node_id: NodeId| -> Option<NodeId> {
            let mut current = dom_tree.parent(node_id);
            while let Some(ancestor) = current {
                if tree.contains(ancestor) {
                    return Some(ancestor);
                }
                current = dom_tree.parent(ancestor);
            }
            None
        };

        let is_ancestor = |ancestor: NodeId, descendant: NodeId| -> bool {
            let mut current = dom_tree.parent(descendant);
            while let Some(candidate) = current {
                if candidate == ancestor {
                    return true;
                }
                current = dom_tree.parent(candidate);
            }
            false
        };

        for tree in [
            &self.text,
            &self.background,
            &self.box_model,
            &self.layout,
            &self.position,
        ] {
            tree.relink_node(node, |node_id| find_parent(tree, node_id), &is_ancestor);
        }
    }

    /// Get a property for a node by `PropertyId`.
    ///
    /// For inherited groups (Text), walks up DOM ancestors via the
    /// sparse tree until a value is found. If no ancestor has the
    /// property, returns the CSS initial value for well-known
    /// inherited properties. For non-inherited groups, returns only
    /// the value explicitly set on this node.
    ///
    /// Applies computed-value dependencies: `border-*-width` returns
    /// `None` when the corresponding `border-*-style` is absent or `none`.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "matches lightningcss property_id() API"
    )]
    pub fn get_property(
        &self,
        node: NodeId,
        prop_id: PropertyId<'static>,
    ) -> Option<Property<'static>> {
        // CSS spec: border-width computes to 0 when border-style is none.
        if is_border_width_prop(&prop_id) && !self.has_border_style(node, &prop_id) {
            return None;
        }

        let group = classify(&prop_id)?;
        let tree = self.tree_for_group(group);

        if group.is_inherited() {
            let dom_tree = &self.tree;
            let ancestors = DomAncestors {
                current: dom_tree.parent(node),
                tree: dom_tree,
            };
            tree.get_inherited_via_dom(node, &prop_id, ancestors)
                .or_else(|| css_initial_value(&prop_id))
        } else {
            tree.get_local(node, &prop_id)
        }
    }

    /// Check if a node has a non-`none` border-style for the side
    /// corresponding to the given border-width property.
    fn has_border_style(&self, node: NodeId, width_prop_id: &PropertyId<'static>) -> bool {
        let style_prop_id = border_style_for_width(width_prop_id);
        // border-style lives in the Background group.
        let Some(prop) = self.background.get_local(node, &style_prop_id) else {
            return false;
        };
        !matches!(
            prop,
            Property::BorderTopStyle(LineStyle::None)
                | Property::BorderRightStyle(LineStyle::None)
                | Property::BorderBottomStyle(LineStyle::None)
                | Property::BorderLeftStyle(LineStyle::None)
        )
    }

    /// Get sparse-tree neighbors (parent, children, siblings) for a
    /// node in the sparse tree that owns the given property. Returns
    /// an empty vec if the property isn't classified or the node isn't
    /// in the relevant tree.
    pub fn neighbors(&self, node: NodeId, prop_id: &PropertyId<'static>) -> Vec<NodeId> {
        let Some(group) = classify(prop_id) else {
            return Vec::new();
        };
        self.tree_for_group(group).neighbors(node)
    }

    /// Get the DOM parent of a node, if any.
    pub fn dom_parent(&self, node: NodeId) -> Option<NodeId> {
        self.tree.parent(node)
    }

    /// Get all direct DOM children of a node.
    pub fn dom_children(&self, node: NodeId) -> Vec<NodeId> {
        self.tree.children(node)
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

/// Return the CSS initial value for well-known inherited properties.
///
/// Per the CSS specification, inherited properties have defined initial
/// values that apply when no ancestor sets the property. This function
/// covers the Text property group (the only inherited group).
fn css_initial_value(prop_id: &PropertyId<'static>) -> Option<Property<'static>> {
    use lightningcss::properties::display::Visibility;
    use lightningcss::properties::font::{
        AbsoluteFontWeight, FontFamily, FontSize, FontStyle, FontWeight, GenericFontFamily,
        LineHeight,
    };
    use lightningcss::properties::text::TextAlign;
    use lightningcss::values::color::{CssColor, RGBA};
    use lightningcss::values::length::LengthValue;
    use lightningcss::values::percentage::DimensionPercentage;

    match prop_id {
        PropertyId::Color => Some(Property::Color(CssColor::RGBA(RGBA::new(0, 0, 0, 1.0)))),
        PropertyId::FontWeight => Some(Property::FontWeight(FontWeight::Absolute(
            AbsoluteFontWeight::Weight(400.0),
        ))),
        PropertyId::FontSize => Some(Property::FontSize(FontSize::Length(
            DimensionPercentage::Dimension(LengthValue::Px(16.0)),
        ))),
        PropertyId::LineHeight => Some(Property::LineHeight(LineHeight::Normal)),
        PropertyId::FontStyle => Some(Property::FontStyle(FontStyle::Normal)),
        PropertyId::FontFamily => Some(Property::FontFamily(vec![FontFamily::Generic(
            GenericFontFamily::SansSerif,
        )])),
        PropertyId::Visibility => Some(Property::Visibility(Visibility::Visible)),
        PropertyId::TextAlign => Some(Property::TextAlign(TextAlign::Start)),
        _ => None,
    }
}

/// Check if a property value is the CSS initial value for its property.
///
/// Properties at their initial value don't need to be stored in the database —
/// the formula system treats missing properties as having their default value.
/// Skipping storage makes the sparse trees genuinely sparse, enabling
/// optimizations like uniform-region detection and tree-skip traversal.
pub fn is_css_initial_value(property: &Property<'static>) -> bool {
    use lightningcss::properties::display::{Display, DisplayInside, DisplayOutside, DisplayPair};
    use lightningcss::properties::size::Size;

    matches!(
        property,
        // display: block is the default for flow elements
        Property::Display(Display::Pair(DisplayPair { outside: DisplayOutside::Block, inside: DisplayInside::Flow, .. }))
        // width/height: auto is the initial value
        | Property::Width(Size::Auto)
        | Property::Height(Size::Auto)
    )
}

/// Check if a property ID is a border-width property.
fn is_border_width_prop(prop_id: &PropertyId<'static>) -> bool {
    matches!(
        prop_id,
        PropertyId::BorderTopWidth
            | PropertyId::BorderRightWidth
            | PropertyId::BorderBottomWidth
            | PropertyId::BorderLeftWidth
    )
}

/// Get the corresponding border-style property ID for a border-width property.
///
/// Callers must ensure `prop_id` is a border-width property.
fn border_style_for_width(prop_id: &PropertyId<'static>) -> PropertyId<'static> {
    match prop_id {
        PropertyId::BorderRightWidth => PropertyId::BorderRightStyle,
        PropertyId::BorderBottomWidth => PropertyId::BorderBottomStyle,
        PropertyId::BorderLeftWidth => PropertyId::BorderLeftStyle,
        // BorderTopWidth and any other (unreachable in practice).
        _ => PropertyId::BorderTopStyle,
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
