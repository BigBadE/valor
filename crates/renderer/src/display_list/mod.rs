//! Retained display list (DL) primitives and utilities.
//!
//! This module provides:
//! - Core display list types and structures (`core`)
//! - Fine-grained diffing for incremental updates (`diffing`)
//! - Binary serialization for recording and replay (`serialization`)

pub mod core;
pub mod diffing;
pub mod serialization;

// Re-export core types for backwards compatibility
pub use core::{
    Batch, DisplayItem, DisplayList, DisplayListDiff, Quad, Scissor, StackingContextBoundary,
    TextBoundsPx, batch_display_list,
};
