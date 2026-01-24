//! HTML parsing and DOM tree building for the rewrite system.

mod parser;
mod tree;

pub use parser::{HtmlParser, parse_html};
pub use tree::{ElementData, NodeData, TreeBuilder};

use rewrite_core::{NodeId, Relationship, ScopedDb};

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
fn get_node_type(scoped: &mut ScopedDb) -> NodeType {
    scoped
        .get_node_data::<NodeData>()
        .map(|data| match data {
            NodeData::Document => NodeType::Document,
            NodeData::Element(_) => NodeType::Element,
            NodeData::Text(_) => NodeType::Text,
            NodeData::Comment(_) => NodeType::Comment,
        })
        .unwrap_or(NodeType::Document)
}

/// Query for element tag name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, rewrite_macros::Query)]
#[query(get_tag_name)]
#[value_type(Option<String>)]
pub struct TagNameQuery;

fn get_tag_name(scoped: &mut ScopedDb) -> Option<String> {
    scoped.get_node_data::<NodeData>().and_then(|data| {
        if let NodeData::Element(elem) = data {
            Some(elem.tag_name.clone())
        } else {
            None
        }
    })
}

/// Query for element attribute value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AttributeQuery;

impl rewrite_core::Query for AttributeQuery {
    type Key = (NodeId, String);
    type Value = Option<String>;

    fn execute(
        db: &rewrite_core::Database,
        key: Self::Key,
        ctx: &mut rewrite_core::DependencyContext,
    ) -> Self::Value {
        let (node, attr_name) = key;
        let scoped = ScopedDb::new(db, node, ctx);
        scoped.get_node_data::<NodeData>().and_then(|data| {
            if let NodeData::Element(elem) = data {
                elem.attributes.get(&attr_name).cloned()
            } else {
                None
            }
        })
    }
}

/// Query for text content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, rewrite_macros::Query)]
#[query(get_text_content)]
#[value_type(Option<String>)]
pub struct TextContentQuery;

fn get_text_content(scoped: &mut ScopedDb) -> Option<String> {
    scoped.get_node_data::<NodeData>().and_then(|data| {
        if let NodeData::Text(text) = data {
            Some(text.clone())
        } else {
            None
        }
    })
}

/// Query for all children of a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, rewrite_macros::Query)]
#[query(get_children)]
#[value_type(Vec<NodeId>)]
pub struct ChildrenQuery;

fn get_children(scoped: &mut ScopedDb) -> Vec<NodeId> {
    scoped
        .db()
        .resolve_relationship(scoped.node(), Relationship::Children)
}
