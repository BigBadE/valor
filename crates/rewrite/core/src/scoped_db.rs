use crate::{Database, DependencyContext, NodeId, Query, Relationship};

/// Database scoped to a specific node for convenience.
pub struct ScopedDb<'a> {
    db: &'a Database,
    node: NodeId,
    ctx: &'a mut DependencyContext,
}

impl<'a> ScopedDb<'a> {
    pub fn new(db: &'a Database, node: NodeId, ctx: &'a mut DependencyContext) -> Self {
        Self { db, node, ctx }
    }

    pub fn node(&self) -> NodeId {
        self.node
    }

    pub fn db(&self) -> &Database {
        self.db
    }

    pub fn ctx_mut(&mut self) -> &mut DependencyContext {
        self.ctx
    }

    /// Query a property on this node.
    pub fn query<Q: Query<Key = NodeId>>(&mut self) -> Q::Value
    where
        Q::Value: Clone + Send + Sync,
    {
        self.db.query::<Q>(self.node, self.ctx)
    }

    /// Query a property on the parent node.
    /// If there's no parent (root node), queries the viewport root instead.
    pub fn parent<Q: Query<Key = NodeId>>(&mut self) -> Q::Value
    where
        Q::Value: Clone + Send + Sync,
    {
        let parent_ids = self
            .db
            .resolve_relationship(self.node, Relationship::Parent);
        let parent = parent_ids.first().copied().unwrap_or_else(|| {
            // No parent - use viewport root (NodeId 0)
            NodeId::new(0)
        });
        self.db.query::<Q>(parent, self.ctx)
    }

    /// Query a property on all children nodes, returning an iterator.
    pub fn children<Q: Query<Key = NodeId>>(&mut self) -> impl Iterator<Item = Q::Value> + '_
    where
        Q::Value: Clone + Send + Sync,
    {
        let children = self
            .db
            .resolve_relationship(self.node, Relationship::Children);
        children
            .into_iter()
            .map(|child| self.db.query::<Q>(child, self.ctx))
    }

    /// Count the number of children nodes.
    pub fn children_count(&self) -> usize {
        self.db
            .resolve_relationship(self.node, Relationship::Children)
            .len()
    }

    /// Query a property on all previous sibling nodes, returning an iterator.
    pub fn prev_siblings<Q: Query<Key = NodeId>>(&mut self) -> impl Iterator<Item = Q::Value> + '_
    where
        Q::Value: Clone + Send + Sync,
    {
        let siblings = self
            .db
            .resolve_relationship(self.node, Relationship::PreviousSiblings);
        siblings
            .into_iter()
            .map(|sibling| self.db.query::<Q>(sibling, self.ctx))
    }

    /// Count the number of previous sibling nodes.
    pub fn prev_siblings_count(&self) -> usize {
        self.db
            .resolve_relationship(self.node, Relationship::PreviousSiblings)
            .len()
    }

    /// Get the number of children of the parent node.
    /// This is useful for calculations that need to know sibling count from a child's perspective.
    pub fn parent_children_count(&self) -> usize {
        let parent_ids = self
            .db
            .resolve_relationship(self.node, Relationship::Parent);
        let parent = parent_ids
            .first()
            .copied()
            .unwrap_or_else(|| NodeId::new(0));
        self.db
            .resolve_relationship(parent, Relationship::Children)
            .len()
    }

    /// Get the parent node ID, if it exists.
    pub fn parent_id(&self) -> Option<NodeId> {
        self.db
            .resolve_relationship(self.node, Relationship::Parent)
            .first()
            .copied()
    }

    /// Get the first child node ID, if it exists.
    pub fn first_child(&self) -> Option<NodeId> {
        self.db
            .resolve_relationship(self.node, Relationship::Children)
            .first()
            .copied()
    }

    /// Get the last child node ID, if it exists.
    pub fn last_child(&self) -> Option<NodeId> {
        self.db
            .resolve_relationship(self.node, Relationship::Children)
            .last()
            .copied()
    }

    /// Get the previous sibling node ID, if it exists.
    pub fn prev_sibling(&self) -> Option<NodeId> {
        self.db
            .resolve_relationship(self.node, Relationship::PreviousSiblings)
            .last()
            .copied()
    }

    /// Query a property on a specific node (not just self or parent).
    pub fn node_query<Q: Query<Key = NodeId>>(&mut self, node: NodeId) -> Q::Value
    where
        Q::Value: Clone + Send + Sync,
    {
        self.db.query::<Q>(node, self.ctx)
    }

    /// Get the parent of a specific node.
    pub fn node_parent(&self, node: NodeId) -> Option<NodeId> {
        self.db
            .resolve_relationship(node, Relationship::Parent)
            .first()
            .copied()
    }

    /// Create a new `ScopedDb` for a different node, reusing the same database and context.
    ///
    /// This is useful when you need to query properties on child/sibling/parent nodes
    /// within the same query execution. The returned `ScopedDb` borrows from `self`,
    /// so it cannot outlive the current scope.
    pub fn scoped_to(&mut self, node: NodeId) -> ScopedDb<'_> {
        ScopedDb::new(self.db, node, self.ctx)
    }

    /// Set arbitrary data on the scoped node.
    pub fn set_node_data<T: Clone + Send + Sync + 'static>(&self, data: T) {
        self.db
            .set_input::<crate::NodeDataInput<T>>(self.node, data);
    }

    /// Get arbitrary data from the scoped node.
    pub fn get_node_data<T: Clone + Send + Sync + 'static>(&self) -> Option<T> {
        self.db.get_input::<crate::NodeDataInput<T>>(&self.node)
    }

    /// Get arbitrary data from a specific node.
    pub fn get_node_data_for<T: Clone + Send + Sync + 'static>(&self, node: NodeId) -> Option<T> {
        self.db.get_input::<crate::NodeDataInput<T>>(&node)
    }

    /// Query with a custom key (for queries that don't use NodeId as the key).
    pub fn query_with_key<Q: Query>(&mut self, key: Q::Key) -> Q::Value
    where
        Q::Value: Clone + Send + Sync,
    {
        self.db.query::<Q>(key, self.ctx)
    }
}
