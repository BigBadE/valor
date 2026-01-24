// Re-export MarginQuery from layout_margin
pub use rewrite_layout_margin::MarginQuery;

// Re-export util types and traits
pub use rewrite_layout_util::{Axis, Dispatcher};

// Re-export size and offset modes
pub use rewrite_layout_offset::OffsetMode;
pub use rewrite_layout_size::SizeMode;

// Re-export marker types
pub use rewrite_layout_offset_impl::OffsetModeMarker;
pub use rewrite_layout_size_impl::{ConstrainedMarker, IntrinsicMarker, SizeModeMarker};
pub use rewrite_layout_util::{BlockMarker, InlineMarker};

// Re-export query types
pub use rewrite_layout_offset::OffsetQuery;

// Instantiate SizeQuery with concrete FlexSize implementation
pub type SizeQuery<AxisParam, ModeParam> =
    rewrite_layout_size::SizeQueryGeneric<AxisParam, ModeParam, rewrite_layout_flex::FlexSize>;

// Type aliases for concrete queries (proc macro generates SizeQuery<Axis, Mode> and OffsetQuery<Axis, Mode>)
pub type InlineSizeQuery = SizeQuery<InlineMarker, ConstrainedMarker>;
pub type BlockSizeQuery = SizeQuery<BlockMarker, ConstrainedMarker>;
pub type IntrinsicInlineSizeQuery = SizeQuery<InlineMarker, IntrinsicMarker>;
pub type IntrinsicBlockSizeQuery = SizeQuery<BlockMarker, IntrinsicMarker>;

// Offset queries now require a mode parameter
pub type StaticInlineOffsetQuery =
    OffsetQuery<InlineMarker, rewrite_layout_offset_impl::StaticMarker>;
pub type StaticBlockOffsetQuery =
    OffsetQuery<BlockMarker, rewrite_layout_offset_impl::StaticMarker>;

// Re-export dimensional queries
pub use rewrite_css_dimensional::PaddingQuery;

pub use rewrite_css::Subpixels;

// Legacy type aliases for compatibility
pub type InlineOffsetQuery = StaticInlineOffsetQuery;
pub type BlockOffsetQuery = StaticBlockOffsetQuery;

// Module crates are available as dependencies but not re-exported here to avoid cycles.
// Downstream crates should import these directly:
// - rewrite_layout_offset for OffsetQuery
// - rewrite_layout_size for SizeQuery
// - rewrite_layout_flex for flex functions
// - rewrite_layout_grid for grid functions
// - rewrite_layout_float for float functions
// - rewrite_layout_positioning for positioning functions

// BFC utilities are now in a separate crate
// Use rewrite_layout_bfc directly

// Text layout
mod text {
    // pub mod inline;  // Disabled - needs SizeQuery
}

// Core layout (kept in base)
mod builder;
// pub mod helpers; // Disabled - needs refactoring for new marker system
mod layout_tree;
mod scroll;
pub mod transform;
mod writing_mode;

// Re-export layout tree types
pub use layout_tree::{BoxType, EdgeSizes, LayoutBox, LayoutTreeBuilder, Rect};

// Re-export builder functions
pub use builder::{build_layout_tree, compute_tree_size, dump_layout_tree};

// Re-export BFC functions from separate crate
pub use rewrite_layout_bfc::{
    blocks_margin_collapsing, creates_formatting_context, establishes_bfc, find_bfc_root,
};

// Note: margin collapsing, positioning, float, flex, and grid functions
// are now in separate crates. Users should import them directly:
// - rewrite_layout_positioning for margin/position/sticky functions
// - rewrite_layout_float for float functions
// - rewrite_layout_flex for flex functions
// - rewrite_layout_grid for grid functions

// Re-export writing mode functions
pub use writing_mode::{
    Direction, PhysicalAxis, PhysicalEdge, WritingMode, WritingModeContext, block_axis_to_physical,
    block_end_edge, block_progression_sign, block_start_edge, get_direction,
    get_physical_block_offset, get_writing_mode, inline_axis_to_physical, inline_end_edge,
    inline_progression_sign, inline_start_edge, is_horizontal_writing_mode,
    is_vertical_writing_mode, logical_to_physical_coords, logical_to_physical_size,
    resolve_logical_property,
};

// Inline layout functions disabled - module needs SizeQuery
// pub use text::inline::{...};

// Table layout functions disabled - module needs SizeQuery
// pub use formatting_contexts::table::{...};

// Re-export scroll integration
pub use scroll::{
    EasingFunction, ScrollAlignment, ScrollAnimation, ScrollBehavior, ScrollBounds, ScrollEvent,
    ScrollIntoViewRequest, ScrollPositionInput, ViewportScrollInput, apply_scroll_momentum,
    calculate_scroll_into_view, can_scroll, update_sticky_scroll_state,
};

// Re-export transform (module is already public)
pub use transform::{
    DecomposedTransform2D, Transform2D, Transform3D, TransformOrigin, parse_transform,
};

// Type aliases and query types are re-exported from the query crates at the top of this file

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
