// Formatting contexts
mod formatting_contexts {
    pub mod bfc;
    pub mod flex;
    #[path = "float.rs"]
    pub mod float_layout;
    pub mod grid;
    pub mod table;
}

// Positioning
mod positioning {
    pub mod margin;
    pub mod position;
    pub mod sticky;
}

// Text layout
mod text {
    pub mod inline;
}

// Core layout
mod builder;
mod helpers;
mod layout_tree;
mod offset;
mod scroll;
mod size;
pub mod transform;
mod writing_mode;

pub use offset::compute_offset;
pub use size::compute_size;

// Re-export layout tree types
pub use layout_tree::{BoxType, EdgeSizes, LayoutBox, LayoutTreeBuilder, Rect};

// Re-export builder functions
pub use builder::{build_layout_tree, compute_tree_size, dump_layout_tree};

// Re-export BFC functions
pub use formatting_contexts::bfc::{
    blocks_margin_collapsing, creates_formatting_context, establishes_bfc, find_bfc_root,
};

// Re-export margin collapsing functions
pub use positioning::margin::{
    compute_collapsed_margin_end, compute_collapsed_margin_start, get_effective_margin_end,
    get_effective_margin_start,
};

// Re-export positioned layout functions
pub use positioning::position::{compute_positioned_offset, establishes_containing_block};

// Re-export float layout functions
pub use formatting_contexts::float_layout::{
    FloatBox, FloatDirection, compute_available_width_with_floats, compute_clearance,
    compute_float_avoiding_offset, compute_float_offset, compute_float_shrink_wrap_size,
    get_float_direction, is_float,
};

// Re-export sticky positioning functions
pub use positioning::sticky::{
    ScrollState, StickyConstraint, calculate_sticky_boundaries, compute_sticky_offset,
    get_scroll_state, get_sticky_scroll_container, has_sticky_threshold, is_currently_sticking,
};

// Re-export writing mode functions
pub use writing_mode::{
    Direction, PhysicalAxis, PhysicalEdge, WritingMode, WritingModeContext, block_axis_to_physical,
    block_end_edge, block_progression_sign, block_start_edge, get_direction,
    get_physical_block_offset, get_writing_mode, inline_axis_to_physical, inline_end_edge,
    inline_progression_sign, inline_start_edge, is_horizontal_writing_mode,
    is_vertical_writing_mode, logical_to_physical_coords, logical_to_physical_size,
    resolve_logical_property,
};

// Re-export inline layout functions
pub use text::inline::{
    BaselineAlignment, FontMetrics, InlineBox, InlineBoxType, LineBox, TextAlign, break_line,
    calculate_inline_content_height, calculate_inline_intrinsic_width, compute_baseline_offset,
    get_baseline_alignment, get_text_align, layout_inline_content, position_inline_boxes,
};

// Re-export table layout functions
pub use formatting_contexts::table::{
    TableCell, TableGrid, TableLayoutMode, build_table_grid, calculate_collapsed_border,
    calculate_table_size, compute_auto_column_widths, compute_fixed_column_widths,
    compute_row_heights, get_cell_position, get_table_layout_mode, is_border_collapse,
    layout_table,
};

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

// Type aliases for commonly used query combinations
pub type BlockOffsetQuery = OffsetQuery<BlockMarker>;
pub type InlineOffsetQuery = OffsetQuery<InlineMarker>;
pub type BlockSizeQuery = SizeQuery<BlockMarker, ConstrainedMarker>;
pub type InlineSizeQuery = SizeQuery<InlineMarker, ConstrainedMarker>;
pub type IntrinsicBlockSizeQuery = SizeQuery<BlockMarker, IntrinsicMarker>;
pub type IntrinsicInlineSizeQuery = SizeQuery<InlineMarker, IntrinsicMarker>;

/// Layout axis in logical coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, rewrite_macros::Markers)]
pub enum Layouts {
    /// Block axis (vertical in horizontal writing mode).
    Block,
    /// Inline axis (horizontal in horizontal writing mode).
    Inline,
}

/// Size computation mode for layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, rewrite_macros::Markers)]
pub enum SizeMode {
    /// Intrinsic size - measures content only (min-content/max-content).
    Intrinsic,
    /// Constrained size - considers available space and constraints.
    Constrained,
}

/// Subpixel value - 1/64th of a pixel.
/// Using integer subpixels provides exact arithmetic and avoids floating-point rounding errors.
pub type Subpixels = i32;

/// Query for layout properties (offset, size).
#[derive(rewrite_macros::Query)]
#[value_type(Subpixels)]
pub enum LayoutQuery {
    /// Offset (position) along an axis.
    #[query(compute_offset)]
    #[params(LayoutsMarker)]
    Offset,
    /// Size (dimension) along an axis with computation mode.
    #[query(compute_size)]
    #[params(LayoutsMarker, SizeModeMarker)]
    Size,
}

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
