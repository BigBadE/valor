//! Paint tree traversal and display list generation.
//!
//! This module provides browser-grade paint order traversal following
//! CSS 2.2 Appendix E painting order specification.

mod builder;
mod stacking;
mod traversal;

pub use builder::DisplayListBuilder;
pub use stacking::{StackingContext, StackingLevel};
pub use traversal::{PaintOrder, traverse_paint_tree};
