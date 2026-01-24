//! DOM tree structure and node data.

use rewrite_core::{Database, NodeDataExt, NodeId, Relationship};
use std::collections::HashMap;

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

/// Builder for constructing a DOM tree in the Database.
pub struct TreeBuilder {
    db: Database,
    root: NodeId,
}

impl TreeBuilder {
    /// Create a new tree builder with a document root.
    pub fn new(db: Database) -> Self {
        let root = db.create_node();
        db.set_node_data(root, NodeData::Document);
        Self { db, root }
    }

    /// Get the root node ID.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Get a reference to the database.
    pub fn database(&self) -> &Database {
        &self.db
    }

    /// Consume the builder and return the database.
    pub fn into_database(self) -> Database {
        self.db
    }

    /// Create an element node.
    pub fn create_element(&self, tag_name: String) -> NodeId {
        let node = self.db.create_node();
        self.db
            .set_node_data(node, NodeData::Element(ElementData::new(tag_name)));
        node
    }

    /// Create a text node.
    pub fn create_text(&self, text: String) -> NodeId {
        let node = self.db.create_node();
        self.db.set_node_data(node, NodeData::Text(text));
        node
    }

    /// Create a comment node.
    pub fn create_comment(&self, text: String) -> NodeId {
        let node = self.db.create_node();
        self.db.set_node_data(node, NodeData::Comment(text));
        node
    }

    /// Append a child to a parent node.
    pub fn append_child(&self, parent: NodeId, child: NodeId) {
        // Get existing children to find the last sibling
        let existing_children = self.db.resolve_relationship(parent, Relationship::Children);
        let last_child = existing_children.last().copied();

        // Establish parent-child relationship
        self.db
            .establish_relationship(parent, Relationship::Children, child);
        self.db
            .establish_relationship(child, Relationship::Parent, parent);

        // Establish sibling relationships if there's a previous child
        if let Some(prev) = last_child {
            self.db
                .establish_relationship(prev, Relationship::NextSibling, child);
            self.db
                .establish_relationship(child, Relationship::PreviousSibling, prev);
        }
    }

    /// Set an attribute on an element.
    pub fn set_attribute(&self, node: NodeId, name: String, value: String) {
        if let Some(NodeData::Element(mut elem)) = self.db.get_node_data::<NodeData>(node) {
            elem.set_attribute(name, value);
            self.db.set_node_data(node, NodeData::Element(elem));
        }
    }

    /// Get element data for a node.
    pub fn get_element(&self, node: NodeId) -> Option<ElementData> {
        self.db.get_node_data::<NodeData>(node).and_then(|data| {
            if let NodeData::Element(elem) = data {
                Some(elem)
            } else {
                None
            }
        })
    }
}
