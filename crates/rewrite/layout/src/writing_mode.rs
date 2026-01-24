/// Writing modes module implementing CSS Writing Modes Level 3.
///
/// This module handles:
/// - Writing mode transformation (horizontal-tb, vertical-rl, vertical-lr)
/// - Logical to physical property mapping
/// - Text direction (ltr, rtl)
/// - Block and inline axis determination
///
/// Spec: https://www.w3.org/TR/css-writing-modes-3/
use crate::{BlockMarker, Subpixels};
use rewrite_core::ScopedDb;
use rewrite_css::{CssKeyword, CssValue, DirectionQuery, WritingModeQuery};

/// Writing mode determines the block and inline flow directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WritingMode {
    /// Horizontal top-to-bottom (default for Latin scripts).
    /// - Block flow: top to bottom
    /// - Inline flow: left to right (or right to left with direction: rtl)
    HorizontalTb,

    /// Vertical right-to-left (common for East Asian scripts).
    /// - Block flow: right to left
    /// - Inline flow: top to bottom
    VerticalRl,

    /// Vertical left-to-right (Mongolian script).
    /// - Block flow: left to right
    /// - Inline flow: top to bottom
    VerticalLr,
}

/// Text direction (ltr or rtl).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Left-to-right (default for most scripts).
    Ltr,
    /// Right-to-left (Arabic, Hebrew, etc.).
    Rtl,
}

/// Physical direction in 2D space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicalAxis {
    Horizontal,
    Vertical,
}

/// Physical edge direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicalEdge {
    Top,
    Right,
    Bottom,
    Left,
}

/// Get the writing mode for an element.
pub fn get_writing_mode(scoped: &mut ScopedDb) -> WritingMode {
    let writing_mode = scoped.query::<WritingModeQuery>();

    match writing_mode {
        CssValue::Keyword(CssKeyword::HorizontalTb) => WritingMode::HorizontalTb,
        CssValue::Keyword(CssKeyword::VerticalRl) => WritingMode::VerticalRl,
        CssValue::Keyword(CssKeyword::VerticalLr) => WritingMode::VerticalLr,
        _ => WritingMode::HorizontalTb, // Default
    }
}

/// Get the text direction for an element.
pub fn get_direction(scoped: &mut ScopedDb) -> Direction {
    let direction = scoped.query::<DirectionQuery>();

    match direction {
        CssValue::Keyword(CssKeyword::Right) => Direction::Rtl,
        CssValue::Keyword(CssKeyword::Left) | _ => Direction::Ltr, // Default to LTR
    }
}

/// Map logical block axis to physical axis based on writing mode.
///
/// # Examples:
/// - horizontal-tb: block axis is vertical (top to bottom)
/// - vertical-rl: block axis is horizontal (right to left)
/// - vertical-lr: block axis is horizontal (left to right)
pub fn block_axis_to_physical(writing_mode: WritingMode) -> PhysicalAxis {
    match writing_mode {
        WritingMode::HorizontalTb => PhysicalAxis::Vertical,
        WritingMode::VerticalRl | WritingMode::VerticalLr => PhysicalAxis::Horizontal,
    }
}

/// Map logical inline axis to physical axis based on writing mode.
///
/// # Examples:
/// - horizontal-tb: inline axis is horizontal (left to right or right to left)
/// - vertical-rl: inline axis is vertical (top to bottom)
/// - vertical-lr: inline axis is vertical (top to bottom)
pub fn inline_axis_to_physical(writing_mode: WritingMode) -> PhysicalAxis {
    match writing_mode {
        WritingMode::HorizontalTb => PhysicalAxis::Horizontal,
        WritingMode::VerticalRl | WritingMode::VerticalLr => PhysicalAxis::Vertical,
    }
}

/// Map logical block-start edge to physical edge.
///
/// The block-start edge is where block-level elements begin.
///
/// # Examples:
/// - horizontal-tb: top edge
/// - vertical-rl: right edge
/// - vertical-lr: left edge
pub fn block_start_edge(writing_mode: WritingMode) -> PhysicalEdge {
    match writing_mode {
        WritingMode::HorizontalTb => PhysicalEdge::Top,
        WritingMode::VerticalRl => PhysicalEdge::Right,
        WritingMode::VerticalLr => PhysicalEdge::Left,
    }
}

/// Map logical block-end edge to physical edge.
pub fn block_end_edge(writing_mode: WritingMode) -> PhysicalEdge {
    match writing_mode {
        WritingMode::HorizontalTb => PhysicalEdge::Bottom,
        WritingMode::VerticalRl => PhysicalEdge::Left,
        WritingMode::VerticalLr => PhysicalEdge::Right,
    }
}

/// Map logical inline-start edge to physical edge.
///
/// The inline-start edge depends on both writing mode and text direction.
///
/// # Examples:
/// - horizontal-tb + ltr: left edge
/// - horizontal-tb + rtl: right edge
/// - vertical-rl: top edge (direction doesn't affect vertical modes)
/// - vertical-lr: top edge
pub fn inline_start_edge(writing_mode: WritingMode, direction: Direction) -> PhysicalEdge {
    match writing_mode {
        WritingMode::HorizontalTb => match direction {
            Direction::Ltr => PhysicalEdge::Left,
            Direction::Rtl => PhysicalEdge::Right,
        },
        WritingMode::VerticalRl | WritingMode::VerticalLr => PhysicalEdge::Top,
    }
}

/// Map logical inline-end edge to physical edge.
pub fn inline_end_edge(writing_mode: WritingMode, direction: Direction) -> PhysicalEdge {
    match writing_mode {
        WritingMode::HorizontalTb => match direction {
            Direction::Ltr => PhysicalEdge::Right,
            Direction::Rtl => PhysicalEdge::Left,
        },
        WritingMode::VerticalRl | WritingMode::VerticalLr => PhysicalEdge::Bottom,
    }
}

/// Transform coordinates from logical to physical space.
///
/// Takes logical (block, inline) coordinates and returns physical (x, y) coordinates.
pub fn logical_to_physical_coords(
    block: Subpixels,
    inline: Subpixels,
    writing_mode: WritingMode,
) -> (Subpixels, Subpixels) {
    match writing_mode {
        WritingMode::HorizontalTb => {
            // Block = Y (vertical), Inline = X (horizontal)
            (inline, block)
        }
        WritingMode::VerticalRl => {
            // Block = X (right to left), Inline = Y (top to bottom)
            // Note: right-to-left means we need to flip X during rendering
            (block, inline)
        }
        WritingMode::VerticalLr => {
            // Block = X (left to right), Inline = Y (top to bottom)
            (block, inline)
        }
    }
}

/// Transform size from logical to physical space.
///
/// Takes logical (block-size, inline-size) and returns physical (width, height).
pub fn logical_to_physical_size(
    block_size: Subpixels,
    inline_size: Subpixels,
    writing_mode: WritingMode,
) -> (Subpixels, Subpixels) {
    match writing_mode {
        WritingMode::HorizontalTb => {
            // Block-size = height, Inline-size = width
            (inline_size, block_size)
        }
        WritingMode::VerticalRl | WritingMode::VerticalLr => {
            // Block-size = width, Inline-size = height
            (block_size, inline_size)
        }
    }
}

/// Get the block-axis offset in physical coordinates.
///
/// This is used for positioning elements in their containing block.
pub fn get_physical_block_offset<Axis>(
    scoped: &mut ScopedDb,
    logical_offset: Subpixels,
) -> Subpixels
where
    Axis: rewrite_layout_util::AxisMarker + 'static,
{
    let writing_mode = get_writing_mode(scoped);

    // Determine if this axis is the block axis in the current writing mode
    let is_block_axis = std::any::TypeId::of::<Axis>() == std::any::TypeId::of::<BlockMarker>();

    match (writing_mode, is_block_axis) {
        (WritingMode::HorizontalTb, true) => {
            // Block offset is vertical (Y) - no transformation needed
            logical_offset
        }
        (WritingMode::HorizontalTb, false) => {
            // Inline offset is horizontal (X) - no transformation needed
            logical_offset
        }
        (WritingMode::VerticalRl, true) => {
            // Block offset is horizontal (X), flowing right to left
            // May need to flip based on container width
            logical_offset
        }
        (WritingMode::VerticalRl, false) => {
            // Inline offset is vertical (Y) - no transformation needed
            logical_offset
        }
        (WritingMode::VerticalLr, true) => {
            // Block offset is horizontal (X), flowing left to right
            logical_offset
        }
        (WritingMode::VerticalLr, false) => {
            // Inline offset is vertical (Y) - no transformation needed
            logical_offset
        }
    }
}

/// Check if the current writing mode is horizontal.
pub fn is_horizontal_writing_mode(writing_mode: WritingMode) -> bool {
    matches!(writing_mode, WritingMode::HorizontalTb)
}

/// Check if the current writing mode is vertical.
pub fn is_vertical_writing_mode(writing_mode: WritingMode) -> bool {
    matches!(
        writing_mode,
        WritingMode::VerticalRl | WritingMode::VerticalLr
    )
}

/// Get the progression direction along the block axis.
///
/// Returns +1 for normal progression, -1 for reverse progression.
pub fn block_progression_sign(writing_mode: WritingMode) -> i32 {
    match writing_mode {
        WritingMode::HorizontalTb | WritingMode::VerticalLr => 1, // Top to bottom, left to right
        WritingMode::VerticalRl => -1,                            // Right to left
    }
}

/// Get the progression direction along the inline axis.
///
/// Returns +1 for normal progression, -1 for reverse progression.
pub fn inline_progression_sign(writing_mode: WritingMode, direction: Direction) -> i32 {
    match writing_mode {
        WritingMode::HorizontalTb => match direction {
            Direction::Ltr => 1,  // Left to right
            Direction::Rtl => -1, // Right to left
        },
        WritingMode::VerticalRl | WritingMode::VerticalLr => 1, // Always top to bottom
    }
}

/// Helper to get the writing mode context for an element.
///
/// This includes both writing-mode and direction properties.
#[derive(Debug, Clone, Copy)]
pub struct WritingModeContext {
    pub writing_mode: WritingMode,
    pub direction: Direction,
}

impl WritingModeContext {
    pub fn from_element(scoped: &mut ScopedDb) -> Self {
        Self {
            writing_mode: get_writing_mode(scoped),
            direction: get_direction(scoped),
        }
    }

    pub fn block_start_edge(&self) -> PhysicalEdge {
        block_start_edge(self.writing_mode)
    }

    pub fn block_end_edge(&self) -> PhysicalEdge {
        block_end_edge(self.writing_mode)
    }

    pub fn inline_start_edge(&self) -> PhysicalEdge {
        inline_start_edge(self.writing_mode, self.direction)
    }

    pub fn inline_end_edge(&self) -> PhysicalEdge {
        inline_end_edge(self.writing_mode, self.direction)
    }

    pub fn is_horizontal(&self) -> bool {
        is_horizontal_writing_mode(self.writing_mode)
    }

    pub fn is_vertical(&self) -> bool {
        is_vertical_writing_mode(self.writing_mode)
    }
}

/// Update logical properties based on writing mode.
///
/// This is used by the CSS property system to correctly interpret logical
/// properties like margin-inline-start, padding-block-end, etc.
pub fn resolve_logical_property<'a>(
    property_name: &'a str,
    writing_mode: WritingMode,
    direction: Direction,
) -> &'a str {
    // Map logical properties to physical properties
    match property_name {
        // Margin
        "margin-block-start" => match block_start_edge(writing_mode) {
            PhysicalEdge::Top => "margin-top",
            PhysicalEdge::Right => "margin-right",
            PhysicalEdge::Bottom => "margin-bottom",
            PhysicalEdge::Left => "margin-left",
        },
        "margin-block-end" => match block_end_edge(writing_mode) {
            PhysicalEdge::Top => "margin-top",
            PhysicalEdge::Right => "margin-right",
            PhysicalEdge::Bottom => "margin-bottom",
            PhysicalEdge::Left => "margin-left",
        },
        "margin-inline-start" => match inline_start_edge(writing_mode, direction) {
            PhysicalEdge::Top => "margin-top",
            PhysicalEdge::Right => "margin-right",
            PhysicalEdge::Bottom => "margin-bottom",
            PhysicalEdge::Left => "margin-left",
        },
        "margin-inline-end" => match inline_end_edge(writing_mode, direction) {
            PhysicalEdge::Top => "margin-top",
            PhysicalEdge::Right => "margin-right",
            PhysicalEdge::Bottom => "margin-bottom",
            PhysicalEdge::Left => "margin-left",
        },

        // Padding (similar mapping)
        "padding-block-start" => match block_start_edge(writing_mode) {
            PhysicalEdge::Top => "padding-top",
            PhysicalEdge::Right => "padding-right",
            PhysicalEdge::Bottom => "padding-bottom",
            PhysicalEdge::Left => "padding-left",
        },
        "padding-block-end" => match block_end_edge(writing_mode) {
            PhysicalEdge::Top => "padding-top",
            PhysicalEdge::Right => "padding-right",
            PhysicalEdge::Bottom => "padding-bottom",
            PhysicalEdge::Left => "padding-left",
        },
        "padding-inline-start" => match inline_start_edge(writing_mode, direction) {
            PhysicalEdge::Top => "padding-top",
            PhysicalEdge::Right => "padding-right",
            PhysicalEdge::Bottom => "padding-bottom",
            PhysicalEdge::Left => "padding-left",
        },
        "padding-inline-end" => match inline_end_edge(writing_mode, direction) {
            PhysicalEdge::Top => "padding-top",
            PhysicalEdge::Right => "padding-right",
            PhysicalEdge::Bottom => "padding-bottom",
            PhysicalEdge::Left => "padding-left",
        },

        // Border
        "border-block-start-width" => match block_start_edge(writing_mode) {
            PhysicalEdge::Top => "border-top-width",
            PhysicalEdge::Right => "border-right-width",
            PhysicalEdge::Bottom => "border-bottom-width",
            PhysicalEdge::Left => "border-left-width",
        },
        "border-block-end-width" => match block_end_edge(writing_mode) {
            PhysicalEdge::Top => "border-top-width",
            PhysicalEdge::Right => "border-right-width",
            PhysicalEdge::Bottom => "border-bottom-width",
            PhysicalEdge::Left => "border-left-width",
        },
        "border-inline-start-width" => match inline_start_edge(writing_mode, direction) {
            PhysicalEdge::Top => "border-top-width",
            PhysicalEdge::Right => "border-right-width",
            PhysicalEdge::Bottom => "border-bottom-width",
            PhysicalEdge::Left => "border-left-width",
        },
        "border-inline-end-width" => match inline_end_edge(writing_mode, direction) {
            PhysicalEdge::Top => "border-top-width",
            PhysicalEdge::Right => "border-right-width",
            PhysicalEdge::Bottom => "border-bottom-width",
            PhysicalEdge::Left => "border-left-width",
        },

        // Sizing
        "block-size" => match writing_mode {
            WritingMode::HorizontalTb => "height",
            WritingMode::VerticalRl | WritingMode::VerticalLr => "width",
        },
        "inline-size" => match writing_mode {
            WritingMode::HorizontalTb => "width",
            WritingMode::VerticalRl | WritingMode::VerticalLr => "height",
        },

        // Positioning
        "inset-block-start" => match block_start_edge(writing_mode) {
            PhysicalEdge::Top => "top",
            PhysicalEdge::Right => "right",
            PhysicalEdge::Bottom => "bottom",
            PhysicalEdge::Left => "left",
        },
        "inset-block-end" => match block_end_edge(writing_mode) {
            PhysicalEdge::Top => "top",
            PhysicalEdge::Right => "right",
            PhysicalEdge::Bottom => "bottom",
            PhysicalEdge::Left => "left",
        },
        "inset-inline-start" => match inline_start_edge(writing_mode, direction) {
            PhysicalEdge::Top => "top",
            PhysicalEdge::Right => "right",
            PhysicalEdge::Bottom => "bottom",
            PhysicalEdge::Left => "left",
        },
        "inset-inline-end" => match inline_end_edge(writing_mode, direction) {
            PhysicalEdge::Top => "top",
            PhysicalEdge::Right => "right",
            PhysicalEdge::Bottom => "bottom",
            PhysicalEdge::Left => "left",
        },

        // Not a logical property, return as-is
        _ => property_name,
    }
}
