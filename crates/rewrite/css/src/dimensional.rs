pub type Subpixels = i32;

/// Dimensional CSS properties that return subpixel values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, rewrite_macros::Query)]
#[value_type(Subpixels)]
pub enum DimensionalProperty {
    // Box Model - Sizing
    #[query(get_dimensional_property)]
    Width,
    #[query(get_dimensional_property)]
    Height,
    #[query(get_dimensional_property)]
    MinWidth,
    #[query(get_dimensional_property)]
    MinHeight,
    #[query(get_dimensional_property)]
    MaxWidth,
    #[query(get_dimensional_property)]
    MaxHeight,

    // Box Model - Directional properties (padding, margin, border)
    #[query(get_padding)]
    #[params(crate::direction::AxisMarker, crate::direction::LogicalDirectionMarker)]
    Padding,

    #[query(get_margin)]
    #[params(crate::direction::AxisMarker, crate::direction::LogicalDirectionMarker)]
    Margin,

    #[query(get_border_width)]
    #[params(crate::direction::AxisMarker, crate::direction::LogicalDirectionMarker)]
    BorderWidth,

    // Positioning offsets (top/right/bottom/left values)
    #[query(get_position_offset)]
    #[params(crate::direction::AxisMarker, crate::direction::LogicalDirectionMarker)]
    PositionOffset,

    // Flexbox/Grid Gaps
    #[query(get_dimensional_property)]
    RowGap,
    #[query(get_dimensional_property)]
    ColumnGap,
    #[query(get_dimensional_property)]
    Gap,

    // Flexbox Factors (stored as fixed-point: value * 64)
    #[query(get_dimensional_property)]
    FlexGrow,
    #[query(get_dimensional_property)]
    FlexShrink,
}

use crate::direction::{Axis, LogicalDirection, PhysicalDirection};

/// Get dimensional property value (width, height, gaps, flex factors).
fn get_dimensional_property(
    _db: &rewrite_core::Database,
    _node: rewrite_core::NodeId,
    _ctx: &mut rewrite_core::DependencyContext,
) -> Subpixels {
    // TODO: Determine which property this is and query the appropriate value
    // For now this requires passing the property name through the query system
    // This is a limitation of the current macro-generated query system

    // Temporary: return 0 until we refactor the query system to pass property names
    0
}

/// Get padding for a specific axis and direction.
fn get_padding(
    db: &rewrite_core::Database,
    node: rewrite_core::NodeId,
    axis: Axis,
    direction: LogicalDirection,
    ctx: &mut rewrite_core::DependencyContext,
) -> Subpixels {
    // Map logical direction to physical direction
    let physical = map_logical_to_physical(axis, direction);

    // Get property name based on physical direction
    let property = match physical {
        PhysicalDirection::Top => crate::storage::properties::PADDING_TOP,
        PhysicalDirection::Right => crate::storage::properties::PADDING_RIGHT,
        PhysicalDirection::Bottom => crate::storage::properties::PADDING_BOTTOM,
        PhysicalDirection::Left => crate::storage::properties::PADDING_LEFT,
    };

    // Query CSS value (automatically resolves em, %, etc.)
    db.query::<crate::storage::CssValueQuery>((node, property.to_string()), ctx)
        .subpixels_or_zero()
}

/// Get margin for a specific axis and direction.
fn get_margin(
    db: &rewrite_core::Database,
    node: rewrite_core::NodeId,
    axis: Axis,
    direction: LogicalDirection,
    ctx: &mut rewrite_core::DependencyContext,
) -> Subpixels {
    // Map logical direction to physical direction
    let physical = map_logical_to_physical(axis, direction);

    // Get property name based on physical direction
    let property = match physical {
        PhysicalDirection::Top => crate::storage::properties::MARGIN_TOP,
        PhysicalDirection::Right => crate::storage::properties::MARGIN_RIGHT,
        PhysicalDirection::Bottom => crate::storage::properties::MARGIN_BOTTOM,
        PhysicalDirection::Left => crate::storage::properties::MARGIN_LEFT,
    };

    // Query CSS value (automatically resolves em, %, etc.)
    // Note: margin can be 'auto', but we return 0 for now
    let resolved = db.query::<crate::storage::CssValueQuery>((node, property.to_string()), ctx);
    let subpixels = resolved.subpixels_or_zero();

    // DEBUG
    if subpixels != 0 {
        eprintln!(
            "get_margin: node={:?}, property={}, resolved={:?}, subpixels={}",
            node, property, resolved, subpixels
        );
    }

    subpixels
}

/// Get border width for a specific axis and direction.
fn get_border_width(
    db: &rewrite_core::Database,
    node: rewrite_core::NodeId,
    axis: Axis,
    direction: LogicalDirection,
    ctx: &mut rewrite_core::DependencyContext,
) -> Subpixels {
    // Map logical direction to physical direction
    let physical = map_logical_to_physical(axis, direction);

    // Get property name based on physical direction
    let property = match physical {
        PhysicalDirection::Top => crate::storage::properties::BORDER_TOP_WIDTH,
        PhysicalDirection::Right => crate::storage::properties::BORDER_RIGHT_WIDTH,
        PhysicalDirection::Bottom => crate::storage::properties::BORDER_BOTTOM_WIDTH,
        PhysicalDirection::Left => crate::storage::properties::BORDER_LEFT_WIDTH,
    };

    // Query CSS value (automatically resolves em, %, etc.)
    // Note: border-width can be affected by border-style (none/hidden = 0 width)
    db.query::<crate::storage::CssValueQuery>((node, property.to_string()), ctx)
        .subpixels_or_zero()
}

/// Get position offset (top/right/bottom/left) for a specific axis and direction.
fn get_position_offset(
    db: &rewrite_core::Database,
    node: rewrite_core::NodeId,
    axis: Axis,
    direction: LogicalDirection,
    ctx: &mut rewrite_core::DependencyContext,
) -> Subpixels {
    // Map logical direction to physical direction
    let physical = map_logical_to_physical(axis, direction);

    // Get property name based on physical direction
    let property = match physical {
        PhysicalDirection::Top => crate::storage::properties::TOP,
        PhysicalDirection::Right => crate::storage::properties::RIGHT,
        PhysicalDirection::Bottom => crate::storage::properties::BOTTOM,
        PhysicalDirection::Left => crate::storage::properties::LEFT,
    };

    // Query CSS value (automatically resolves em, %, etc.)
    // Note: position offsets can be 'auto', which returns 0
    db.query::<crate::storage::CssValueQuery>((node, property.to_string()), ctx)
        .subpixels_or_zero()
}

/// Map logical axis and direction to physical direction.
/// Assumes horizontal-tb writing mode (left-to-right, top-to-bottom).
fn map_logical_to_physical(axis: Axis, direction: LogicalDirection) -> PhysicalDirection {
    use crate::direction::{Axis::*, LogicalDirection::*, PhysicalDirection::*};

    match (axis, direction) {
        // Block axis in horizontal-tb: top-to-bottom
        (Block, Start) => Top,
        (Block, End) => Bottom,

        // Inline axis in horizontal-tb: left-to-right
        (Inline, Start) => Left,
        (Inline, End) => Right,
    }
}
