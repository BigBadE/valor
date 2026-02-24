//! Trait for DOM tree access from the database layer.
//!
//! The database lives in `rewrite_core` which cannot depend on
//! `rewrite_html` (circular dependency). This trait abstracts the
//! tree operations the database needs so the concrete `DomTree`
//! implementation can be provided from a higher layer.

use crate::NodeId;

/// Read-only access to DOM tree parent relationships.
///
/// Implemented by `DomTree` in the html crate and passed into
/// `Database::new()` so sparse trees can find ancestors when
/// inserting nodes.
pub trait TreeAccess: Send + Sync {
    /// Get the parent of a node, if it has one.
    fn parent(&self, node: NodeId) -> Option<NodeId>;

    /// Get all direct children of a node.
    fn children(&self, node: NodeId) -> Vec<NodeId>;
}
