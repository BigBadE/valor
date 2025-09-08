use anyhow::Error;
use html::dom::updating::{DOMUpdate, DOMSubscriber, DOMMirror};
use html::dom::NodeKey;
use log::{debug, trace, warn};
use std::collections::HashMap;
use css::types::Stylesheet;
use style_engine::ComputedStyle;

mod layout;
mod printing;

pub use layout::LayoutRect;

#[derive(Debug, Clone)]
pub enum LayoutNodeKind {
    Document,
    Block { tag: String },
    InlineText { text: String },
}

#[derive(Debug, Clone)]
pub struct LayoutNode {
    pub kind: LayoutNodeKind,
    pub attrs: HashMap<String, String>,
    pub parent: Option<NodeKey>,
    pub children: Vec<NodeKey>,
}

impl LayoutNode {
    fn new_document() -> Self {
        Self { kind: LayoutNodeKind::Document, attrs: HashMap::new(), parent: None, children: Vec::new() }
    }
    fn new_block(tag: String, parent: Option<NodeKey>) -> Self {
        Self { kind: LayoutNodeKind::Block { tag }, attrs: HashMap::new(), parent, children: Vec::new() }
    }
    fn new_text(text: String, parent: Option<NodeKey>) -> Self {
        Self { kind: LayoutNodeKind::InlineText { text }, attrs: HashMap::new(), parent, children: Vec::new() }
    }
}

pub struct Layouter {
    nodes: HashMap<NodeKey, LayoutNode>,
    root: NodeKey,
    stylesheet: Stylesheet,
    computed: HashMap<NodeKey, ComputedStyle>,
}

impl Layouter {
    pub fn new() -> Self {
        let mut nodes = HashMap::new();
        // Seed root node
        nodes.insert(NodeKey::ROOT, LayoutNode::new_document());
        Self { nodes, root: NodeKey::ROOT, stylesheet: Stylesheet::default(), computed: HashMap::new() }
    }

    pub fn root(&self) -> NodeKey { self.root }

    /// Replace the current computed styles snapshot (from StyleEngine).
    pub fn set_computed_styles(&mut self, map: HashMap<NodeKey, ComputedStyle>) {
        self.computed = map;
    }

    /// Read-only access to computed styles.
    pub fn computed_styles(&self) -> &HashMap<NodeKey, ComputedStyle> { &self.computed }

    /// Return a cloned map of attributes per node (for layout/style resolution).
    pub fn attrs_map(&self) -> HashMap<NodeKey, HashMap<String, String>> {
        let mut out = HashMap::new();
        for (k, n) in self.nodes.iter() {
            out.insert(*k, n.attrs.clone());
        }
        out
    }

    /// Internal implementation for applying a single DOM update to the layout tree mirror.
    fn apply_update_impl(&mut self, update: DOMUpdate) -> Result<(), Error> {
        use DOMUpdate::*;
        match update {
            InsertElement { parent, node, tag, pos } => {
                trace!("InsertElement parent={:?} node={:?} tag={} pos={}", parent, node, tag, pos);
                self.ensure_parent_exists(parent);
                {
                    let entry = self
                        .nodes
                        .entry(node)
                        .or_insert_with(|| LayoutNode::new_block(tag.clone(), Some(parent)));
                    entry.kind = LayoutNodeKind::Block { tag };
                    entry.parent = Some(parent);
                }
                let parent_children = &mut self
                    .nodes
                    .get_mut(&parent)
                    .expect("parent must exist")
                    .children;
                if pos >= parent_children.len() {
                    parent_children.push(node);
                } else {
                    parent_children.insert(pos, node);
                }
            }
            InsertText { parent, node, text, pos } => {
                trace!("InsertText parent={:?} node={:?} text='{}' pos={}", parent, node, text.replace("\n", "\\n"), pos);
                self.ensure_parent_exists(parent);
                {
                    let entry = self
                        .nodes
                        .entry(node)
                        .or_insert_with(|| LayoutNode::new_text(text.clone(), Some(parent)));
                    entry.kind = LayoutNodeKind::InlineText { text };
                    entry.parent = Some(parent);
                }
                let parent_children = &mut self
                    .nodes
                    .get_mut(&parent)
                    .expect("parent must exist")
                    .children;
                if pos >= parent_children.len() {
                    parent_children.push(node);
                } else {
                    parent_children.insert(pos, node);
                }
            }
            SetAttr { node, name, value } => {
                trace!("SetAttr node={:?} {}='{}'", node, name, value);
                let entry = self.nodes.entry(node).or_insert_with(LayoutNode::new_document);
                entry.attrs.insert(name, value);
            }
            RemoveNode { node } => {
                trace!("RemoveNode node={:?}", node);
                self.remove_node_recursive(node);
            }
            EndOfDocument => {
                debug!("EndOfDocument received by layouter");
            }
        }
        Ok(())
    }

    /// Apply a single DOM update to the layout tree mirror (public API).
    pub fn apply_update(&mut self, update: DOMUpdate) -> Result<(), Error> {
        self.apply_update_impl(update)
    }

    /// Apply a batch of updates.
    pub fn apply_updates<I: IntoIterator<Item = DOMUpdate>>(&mut self, updates: I) -> Result<(), Error> {
        for u in updates { self.apply_update(u)?; }
        Ok(())
    }

    /// Update the active stylesheet used for layout/style resolution (placeholder for now).
    pub fn set_stylesheet(&mut self, stylesheet: Stylesheet) {
        self.stylesheet = stylesheet;
    }

    pub fn stylesheet(&self) -> &Stylesheet { &self.stylesheet }

    /// Compute layout using the dedicated layout module.
    pub fn compute_layout(&self) -> usize {
        layout::compute_layout(self)
    }

    /// Compute per-node layout geometry (x, y, width, height) for the current tree.
    pub fn compute_layout_geometry(&self) -> HashMap<NodeKey, LayoutRect> {
        layout::compute_layout_geometry(self)
    }

    /// Get a snapshot of the current layout tree for debugging/inspection.
    pub fn snapshot(&self) -> Vec<(NodeKey, LayoutNodeKind, Vec<NodeKey>)> {
        let mut v: Vec<_> = self
            .nodes
            .iter()
            .map(|(k, n)| (k.clone(), n.kind.clone(), n.children.clone()))
            .collect();
        v.sort_by_key(|(k, _, _)| k.0);
        v
    }

    fn ensure_parent_exists(&mut self, parent: NodeKey) {
        if !self.nodes.contains_key(&parent) {
            warn!("Parent {:?} missing in layouter; creating placeholder Document child", parent);
            self.nodes.insert(parent, LayoutNode::new_document());
        }
    }

    fn remove_node_recursive(&mut self, node: NodeKey) {
        if let Some(n) = self.nodes.remove(&node) {
            // detach from parent
            if let Some(p) = n.parent {
                if let Some(parent_node) = self.nodes.get_mut(&p) {
                    parent_node.children.retain(|c| *c != node);
                }
            }
            // remove children
            for c in n.children {
                self.remove_node_recursive(c);
            }
        }
    }
}

impl DOMSubscriber for Layouter {
    fn apply_update(&mut self, update: DOMUpdate) -> Result<(), Error> {
        self.apply_update_impl(update)
    }
}

pub type LayouterMirror = DOMMirror<Layouter>;