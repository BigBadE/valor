//! Dimensional CSS property queries
//!
//! This module provides queries for dimensional CSS properties (padding, border, gaps).
//! MarginQuery is re-exported from layout_margin which resolves auto directly.

use rewrite_core::ScopedDb;
use rewrite_css::{Axis, LogicalDirection, Subpixels};

// Re-export MarginQuery from layout_margin (it resolves auto by calling SizeQuery)
pub use rewrite_layout_margin::MarginQuery;

/// Dimensional CSS properties that return subpixel values.
/// Note: MarginQuery is re-exported separately from layout_margin
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, rewrite_macros::Query)]
#[value_type(Subpixels)]
pub enum DimensionalProperty {
    #[query(get_padding)]
    #[params(rewrite_css::AxisMarker, rewrite_css::LogicalDirectionMarker)]
    Padding,

    #[query(get_border_width)]
    #[params(rewrite_css::AxisMarker, rewrite_css::LogicalDirectionMarker)]
    BorderWidth,

    #[query(get_row_gap)]
    RowGap,

    #[query(get_column_gap)]
    ColumnGap,

    #[query(get_flex_grow)]
    FlexGrow,

    #[query(get_flex_shrink)]
    FlexShrink,

    #[query(get_position_offset)]
    #[params(rewrite_css::AxisMarker, rewrite_css::LogicalDirectionMarker)]
    PositionOffset,
}

// The macro automatically generates type aliases:
// - MarginQuery, PaddingQuery, BorderWidthQuery, PositionOffsetQuery, etc.

// ============================================================================
// Query Implementations
// ============================================================================

fn get_padding(scoped: &mut ScopedDb, axis: Axis, direction: LogicalDirection) -> Subpixels {
    let property = rewrite_css::padding_property_name(axis, direction);
    rewrite_css::get_dimensional_value(scoped, property).unwrap_or(0)
}

fn get_border_width(scoped: &mut ScopedDb, axis: Axis, direction: LogicalDirection) -> Subpixels {
    // Check border-style first - if it's 'none' (or not set, which defaults to none),
    // border-width has no effect
    let style_property = rewrite_css::border_style_property_name(axis, direction);

    // Check if border-style is explicitly 'none' or not set (which defaults to 'none')
    // Use is_keyword to check if it's explicitly set to 'none'
    if rewrite_css::is_keyword(scoped, style_property, rewrite_css::CssKeyword::None) {
        return 0;
    }

    // Also check if it's unset (which defaults to none)
    // InheritedCssPropertyQuery will return the Initial keyword if not in cascade
    use rewrite_css::storage::InheritedCssPropertyQuery;
    let style_css_value = scoped
        .query_with_key::<InheritedCssPropertyQuery>((scoped.node(), style_property.to_string()));

    // If not explicitly set to a border style keyword, it defaults to 'none'
    let has_border_style = match style_css_value {
        rewrite_css::CssValue::Keyword(kw) => {
            // Has a keyword - check if it's not 'none' or 'initial'
            !matches!(
                kw,
                rewrite_css::CssKeyword::None | rewrite_css::CssKeyword::Initial
            )
        }
        _ => false, // Other values (shouldn't happen for border-style)
    };

    if !has_border_style {
        return 0;
    }

    let property = rewrite_css::border_width_property_name(axis, direction);
    rewrite_css::get_dimensional_value(scoped, property).unwrap_or(0)
}

fn get_row_gap(scoped: &mut ScopedDb) -> Subpixels {
    rewrite_css::get_dimensional_value(scoped, "row-gap").unwrap_or(0)
}

fn get_column_gap(scoped: &mut ScopedDb) -> Subpixels {
    rewrite_css::get_dimensional_value(scoped, "column-gap").unwrap_or(0)
}

fn get_flex_grow(scoped: &mut ScopedDb) -> Subpixels {
    rewrite_css::get_dimensional_value(scoped, "flex-grow").unwrap_or(0)
}

fn get_flex_shrink(scoped: &mut ScopedDb) -> Subpixels {
    rewrite_css::get_dimensional_value(scoped, "flex-shrink").unwrap_or(64) // Default 1.0 = 64
}

fn get_position_offset(
    scoped: &mut ScopedDb,
    axis: Axis,
    direction: LogicalDirection,
) -> Subpixels {
    let property = rewrite_css::position_offset_property_name(axis, direction);
    rewrite_css::get_dimensional_value(scoped, property).unwrap_or(0)
}
