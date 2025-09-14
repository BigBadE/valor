//! LayoutBox tree scaffold: separates DOM nodes from layout boxes.
//!
//! Phase 2 goal is to provide a structural bridge between the DOM (mirrored by
//! Layouter) and box/fragment generation. This module introduces minimal types
//! and a builder that converts the Layouter snapshot + ComputedStyle map into a
//! LayoutBox tree. Behavior is intentionally conservative to avoid changing
//! existing geometry; the box tree primarily filters out non-rendering nodes
//! (display:none, <head>, <style>, etc.) and records inline vs block hints.

use std::collections::HashMap;

use js::NodeKey;
use style_engine::ComputedStyle;

use crate::{Layouter, LayoutNodeKind};

use super::block::is_non_rendering_tag;

/// Stable identifier for a layout box within a tree.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct LayoutBoxId(pub u32);

/// Kinds of layout boxes supported by the scaffold.
#[derive(Debug, Clone)]
pub enum LayoutBoxKind {
    /// A block-level element box (may carry tag name for debugging).
    Block { tag: String },
    /// An inline-level element box (e.g., <span> with display:inline>).
    InlineElement { tag: String },
    /// An inline text leaf box, mirroring a DOM text node.
    InlineText { text: String },
    /// An anonymous block box used to wrap inline runs in block contexts (MVP placeholder).
    AnonymousBlock,
}

/// A node in the LayoutBox tree.
#[derive(Debug, Clone)]
pub struct LayoutBox {
    /// Local identifier within the tree.
    pub id: LayoutBoxId,
    /// Backreference to DOM node key when this box originates from a DOM node.
    /// Anonymous boxes have `None`.
    pub dom_node: Option<NodeKey>,
    /// The kind of box.
    pub kind: LayoutBoxKind,
    /// Parent box if any.
    pub parent: Option<LayoutBoxId>,
    /// Children boxes.
    pub children: Vec<LayoutBoxId>,
}

impl LayoutBox {
    fn new(id: LayoutBoxId, dom_node: Option<NodeKey>, kind: LayoutBoxKind, parent: Option<LayoutBoxId>) -> Self {
        Self { id, dom_node, kind, parent, children: Vec::new() }
    }
}

/// A full tree of layout boxes plus lookups for integration logic.
#[derive(Debug, Clone)]
pub struct LayoutBoxTree {
    /// All boxes stored densely by id index.
    pub boxes: Vec<LayoutBox>,
    /// The root box (corresponds to Document child root placeholder).
    pub root: LayoutBoxId,
    /// Map from DOM NodeKey to primary LayoutBoxId.
    pub node_to_box: HashMap<NodeKey, LayoutBoxId>,
}

impl LayoutBoxTree {
    /// Retrieve a box by id.
    pub fn get(&self, id: LayoutBoxId) -> Option<&LayoutBox> { self.boxes.get(id.0 as usize) }

    /// Push a box and return its id.
    fn push_box(&mut self, dom_node: Option<NodeKey>, kind: LayoutBoxKind, parent: Option<LayoutBoxId>) -> LayoutBoxId {
        let id = LayoutBoxId(self.boxes.len() as u32);
        let bx = LayoutBox::new(id, dom_node, kind, parent);
        self.boxes.push(bx);
        if let Some(p) = parent { self.boxes[p.0 as usize].children.push(id); }
        if let Some(nk) = dom_node { self.node_to_box.entry(nk).or_insert(id); }
        id
    }
}

/// Build a LayoutBox tree from the Layouter snapshot and computed styles.
///
/// Behavior notes:
/// - Filters out non-rendering tags and nodes with display:none (prunes subtree).
/// - Preserves DOM order; does not yet synthesize anonymous block boxes.
/// - Marks inline text nodes as InlineText boxes; element nodes become Block boxes
///   regardless of inline vs block display. Inline-vs-block is handled later by
///   fragment generation and layout algorithms.
pub fn build_layout_box_tree(layouter: &Layouter) -> LayoutBoxTree {
    let snapshot = layouter.snapshot();
    let mut kind_by_key: HashMap<NodeKey, LayoutNodeKind> = HashMap::new();
    let mut children_by_key: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();
    for (key, kind, children) in snapshot.into_iter() {
        kind_by_key.insert(key, kind);
        children_by_key.insert(key, children);
    }
    let computed = layouter.computed_styles();

    let mut tree = LayoutBoxTree { boxes: Vec::new(), root: LayoutBoxId(0), node_to_box: HashMap::new() };
    // Seed root as an anonymous block (not associated with a DOM node).
    let root = tree.push_box(None, LayoutBoxKind::AnonymousBlock, None);
    tree.root = root;

    // DFS from Document root children
    let doc = NodeKey::ROOT;
    if let Some(children) = children_by_key.get(&doc) {
        for child in children {
            build_boxes_rec(*child, root, &kind_by_key, &children_by_key, computed, &mut tree);
        }
    }

    tree
}

fn display_none_for(node: NodeKey, computed: &HashMap<NodeKey, ComputedStyle>) -> bool {
    computed.get(&node).map(|cs| cs.display == style_engine::Display::None).unwrap_or(false)
}

fn should_skip_tag(tag: &str) -> bool { is_non_rendering_tag(tag) }

fn build_boxes_rec(
    node: NodeKey,
    parent_box: LayoutBoxId,
    kind_by_key: &HashMap<NodeKey, LayoutNodeKind>,
    children_by_key: &HashMap<NodeKey, Vec<NodeKey>>,
    computed: &HashMap<NodeKey, ComputedStyle>,
    tree: &mut LayoutBoxTree,
) {
    match kind_by_key.get(&node) {
        Some(LayoutNodeKind::Block { tag }) => {
            if should_skip_tag(tag) || display_none_for(node, computed) { return; }
            // Determine if this element is inline-level or block-level
            let is_inline = computed
                .get(&node)
                .map(|cs| cs.display == style_engine::Display::Inline)
                .unwrap_or(false);
            let kind = if is_inline { LayoutBoxKind::InlineElement { tag: tag.clone() } } else { LayoutBoxKind::Block { tag: tag.clone() } };
            let id = tree.push_box(Some(node), kind, Some(parent_box));
            // For block-level containers, synthesize anonymous blocks around inline runs
            if !is_inline {
                if let Some(children) = children_by_key.get(&node) {
                    let mut current_anon: Option<LayoutBoxId> = None;
                    for child in children {
                        // Determine if child is inline-level
                        let child_inline = match kind_by_key.get(child) {
                            Some(LayoutNodeKind::InlineText { .. }) => true,
                            Some(LayoutNodeKind::Block { .. }) => computed
                                .get(child)
                                .map(|cs| cs.display == style_engine::Display::Inline)
                                .unwrap_or(false),
                            _ => false,
                        };
                        if child_inline {
                            // Ensure an anonymous block exists
                            let anon_id = if let Some(a) = current_anon { a } else { let a = tree.push_box(None, LayoutBoxKind::AnonymousBlock, Some(id)); current_anon = Some(a); a };
                            build_boxes_rec(*child, anon_id, kind_by_key, children_by_key, computed, tree);
                        } else {
                            // Close any open anonymous block
                            current_anon = None;
                            build_boxes_rec(*child, id, kind_by_key, children_by_key, computed, tree);
                        }
                    }
                }
            } else {
                // Inline element: just recurse into children under this inline box
                if let Some(children) = children_by_key.get(&node) {
                    for child in children { build_boxes_rec(*child, id, kind_by_key, children_by_key, computed, tree); }
                }
            }
        }
        Some(LayoutNodeKind::InlineText { text }) => {
            // Inline text always creates a leaf box; parent remains unchanged.
            let _ = tree.push_box(Some(node), LayoutBoxKind::InlineText { text: text.clone() }, Some(parent_box));
        }
        Some(LayoutNodeKind::Document) | None => {
            // Recurse into children, attaching them to the parent box.
            if let Some(children) = children_by_key.get(&node) {
                for child in children { build_boxes_rec(*child, parent_box, kind_by_key, children_by_key, computed, tree); }
            }
        }
    }
}

/// Utility: derive NodeKey-keyed maps from a LayoutBoxTree for reuse of existing
/// layout algorithms during the scaffold phase. Anonymous boxes are elided from
/// the maps since they do not have a DOM NodeKey.
pub fn derive_maps_from_box_tree(
    tree: &LayoutBoxTree,
) -> (HashMap<NodeKey, LayoutNodeKind>, HashMap<NodeKey, Vec<NodeKey>>) {
    // Gather kind and children keyed by DOM NodeKey by projecting the box tree.
    let mut kind_by_key: HashMap<NodeKey, LayoutNodeKind> = HashMap::new();
    let mut children_by_key: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();

    // We'll build children lists by traversing boxes in order and linking DOM-backed boxes.
    for bx in &tree.boxes {
        if let Some(dom_key) = bx.dom_node {
            match &bx.kind {
                LayoutBoxKind::Block { tag } => {
                    kind_by_key.insert(dom_key, LayoutNodeKind::Block { tag: tag.clone() });
                }
                LayoutBoxKind::InlineElement { tag } => {
                    // Map inline element boxes to the same LayoutNodeKind::Block for legacy algorithms
                    kind_by_key.insert(dom_key, LayoutNodeKind::Block { tag: tag.clone() });
                }
                LayoutBoxKind::InlineText { text } => {
                    kind_by_key.insert(dom_key, LayoutNodeKind::InlineText { text: text.clone() });
                }
                LayoutBoxKind::AnonymousBlock => {
                    // no DOM key for anonymous blocks
                }
            }
        }
    }

    fn collect_dom_children(tree: &LayoutBoxTree, id: LayoutBoxId, out: &mut Vec<NodeKey>) {
        if let Some(bx) = tree.get(id) {
            for &child_id in &bx.children {
                let child = &tree.boxes[child_id.0 as usize];
                match (&child.dom_node, &child.kind) {
                    (Some(dom_key), _) => out.push(*dom_key),
                    (None, LayoutBoxKind::AnonymousBlock) => {
                        collect_dom_children(tree, child_id, out);
                    }
                    _ => {}
                }
            }
        }
    }

    for bx in &tree.boxes {
        if let Some(dom_key) = bx.dom_node {
            let mut out_children: Vec<NodeKey> = Vec::new();
            collect_dom_children(tree, bx.id, &mut out_children);
            children_by_key.insert(dom_key, out_children);
        }
    }

    (kind_by_key, children_by_key)
}
