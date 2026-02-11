//! Common types for DOM representation.

use lasso::Spur;
use rewrite_core::NodeId;
use std::collections::HashMap;

/// Node data, varying by node type.
pub enum NodeData {
    Document,
    Element {
        tag: Spur,
        attributes: HashMap<Spur, Box<str>>,
    },
    Text(Box<str>),
    Comment(Box<str>),
}

/// DOM update events emitted by the parser.
pub enum DomUpdate {
    /// Create a node. Callback must return the assigned NodeId.
    CreateNode(NodeData),
    /// Append child to parent.
    AppendChild { parent: NodeId, child: NodeId },
}
