//! Flexbox layout formulas.
//!
//! Ported from `crates/layout/crates/flex/src/lib.rs`.
//! Key change: `FormulaList::CssValue` replaced with `FormulaList::Related`
//! and `ScopedStyler` replaced with `NodeStylerContext`.

use lightningcss::properties::PropertyId;
use lightningcss::properties::flex::FlexDirection;
use lightningcss::vendor_prefix::VendorPrefix;
use rewrite_core::{Aggregation, Axis, MultiRelationship};
use rewrite_css::{Formula, FormulaList, NodeStylerContext};

use super::size::{size_query_horizontal, size_query_vertical};

/// Compute flex container size formula.
/// Branches on flex-direction to determine main/cross axis.
/// Returns `None` if flex-direction isn't available yet.
pub fn flex_size(styler: &NodeStylerContext<'_>, axis: Axis) -> Option<&'static Formula> {
    let flex_direction = get_flex_direction(styler)?;
    let is_main = is_main_axis(axis, flex_direction);

    if is_main {
        Some(flex_main_axis_size(axis))
    } else {
        Some(flex_cross_axis_size(axis))
    }
}

/// Get the flex-direction property from a `NodeStylerContext`.
fn get_flex_direction(styler: &NodeStylerContext<'_>) -> Option<FlexDirection> {
    match styler.get_css_property(&PropertyId::FlexDirection(VendorPrefix::None))? {
        lightningcss::properties::Property::FlexDirection(dir, _) => Some(*dir),
        _ => None,
    }
}

/// Determine if the given axis is the main axis based on flex-direction.
fn is_main_axis(axis: Axis, direction: FlexDirection) -> bool {
    match direction {
        FlexDirection::Row | FlexDirection::RowReverse => axis == Axis::Horizontal,
        FlexDirection::Column | FlexDirection::ColumnReverse => axis == Axis::Vertical,
    }
}

fn flex_main_axis_size(axis: Axis) -> &'static Formula {
    // Main axis size is sum of children's computed sizes
    match axis {
        Axis::Horizontal => {
            static CHILDREN: FormulaList =
                FormulaList::Related(MultiRelationship::Children, size_query_horizontal);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &CHILDREN);
            &RESULT
        }
        Axis::Vertical => {
            static CHILDREN: FormulaList =
                FormulaList::Related(MultiRelationship::Children, size_query_vertical);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &CHILDREN);
            &RESULT
        }
    }
}

fn flex_cross_axis_size(axis: Axis) -> &'static Formula {
    // Cross axis size is max of children's computed sizes
    match axis {
        Axis::Horizontal => {
            static CHILDREN: FormulaList =
                FormulaList::Related(MultiRelationship::Children, size_query_horizontal);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Max, &CHILDREN);
            &RESULT
        }
        Axis::Vertical => {
            static CHILDREN: FormulaList =
                FormulaList::Related(MultiRelationship::Children, size_query_vertical);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Max, &CHILDREN);
            &RESULT
        }
    }
}

/// Min-content size for flex container.
pub fn flex_min_content_size(axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => {
            static CHILDREN: FormulaList =
                FormulaList::Related(MultiRelationship::Children, size_query_horizontal);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Max, &CHILDREN);
            &RESULT
        }
        Axis::Vertical => {
            static CHILDREN: FormulaList =
                FormulaList::Related(MultiRelationship::Children, size_query_vertical);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Max, &CHILDREN);
            &RESULT
        }
    }
}

/// Max-content size for flex container.
pub fn flex_max_content_size(axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => {
            static CHILDREN: FormulaList =
                FormulaList::Related(MultiRelationship::Children, size_query_horizontal);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &CHILDREN);
            &RESULT
        }
        Axis::Vertical => {
            static CHILDREN: FormulaList =
                FormulaList::Related(MultiRelationship::Children, size_query_vertical);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &CHILDREN);
            &RESULT
        }
    }
}

/// Compute flex item offset formula.
///
/// `parent_direction` is passed in from `offset_query` which already read
/// the parent's flex-direction before dispatching here.
pub fn flex_offset(parent_direction: FlexDirection, axis: Axis) -> &'static Formula {
    let is_main = is_main_axis(axis, parent_direction);

    if is_main {
        flex_main_axis_offset(axis)
    } else {
        // Cross axis offset depends on align-items, default to start (0)
        static ZERO: Formula = Formula::Constant(0);
        &ZERO
    }
}

fn flex_main_axis_offset(axis: Axis) -> &'static Formula {
    // Main axis offset is sum of previous siblings' computed sizes
    match axis {
        Axis::Horizontal => {
            static PREV_SIBLINGS: FormulaList =
                FormulaList::Related(MultiRelationship::PrevSiblings, size_query_horizontal);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &PREV_SIBLINGS);
            &RESULT
        }
        Axis::Vertical => {
            static PREV_SIBLINGS: FormulaList =
                FormulaList::Related(MultiRelationship::PrevSiblings, size_query_vertical);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &PREV_SIBLINGS);
            &RESULT
        }
    }
}
