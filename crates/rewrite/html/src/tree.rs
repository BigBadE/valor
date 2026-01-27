//! DOM tree structure and node data.

use html5ever::tree_builder::{NodeOrText, QuirksMode, TreeSink};
use html5ever::{Attribute, ExpandedName, QualName};
use rewrite_core::NodeId;
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::mpsc;
use tendril::StrTendril;

/// Data stored for each DOM node.
#[derive(Debug, Clone)]
pub enum NodeData {
    Document,
    Element(ElementData),
    Text(String),
    Comment(String),
}

/// Data for an element node.
#[derive(Debug, Clone)]
pub struct ElementData {
    pub tag_name: String,
    pub attributes: HashMap<String, String>,
}

impl ElementData {
    pub fn new(tag_name: String) -> Self {
        Self {
            tag_name,
            attributes: HashMap::new(),
        }
    }

    pub fn set_attribute(&mut self, name: String, value: String) {
        self.attributes.insert(name, value);
    }
}

/// DOM tree that stores node relationships and data separately from the layout Database.
pub struct DomTree {
    next_id: u64,
    node_data: HashMap<NodeId, NodeData>,
    parents: HashMap<NodeId, NodeId>,
    children: HashMap<NodeId, Vec<NodeId>>,
}

impl DomTree {
    /// Create a new empty DOM tree.
    pub fn new() -> Self {
        Self {
            next_id: 0,
            node_data: HashMap::new(),
            parents: HashMap::new(),
            children: HashMap::new(),
        }
    }

    /// Create a new node with unique ID.
    pub fn create_node(&mut self) -> NodeId {
        let id = NodeId::from_raw(self.next_id);
        self.next_id += 1;
        id
    }

    /// Set data for a node.
    pub fn set_node_data(&mut self, node: NodeId, data: NodeData) {
        self.node_data.insert(node, data);
    }

    /// Get data for a node.
    pub fn get_node_data(&self, node: NodeId) -> Option<&NodeData> {
        self.node_data.get(&node)
    }

    /// Get mutable data for a node.
    pub fn get_node_data_mut(&mut self, node: NodeId) -> Option<&mut NodeData> {
        self.node_data.get_mut(&node)
    }

    /// Establish parent-child relationship.
    pub fn append_child(&mut self, parent: NodeId, child: NodeId) {
        self.parents.insert(child, parent);
        self.children
            .entry(parent)
            .or_insert_with(Vec::new)
            .push(child);
    }

    /// Get parent of a node.
    pub fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.parents.get(&node).copied()
    }

    /// Get children of a node.
    pub fn children(&self, node: NodeId) -> &[NodeId] {
        self.children
            .get(&node)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}

impl Default for DomTree {
    fn default() -> Self {
        Self::new()
    }
}

/// Update sent during HTML parsing to build the DOM tree.
#[derive(Debug, Clone)]
pub enum DomUpdate {
    CreateNode { id: NodeId, data: NodeData },
    AppendChild { parent: NodeId, child: NodeId },
}

/// Builder for streaming DOM construction - implements TreeSink and sends updates via channel.
pub struct TreeBuilder {
    next_id: Cell<u64>,
    document: NodeId,
    tx: mpsc::Sender<DomUpdate>,
    // Static atoms for elem_name
    empty_ns: &'static html5ever::Namespace,
    empty_local: &'static html5ever::LocalName,
}

impl TreeBuilder {
    /// Create a new tree builder that streams updates to the given channel.
    pub fn new(tx: mpsc::Sender<DomUpdate>) -> Self {
        // Create static atoms
        use html5ever::{local_name, namespace_url};
        static EMPTY_NS: html5ever::Namespace = namespace_url!("");
        static EMPTY_LOCAL: html5ever::LocalName = local_name!("");

        let next_id = Cell::new(0);
        let document = NodeId::from_raw(0);
        next_id.set(1);

        // Send document creation
        let _ = tx.send(DomUpdate::CreateNode {
            id: document,
            data: NodeData::Document,
        });

        Self {
            next_id,
            document,
            tx,
            empty_ns: &EMPTY_NS,
            empty_local: &EMPTY_LOCAL,
        }
    }

    /// Create a new node ID and increment counter.
    fn create_node(&self) -> NodeId {
        let id = NodeId::from_raw(self.next_id.get());
        self.next_id.set(self.next_id.get() + 1);
        id
    }

    /// Send a DOM update.
    fn send_update(&self, update: DomUpdate) {
        let _ = self.tx.send(update);
    }

    /// Get the document node ID.
    pub fn document(&self) -> NodeId {
        self.document
    }
}

// Implement TreeSink for true streaming parsing
impl TreeSink for TreeBuilder {
    type Handle = NodeId;
    type Output = ();
    type ElemName<'a> = ExpandedName<'a>;

    fn finish(self) -> Self::Output {
        // Nothing to return - all updates were streamed
    }

    fn parse_error(&self, _msg: std::borrow::Cow<'static, str>) {
        // Ignore parse errors for now
    }

    fn get_document(&self) -> Self::Handle {
        self.document
    }

    fn elem_name<'a>(&'a self, _target: &'a Self::Handle) -> ExpandedName<'a> {
        // Return empty expanded name - not used in our implementation
        ExpandedName {
            ns: self.empty_ns,
            local: self.empty_local,
        }
    }

    fn create_element(
        &self,
        name: QualName,
        attrs: Vec<Attribute>,
        _flags: html5ever::tree_builder::ElementFlags,
    ) -> Self::Handle {
        let node = self.create_node();
        let mut elem_data = ElementData::new(name.local.to_string());

        for attr in attrs {
            elem_data.set_attribute(attr.name.local.to_string(), attr.value.to_string());
        }

        self.send_update(DomUpdate::CreateNode {
            id: node,
            data: NodeData::Element(elem_data),
        });

        node
    }

    fn create_comment(&self, text: StrTendril) -> Self::Handle {
        let node = self.create_node();
        self.send_update(DomUpdate::CreateNode {
            id: node,
            data: NodeData::Comment(text.to_string()),
        });
        node
    }

    fn create_pi(&self, _target: StrTendril, _data: StrTendril) -> Self::Handle {
        // Processing instructions - create as comment
        let node = self.create_node();
        self.send_update(DomUpdate::CreateNode {
            id: node,
            data: NodeData::Comment(String::new()),
        });
        node
    }

    fn append(&self, parent: &Self::Handle, child: NodeOrText<Self::Handle>) {
        match child {
            NodeOrText::AppendNode(node) => {
                self.send_update(DomUpdate::AppendChild {
                    parent: *parent,
                    child: node,
                });
            }
            NodeOrText::AppendText(text) => {
                // Create text node and append
                let text_node = self.create_node();
                self.send_update(DomUpdate::CreateNode {
                    id: text_node,
                    data: NodeData::Text(text.to_string()),
                });
                self.send_update(DomUpdate::AppendChild {
                    parent: *parent,
                    child: text_node,
                });
            }
        }
    }

    fn append_based_on_parent_node(
        &self,
        element: &Self::Handle,
        _prev_element: &Self::Handle,
        child: NodeOrText<Self::Handle>,
    ) {
        self.append(element, child);
    }

    fn append_doctype_to_document(
        &self,
        _name: StrTendril,
        _public_id: StrTendril,
        _system_id: StrTendril,
    ) {
        // Ignore doctype
    }

    fn get_template_contents(&self, target: &Self::Handle) -> Self::Handle {
        *target
    }

    fn same_node(&self, x: &Self::Handle, y: &Self::Handle) -> bool {
        x == y
    }

    fn set_quirks_mode(&self, _mode: QuirksMode) {
        // Ignore quirks mode
    }

    fn append_before_sibling(&self, _sibling: &Self::Handle, _new_node: NodeOrText<Self::Handle>) {
        // Not implemented - would need sibling tracking
    }

    fn add_attrs_if_missing(&self, _target: &Self::Handle, attrs: Vec<Attribute>) {
        // Would need to send an update to modify existing node attributes
        // For now, ignore this - it's only used for quirks mode
        let _ = attrs;
    }

    fn remove_from_parent(&self, _target: &Self::Handle) {
        // Would need a RemoveChild update type
    }

    fn reparent_children(&self, _node: &Self::Handle, _new_parent: &Self::Handle) {
        // Would need a ReparentChild update type
    }
}
