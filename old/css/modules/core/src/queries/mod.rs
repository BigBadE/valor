//! Layout queries for demand-driven, memoized layout computation.
//!
//! This module provides a query-based interface for layout computation
//! that enables:
//! - Automatic memoization of layout results
//! - Dependency tracking for incremental updates
//! - Parallel execution of independent formatting contexts
//!
//! Query architecture:
//! - Input queries: DOM structure, computed styles, viewport
//! - Derived queries: formatting context detection, constraint computation, layout results

pub mod formatting_context;
pub mod layout_queries;

pub use formatting_context::{FormattingContextQuery, FormattingContextType};
pub use layout_queries::LayoutResultQuery;
