//! Storage backends for query results.
//!
//! This module provides the storage infrastructure for:
//! - Memoized query results with dependency tracking
//! - Input values from external sources (DOM, etc.)

mod input;
mod memoized;

pub use input::InputStorage;
pub use memoized::{MemoizedStorage, QueryStorage};
