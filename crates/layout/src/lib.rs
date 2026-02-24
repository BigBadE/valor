//! Layout computation crate.
//!
//! This crate coordinates layout computation using the formula-based system.
//! Query modules (size, offset, block, flex, grid) are consolidated
//! under `queries/` to eliminate circular dependencies.

// Formula construction macros from rewrite_core
#[macro_use]
extern crate rewrite_core;

mod macros;

// Query modules — size/offset dispatch based on display mode
pub mod queries;

// Re-export query entry points
pub use queries::{offset_query, property_query, size_query};

// Core layout modules
mod layout_tree;
mod scroll;
mod writing_mode;

// Re-export layout tree types
pub use layout_tree::{BoxType, EdgeSizes, LayoutBox, LayoutTreeBuilder, Rect};

// Re-export writing mode functions
pub use writing_mode::{
    Direction, PhysicalAxis, PhysicalEdge, Subpixels, WritingMode, WritingModeContext,
    block_axis_to_physical, block_end_edge, block_progression_sign, block_start_edge,
    get_direction, get_physical_block_offset, get_writing_mode, inline_axis_to_physical,
    inline_end_edge, inline_progression_sign, inline_start_edge, is_horizontal_writing_mode,
    is_vertical_writing_mode, logical_to_physical_coords, logical_to_physical_size,
    resolve_logical_property,
};

// Re-export scroll integration
pub use scroll::{
    EasingFunction, ScrollAlignment, ScrollAnimation, ScrollBehavior, ScrollBounds, ScrollEvent,
    ScrollIntoViewRequest, ScrollPositionInput, ViewportScrollInput, apply_scroll_momentum,
    calculate_scroll_into_view, can_scroll, update_sticky_scroll_state,
};

/// Available size for layout constraints.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AvailableSize {
    /// Definite size in pixels.
    Definite(f32),
    /// Indefinite size (auto).
    Indefinite,
    /// Min-content sizing.
    MinContent,
    /// Max-content sizing.
    MaxContent,
}
