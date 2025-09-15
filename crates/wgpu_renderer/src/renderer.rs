use anyhow::Error;
use js::{DOMSubscriber, DOMUpdate, NodeKey};
use std::collections::HashMap;

// Keep tuple-heavy types readable and satisfy clippy's type_complexity.
pub type SnapshotEntry = (NodeKey, RenderNodeKind, Vec<NodeKey>);

/// A simple rectangle draw command in device-independent pixel space.
/// Colors are linear RGB [0..1]. This is a temporary bridge until a full display list exists.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: [f32; 3],
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

/// A simple text draw command in device-independent pixel space.
/// Text is rendered using a built-in 5x7 bitmap font expanded into colored quads.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawText {
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub color: [f32; 3],
    pub font_size: f32,
}

/// RenderNodeKind represents the minimal kinds of nodes a renderer cares about
/// when mirroring the DOM: a document root, elements (by tag), and text nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderNodeKind {
    Document,
    Element { tag: String },
    Text { text: String },
}

/// RenderNode is a simple scene-graph node that mirrors the DOM structure.
/// Attributes are preserved to enable later style-to-render mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderNode {
    pub kind: RenderNodeKind,
    pub attributes: HashMap<String, String>,
    pub parent: Option<NodeKey>,
    pub children: Vec<NodeKey>,
}

impl RenderNode {
    /// Create a new document root render node.
    fn new_document() -> Self {
        Self {
            kind: RenderNodeKind::Document,
            attributes: HashMap::new(),
            parent: None,
            children: Vec::new(),
        }
    }

    /// Create a new element render node for the given tag under the optional parent.
    fn new_element(tag: String, parent: Option<NodeKey>) -> Self {
        Self {
            kind: RenderNodeKind::Element { tag },
            attributes: HashMap::new(),
            parent,
            children: Vec::new(),
        }
    }

    /// Create a new text render node with the given text content under the optional parent.
    fn new_text(text: String, parent: Option<NodeKey>) -> Self {
        Self {
            kind: RenderNodeKind::Text { text },
            attributes: HashMap::new(),
            parent,
            children: Vec::new(),
        }
    }
}

/// Renderer mirrors DOM updates into a lightweight scene graph suitable for
/// translation into GPU-ready draw lists. It does not perform GPU work itself;
/// RenderState or another backend can consume its snapshots.
pub struct Renderer {
    nodes: HashMap<NodeKey, RenderNode>,
    root: NodeKey,
    /// Dirty rectangles provided by the layouter for partial redraw.
    dirty_rects: Vec<DrawRect>,
}

impl Renderer {
    /// Create a new, empty renderer with a seeded root document node.
    pub fn new() -> Self {
        let mut nodes = HashMap::new();
        nodes.insert(NodeKey::ROOT, RenderNode::new_document());
        Self {
            nodes,
            root: NodeKey::ROOT,
            dirty_rects: Vec::new(),
        }
    }

    /// Returns the root node key of the scene graph.
    pub fn root(&self) -> NodeKey {
        self.root
    }

    /// Apply a single DOMUpdate to the renderer's scene graph.
    fn apply_update_impl(&mut self, update: DOMUpdate) -> Result<(), Error> {
        use DOMUpdate::*;
        match update {
            InsertElement {
                parent,
                node,
                tag,
                pos,
            } => {
                self.ensure_parent_exists(parent);
                {
                    let entry = self
                        .nodes
                        .entry(node)
                        .or_insert_with(|| RenderNode::new_element(tag.clone(), Some(parent)));
                    entry.kind = RenderNodeKind::Element { tag };
                    entry.parent = Some(parent);
                }
                self.insert_child_at(parent, node, pos);
            }
            InsertText {
                parent,
                node,
                text,
                pos,
            } => {
                self.ensure_parent_exists(parent);
                {
                    let entry = self
                        .nodes
                        .entry(node)
                        .or_insert_with(|| RenderNode::new_text(text.clone(), Some(parent)));
                    entry.kind = RenderNodeKind::Text { text };
                    entry.parent = Some(parent);
                }
                self.insert_child_at(parent, node, pos);
            }
            SetAttr { node, name, value } => {
                let entry = self
                    .nodes
                    .entry(node)
                    .or_insert_with(RenderNode::new_document);
                entry.attributes.insert(name, value);
            }
            RemoveNode { node } => {
                self.remove_node_recursive(node);
            }
            EndOfDocument => {
                // No-op for now; a backend could trigger finalize hooks here.
            }
        }
        Ok(())
    }

    /// Insert a child under a parent at the given position, appending if pos is beyond the end.
    fn insert_child_at(&mut self, parent: NodeKey, child: NodeKey, position: usize) {
        if let Some(parent_node) = self.nodes.get_mut(&parent) {
            let children = &mut parent_node.children;
            if position >= children.len() {
                children.push(child);
            } else {
                children.insert(position, child);
            }
        }
    }

    /// Ensure a parent node exists in the map; if absent, seed as a document node.
    fn ensure_parent_exists(&mut self, parent: NodeKey) {
        self.nodes
            .entry(parent)
            .or_insert_with(RenderNode::new_document);
    }

    /// Recursively remove a node and all of its descendants from the scene graph,
    /// and detach it from its parent if present.
    fn remove_node_recursive(&mut self, node: NodeKey) {
        if let Some(node_entry) = self.nodes.remove(&node) {
            if let Some(parent_key) = node_entry.parent
                && let Some(parent_node) = self.nodes.get_mut(&parent_key)
            {
                parent_node.children.retain(|c| *c != node);
            }
            node_entry
                .children
                .into_iter()
                .for_each(|child| self.remove_node_recursive(child));
        }
    }

    /// Returns a stable snapshot of the scene graph as tuples of (key, kind, children).
    pub fn snapshot(&self) -> Vec<SnapshotEntry> {
        let mut out: Vec<SnapshotEntry> = self
            .nodes
            .iter()
            .map(|(key, node)| (*key, node.kind.clone(), node.children.clone()))
            .collect();
        out.sort_by_key(|(k, _, _)| k.0);
        out
    }

    /// Replace the current set of dirty rectangles to be used for partial redraws.
    pub fn set_dirty_rects(&mut self, rects: Vec<DrawRect>) {
        self.dirty_rects = rects;
    }

    /// Drain and return the current dirty rectangles (for testing/integration).
    pub fn take_dirty_rects(&mut self) -> Vec<DrawRect> {
        let mut out = Vec::new();
        std::mem::swap(&mut out, &mut self.dirty_rects);
        out
    }
}

impl DOMSubscriber for Renderer {
    /// Apply a DOMUpdate dispatched by the DOM runtime to keep the render scene in sync.
    fn apply_update(&mut self, update: DOMUpdate) -> Result<(), Error> {
        self.apply_update_impl(update)
    }
}
