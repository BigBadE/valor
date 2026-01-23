/// Unique identifier for a DOM node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(u64);

impl NodeId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }
}

pub mod database;
pub mod dependency;
pub mod input;
pub mod node_data;
pub mod pattern_cache;
pub mod property_state;
pub mod query;
pub mod relationship;
pub mod scoped_db;

pub use database::{Database, DatabaseStats};
pub use dependency::{Dependency, DependencyContext, DependencyPattern};
pub use input::{Input, InputQuery};
pub use node_data::{NodeDataExt, NodeDataInput};
pub use pattern_cache::PatternCache;
pub use property_state::{PropertyKey, PropertyState};
pub use query::Query;
pub use relationship::Relationship;
pub use scoped_db::ScopedDb;
