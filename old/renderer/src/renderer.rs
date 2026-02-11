use crate::display_list::TextBoundsPx;
use anyhow::Error;
use core::mem;
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
    #[inline]
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
    /// Requested font weight from CSS (100-900, default 400 = normal, 700 = bold)
    pub font_weight: u16,
    /// Matched font weight after CSS font matching (e.g., requested 300 -> matched 400)
    pub matched_font_weight: u16,
    /// Font family (e.g., "Courier New", "monospace")
    pub font_family: Option<String>,
    /// Line height in pixels (for vertical metrics) - ROUNDED for layout
    pub line_height: f32,
    /// Unrounded line height in pixels - for rendering to match layout calculations
    pub line_height_unrounded: f32,
    /// Optional bounds for wrapping/clipping: (left, top, right, bottom) in framebuffer pixels.
    pub bounds: Option<TextBoundsPx>,
    /// Measured text width from layout (for wrapping during rendering)
    pub measured_width: f32,
}

/// `RenderNodeKind` represents the minimal kinds of nodes a renderer cares about
/// when mirroring the DOM: a document root, elements (by tag), and text nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderNodeKind {
    Document,
    Element { tag: String },
    Text { text: String },
}

/// `RenderNode` is a simple scene-graph node that mirrors the DOM structure.
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

/// Renderer mirrors DOM updates into a lightweight scene graph.
///
/// The scene graph is suitable for translation into GPU-ready draw lists. It does not
/// perform GPU work itself; `RenderState` or another backend can consume its snapshots.
pub struct Renderer {
    /// Map of node keys to render nodes.
    nodes: HashMap<NodeKey, RenderNode>,
    /// Root node key.
    root: NodeKey,
    /// Dirty rectangles provided by the layouter for partial redraw.
    dirty_rects: Vec<DrawRect>,
}

impl Renderer {
    /// Create a new, empty renderer with a seeded root document node.
    #[inline]
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
    #[inline]
    pub const fn root(&self) -> NodeKey {
        self.root
    }

    /// Apply a single `DOMUpdate` to the renderer's scene graph.
    /// Apply a DOM update to the renderer.
    ///
    /// # Errors
    /// Returns an error if the update fails.
    fn apply_update_impl(&mut self, update: DOMUpdate) {
        use DOMUpdate::{
            EndOfDocument, InsertElement, InsertText, RemoveNode, SetAttr, UpdateText,
        };
        match update {
            InsertElement {
                parent,
                node,
                tag,
                pos,
            } => {
                self.ensure_parent_exists(parent);
                let entry = self
                    .nodes
                    .entry(node)
                    .or_insert_with(|| RenderNode::new_element(tag.clone(), Some(parent)));
                entry.kind = RenderNodeKind::Element { tag };
                entry.parent = Some(parent);
                self.insert_child_at(parent, node, pos);
            }
            InsertText {
                parent,
                node,
                text,
                pos,
            } => {
                self.ensure_parent_exists(parent);
                let entry = self
                    .nodes
                    .entry(node)
                    .or_insert_with(|| RenderNode::new_text(text.clone(), Some(parent)));
                entry.parent = Some(parent);
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
            UpdateText { node, text } => {
                // Update the text content of an existing text node in-place
                if let Some(render_node) = self.nodes.get_mut(&node)
                    && let RenderNodeKind::Text { text: text_ref } = &mut render_node.kind
                {
                    text_ref.clone_from(&text);
                }
            }
            EndOfDocument => {
                // No-op for now; a backend could trigger finalize hooks here.
            }
        }
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
                parent_node.children.retain(|child| *child != node);
            }
            node_entry
                .children
                .into_iter()
                .for_each(|child| self.remove_node_recursive(child));
        }
    }

    /// Returns a stable snapshot of the scene graph as tuples of (key, kind, children).
    #[inline]
    pub fn snapshot(&self) -> Vec<SnapshotEntry> {
        let mut out: Vec<SnapshotEntry> = self
            .nodes
            .iter()
            .map(|(key, node)| (*key, node.kind.clone(), node.children.clone()))
            .collect();
        out.sort_by_key(|(key, _, _)| key.0);
        out
    }

    /// Replace the current set of dirty rectangles to be used for partial redraws.
    #[inline]
    pub fn set_dirty_rects(&mut self, rects: Vec<DrawRect>) {
        self.dirty_rects = rects;
    }

    /// Drain and return the current dirty rectangles (for testing/integration).
    #[inline]
    pub fn take_dirty_rects(&mut self) -> Vec<DrawRect> {
        mem::take(&mut self.dirty_rects)
    }
}

impl DOMSubscriber for Renderer {
    /// Apply a `DOMUpdate` dispatched by the DOM runtime to keep the render scene in sync.
    #[inline]
    fn apply_update(&mut self, update: DOMUpdate) -> Result<(), Error> {
        self.apply_update_impl(update);
        Ok(())
    }
}
