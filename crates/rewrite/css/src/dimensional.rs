pub type Subpixels = i32;

use crate::direction::{Axis, LogicalDirection, PhysicalDirection};

// ============================================================================
// PUBLIC API - Pure functions for CSS value resolution
// ============================================================================
// These functions are called by the rewrite_queries crate to implement queries.
// They don't call queries themselves, avoiding circular dependencies.

/// Get the raw CSS value for a dimensional property (padding, margin, border).
/// Returns None if the value is 'auto', Some(Subpixels) for concrete values.
pub fn get_dimensional_value(
    scoped: &mut rewrite_core::ScopedDb,
    property_name: &str,
) -> Option<Subpixels> {
    let node = scoped.node();
    let property_string = property_name.to_string();
    let resolved = scoped.query_with_key::<crate::storage::CssValueQuery>((node, property_string));

    if matches!(resolved, crate::storage::ResolvedValue::Auto) {
        None // Auto - caller needs to resolve
    } else {
        Some(resolved.subpixels_or_zero())
    }
}

/// Check if a CSS property is set to a specific keyword.
/// Returns true if the property value is the given keyword, false otherwise.
pub fn is_keyword(
    scoped: &mut rewrite_core::ScopedDb,
    property_name: &str,
    keyword: crate::CssKeyword,
) -> bool {
    let node = scoped.node();
    let property_string = property_name.to_string();
    let css_value =
        scoped.query_with_key::<crate::storage::InheritedCssPropertyQuery>((node, property_string));

    matches!(css_value, crate::CssValue::Keyword(kw) if kw == keyword)
}

/// Get padding property name for an axis and direction.
pub fn padding_property_name(axis: Axis, direction: LogicalDirection) -> &'static str {
    let physical = map_logical_to_physical(axis, direction);
    match physical {
        PhysicalDirection::Top => crate::storage::properties::PADDING_TOP,
        PhysicalDirection::Right => crate::storage::properties::PADDING_RIGHT,
        PhysicalDirection::Bottom => crate::storage::properties::PADDING_BOTTOM,
        PhysicalDirection::Left => crate::storage::properties::PADDING_LEFT,
    }
}

/// Get margin property name for an axis and direction.
pub fn margin_property_name(axis: Axis, direction: LogicalDirection) -> &'static str {
    let physical = map_logical_to_physical(axis, direction);
    match physical {
        PhysicalDirection::Top => crate::storage::properties::MARGIN_TOP,
        PhysicalDirection::Right => crate::storage::properties::MARGIN_RIGHT,
        PhysicalDirection::Bottom => crate::storage::properties::MARGIN_BOTTOM,
        PhysicalDirection::Left => crate::storage::properties::MARGIN_LEFT,
    }
}

/// Get border width property name for an axis and direction.
pub fn border_width_property_name(axis: Axis, direction: LogicalDirection) -> &'static str {
    let physical = map_logical_to_physical(axis, direction);
    match physical {
        PhysicalDirection::Top => crate::storage::properties::BORDER_TOP_WIDTH,
        PhysicalDirection::Right => crate::storage::properties::BORDER_RIGHT_WIDTH,
        PhysicalDirection::Bottom => crate::storage::properties::BORDER_BOTTOM_WIDTH,
        PhysicalDirection::Left => crate::storage::properties::BORDER_LEFT_WIDTH,
    }
}

/// Get border style property name for an axis and direction.
pub fn border_style_property_name(axis: Axis, direction: LogicalDirection) -> &'static str {
    let physical = map_logical_to_physical(axis, direction);
    match physical {
        PhysicalDirection::Top => crate::storage::properties::BORDER_TOP_STYLE,
        PhysicalDirection::Right => crate::storage::properties::BORDER_RIGHT_STYLE,
        PhysicalDirection::Bottom => crate::storage::properties::BORDER_BOTTOM_STYLE,
        PhysicalDirection::Left => crate::storage::properties::BORDER_LEFT_STYLE,
    }
}

/// Get position offset property name for an axis and direction.
pub fn position_offset_property_name(axis: Axis, direction: LogicalDirection) -> &'static str {
    let physical = map_logical_to_physical(axis, direction);
    match physical {
        PhysicalDirection::Top => crate::storage::properties::TOP,
        PhysicalDirection::Right => crate::storage::properties::RIGHT,
        PhysicalDirection::Bottom => crate::storage::properties::BOTTOM,
        PhysicalDirection::Left => crate::storage::properties::LEFT,
    }
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
