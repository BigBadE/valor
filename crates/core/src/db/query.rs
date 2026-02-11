//! Query types for formula selection.

use crate::{Database, NodeId, Subpixel};
use lightningcss::properties::{Property, PropertyId};

/// A query function that selects which formula to use based on a node's CSS properties.
/// The concrete formula type depends on the styler — use the CSS crate's `Formula` alias.
pub type Query = fn(&ScopedDb<'_>) -> Option<Subpixel>;

/// Scoped database access for a specific node.
///
/// Provides property lookups that automatically handle inheritance
/// (walking DOM ancestors for inherited property groups like Text).
pub struct ScopedDb<'db> {
    database: &'db Database,
    node: NodeId,
}

impl<'db> ScopedDb<'db> {
    /// Create a scoped database accessor for a specific node.
    pub fn new(database: &'db Database, node: NodeId) -> Self {
        Self { database, node }
    }

    /// Get the node this query is scoped to.
    pub fn node(&self) -> NodeId {
        self.node
    }

    /// Get a property for the current node.
    ///
    /// Inherited properties (Text group) automatically walk DOM ancestors.
    /// Non-inherited properties return only values explicitly set on this node.
    pub fn get_property(&self, prop_id: PropertyId<'static>) -> Option<Property<'static>> {
        self.database.get_property(self.node, prop_id)
    }
}
