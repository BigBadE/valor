//! Constraint-based layout system for CSS, modeled after Chromium's `LayoutNG`.
//!
//! Spec: CSS 2.2 §9 Visual Formatting Model
//!   - <https://www.w3.org/TR/CSS22/visuren.html>
//!
//! This module implements a constraint-based layout algorithm similar to Chromium's approach,
//! where layout is computed by passing constraint spaces down the tree and returning
//! layout fragments back up.

// Re-export LayoutUnit from css_box
pub use css_box::LayoutUnit;

// Constraint space types (used by query-based layout)
pub mod constraint_space;
pub mod exclusion_space;
pub mod grid_template_parser;
pub mod margin_strut;

// OLD SYSTEM - DELETED: The old ConstraintLayoutTree-based layout has been replaced
// by the query-based incremental layout system in queries/layout_queries.rs
// pub mod constraint_block_layout;

// Export constraint space types (used by query-based layout)
pub use constraint_space::{AvailableSize, BfcOffset, ConstraintSpace, LayoutResult};
pub use exclusion_space::{ExclusionSpace, FloatExclusion, FloatSize};
pub use margin_strut::MarginStrut;

/// Simple rectangle type for layout results.
///
/// This is used for rendering/painting and represents the final
/// border-box position and size of an element.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LayoutRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl LayoutRect {
    // Note: LayoutRect is now created directly in incremental_layout.rs
    // The old from_layout_result method is removed to avoid type confusion
    // between constraint_space::LayoutResult and layout_queries::LayoutResult
}
