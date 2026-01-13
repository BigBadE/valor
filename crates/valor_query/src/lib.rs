//! Query-based incremental computation system for Valor.
//!
//! This crate provides a Salsa/Rustc-style query system with:
//! - Automatic memoization of query results
//! - Dependency tracking between queries
//! - Incremental recomputation (only invalidate what changed)
//! - Support for parallel query execution
//!
//! # Architecture
//!
//! The system is organized in layers:
//!
//! ```text
//! Layer 0: Input Queries (DOM data from external sources)
//!     ↓
//! Layer 1-3: Style Queries (selector matching, cascade, computed styles)
//!     ↓
//! Layer 4: Layout Queries (formatting contexts, layout results)
//!     ↓
//! Layer 5: Paint Queries (display lists, stacking contexts)
//! ```
//!
//! # Example
//!
//! ```ignore
//! use valor_query::{Query, InputQuery, QueryDatabase};
//!
//! // Define an input query
//! struct DomTagInput;
//! impl InputQuery for DomTagInput {
//!     type Key = NodeKey;
//!     type Value = String;
//!     fn default_value() -> String { String::new() }
//! }
//!
//! // Define a derived query
//! struct ComputedStyleQuery;
//! impl Query for ComputedStyleQuery {
//!     type Key = NodeKey;
//!     type Value = ComputedStyle;
//!
//!     fn execute(db: &QueryDatabase, key: NodeKey) -> ComputedStyle {
//!         // Dependencies are automatically tracked
//!         let tag = db.input::<DomTagInput>(key);
//!         let inherited = db.query::<InheritedStyleQuery>(key);
//!         compute_style(tag, inherited)
//!     }
//! }
//!
//! // Use the database
//! let db = QueryDatabase::new();
//! db.set_input::<DomTagInput>(node, "div".into());
//! let style = db.query::<ComputedStyleQuery>(node);
//! ```

#![allow(
    clippy::module_name_repetitions,
    reason = "Query types like InputQuery are clearer than just Input"
)]
#![allow(clippy::missing_errors_doc, reason = "Internal crate")]
#![allow(clippy::missing_panics_doc, reason = "Internal crate")]

mod database;
mod query;
mod revision;
mod storage;

pub mod parallel;

// Re-exports
pub use database::QueryDatabase;
pub use query::{InputQuery, MemoizedResult, ParallelQuery, Query, QueryExt, QueryKey};
pub use revision::{Revision, RevisionCounter};
