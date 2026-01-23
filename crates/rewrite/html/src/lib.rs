//! HTML parsing and DOM tree building for the rewrite system.

mod parser;
mod tree;

pub use parser::{HtmlParser, parse_html};
pub use tree::{ElementData, NodeData, TreeBuilder};

use rewrite_core::{Database, DependencyContext, NodeDataExt, NodeId, Query, Relationship};

/// DOM node types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    Element,
    Text,
    Comment,
    Document,
}

/// DOM properties that can be queried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, rewrite_macros::Query)]
#[value_type(NodeType)]
pub enum DomProperty {
    #[query(get_node_type)]
    NodeType,
}

/// Get the node type for a node.
fn get_node_type(db: &Database, node: NodeId, _ctx: &mut DependencyContext) -> NodeType {
    db.get_node_data::<NodeData>(node)
        .map(|data| match data {
            NodeData::Document => NodeType::Document,
            NodeData::Element(_) => NodeType::Element,
            NodeData::Text(_) => NodeType::Text,
            NodeData::Comment(_) => NodeType::Comment,
        })
        .unwrap_or(NodeType::Document)
}

/// Query for element tag name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TagNameQuery;

impl Query for TagNameQuery {
    type Key = NodeId;
    type Value = Option<String>;

    fn execute(db: &Database, key: Self::Key, _ctx: &mut DependencyContext) -> Self::Value {
        db.get_node_data::<NodeData>(key).and_then(|data| {
            if let NodeData::Element(elem) = data {
                Some(elem.tag_name.clone())
            } else {
                None
            }
        })
    }
}

/// Query for element attribute value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AttributeQuery;

impl Query for AttributeQuery {
    type Key = (NodeId, String); // (node, attribute_name)
    type Value = Option<String>;

    fn execute(db: &Database, key: Self::Key, _ctx: &mut DependencyContext) -> Self::Value {
        let (node, attr_name) = key;
        db.get_node_data::<NodeData>(node).and_then(|data| {
            if let NodeData::Element(elem) = data {
                elem.attributes.get(&attr_name).cloned()
            } else {
                None
            }
        })
    }
}

/// Query for text content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextContentQuery;

impl Query for TextContentQuery {
    type Key = NodeId;
    type Value = Option<String>;

    fn execute(db: &Database, key: Self::Key, _ctx: &mut DependencyContext) -> Self::Value {
        db.get_node_data::<NodeData>(key).and_then(|data| {
            if let NodeData::Text(text) = data {
                Some(text.clone())
            } else {
                None
            }
        })
    }
}

/// Query for all children of a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChildrenQuery;

impl Query for ChildrenQuery {
    type Key = NodeId;
    type Value = Vec<NodeId>;

    fn execute(db: &Database, key: Self::Key, _ctx: &mut DependencyContext) -> Self::Value {
        db.resolve_relationship(key, Relationship::Children)
    }
}
