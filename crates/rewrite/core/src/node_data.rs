//! Node data storage using the Input system.

use crate::{Input, NodeId};

/// Input for storing arbitrary data on nodes.
pub struct NodeDataInput<T: Clone + Send + Sync + 'static> {
    _phantom: std::marker::PhantomData<T>,
}

impl<T: Clone + Send + Sync + 'static> Input for NodeDataInput<T> {
    type Key = NodeId;
    type Value = T;

    fn name() -> &'static str {
        std::any::type_name::<T>()
    }

    fn default_value(_key: &Self::Key) -> Self::Value {
        panic!("No default value for node data")
    }
}

/// Extension trait for Database to work with node data.
pub trait NodeDataExt {
    /// Set arbitrary data on a node.
    fn set_node_data<T: Clone + Send + Sync + 'static>(&self, node: NodeId, data: T);

    /// Get arbitrary data from a node.
    fn get_node_data<T: Clone + Send + Sync + 'static>(&self, node: NodeId) -> Option<T>;
}

impl NodeDataExt for crate::Database {
    fn set_node_data<T: Clone + Send + Sync + 'static>(&self, node: NodeId, data: T) {
        self.set_input::<NodeDataInput<T>>(node, data);
    }

    fn get_node_data<T: Clone + Send + Sync + 'static>(&self, node: NodeId) -> Option<T> {
        self.get_input::<NodeDataInput<T>>(&node)
    }
}
