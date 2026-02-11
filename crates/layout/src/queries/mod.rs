//! Layout query modules - compute size and offset formulas based on display mode.
//!
//! All layout query crates are consolidated here to eliminate circular
//! dependencies. Block can reference size, size can reference block, etc.

pub mod bfc;
pub mod block;
pub mod flex;
pub mod grid;
pub mod offset;
pub mod size;

pub use offset::{offset_query, offset_query_horizontal, offset_query_vertical};
pub use size::{size_query, size_query_horizontal, size_query_vertical};
