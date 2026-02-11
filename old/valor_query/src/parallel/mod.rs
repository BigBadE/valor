//! Parallel query execution infrastructure.
//!
//! This module provides support for executing independent queries in parallel,
//! with special handling for formatting context boundaries in layout.

mod scheduler;

pub use scheduler::{ParallelRuntime, WorkUnit};
