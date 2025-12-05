//! CSS Core Layout Module
//!
//! This module provides a constraint-based layout system modeled after Chromium's `LayoutNG`.
//! Layout is computed by passing constraint spaces down the tree and returning layout
//! fragments back up.
//!
//! ## Architecture
//!
//! - `box_tree::ConstraintLayoutTree` - Main layout tree structure
//! - `box_tree::ConstraintSpace` - Constraints passed to layout (available size, BFC offset, etc.)
//! - `box_tree::ExclusionSpace` - Float exclusions and clearance tracking
//! - `box_tree::MarginStrut` - Margin collapsing state
//! - `box_tree::layout_tree()` - Entry point for constraint-based layout
//!
//! Spec reference: <https://www.w3.org/TR/CSS22>

// Constraint-based layout architecture (Chromium LayoutNG-like)
pub mod box_tree;

// Re-export primary constraint-based API
pub use box_tree::layout_tree;
pub use box_tree::{AvailableSize, BfcOffset, ConstraintLayoutTree, ConstraintSpace};
pub use box_tree::{ExclusionSpace, FloatExclusion, FloatSize, MarginStrut};
pub use box_tree::{LayoutRect, LayoutResult, LayoutUnit};
