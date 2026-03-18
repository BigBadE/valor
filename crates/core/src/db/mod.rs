//! Database and query types.

pub mod property_group;
mod query;
pub mod sparse_tree;
mod storage;
pub mod tree_access;

pub use property_group::{PropertyGroup, classify as classify_property};
pub use query::{Query, ScopedDb};
pub use sparse_tree::SparseTree;
pub use storage::{Database, is_css_initial_value};
pub use tree_access::TreeAccess;
