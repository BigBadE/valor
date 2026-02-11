//! Flexbox layout implementation using the formula system.
//!
//! Query functions return `Option<&'static Formula>` - returning `None` when
//! the required CSS properties aren't available yet (low confidence).

use lightningcss::properties::PropertyId;
use lightningcss::properties::flex::FlexDirection;
use lightningcss::vendor_prefix::VendorPrefix;
use rewrite_core::{Aggregation, Axis, Formula, FormulaList, MultiRelationship};
use rewrite_css::ScopedStyler;

/// Compute flex container size formula.
/// Branches on flex-direction to determine main/cross axis.
/// Returns `None` if flex-direction isn't available yet.
pub fn flex_size(styler: &ScopedStyler<'_>, axis: Axis) -> Option<&'static Formula> {
    let flex_direction = get_flex_direction(styler)?;
    let is_main_axis = is_main_axis(axis, flex_direction);

    if is_main_axis {
        Some(flex_main_axis_size(axis))
    } else {
        Some(flex_cross_axis_size(axis))
    }
}

/// Get the flex-direction property, returning None if not set.
fn get_flex_direction(styler: &ScopedStyler<'_>) -> Option<FlexDirection> {
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
    // Main axis size is sum of flex items
    match axis {
        Axis::Horizontal => {
            static CHILDREN: FormulaList =
                FormulaList::CssValue(MultiRelationship::Children, PropertyId::Width);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &CHILDREN);
            &RESULT
        }
        Axis::Vertical => {
            static CHILDREN: FormulaList =
                FormulaList::CssValue(MultiRelationship::Children, PropertyId::Height);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &CHILDREN);
            &RESULT
        }
    }
}

fn flex_cross_axis_size(axis: Axis) -> &'static Formula {
    // Cross axis size is max of children
    match axis {
        Axis::Horizontal => {
            static CHILDREN: FormulaList =
                FormulaList::CssValue(MultiRelationship::Children, PropertyId::Width);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Max, &CHILDREN);
            &RESULT
        }
        Axis::Vertical => {
            static CHILDREN: FormulaList =
                FormulaList::CssValue(MultiRelationship::Children, PropertyId::Height);
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
                FormulaList::CssValue(MultiRelationship::Children, PropertyId::MinWidth);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Max, &CHILDREN);
            &RESULT
        }
        Axis::Vertical => {
            static CHILDREN: FormulaList =
                FormulaList::CssValue(MultiRelationship::Children, PropertyId::MinHeight);
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
                FormulaList::CssValue(MultiRelationship::Children, PropertyId::MaxWidth);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &CHILDREN);
            &RESULT
        }
        Axis::Vertical => {
            static CHILDREN: FormulaList =
                FormulaList::CssValue(MultiRelationship::Children, PropertyId::MaxHeight);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &CHILDREN);
            &RESULT
        }
    }
}

/// Compute flex item offset formula.
/// Branches on parent's flex-direction.
/// Returns `None` if parent's flex-direction isn't available yet.
pub fn flex_offset(styler: &ScopedStyler<'_>, axis: Axis) -> Option<&'static Formula> {
    // Get parent's flex-direction
    let parent = styler.parent()?;
    let flex_direction = get_flex_direction(&parent)?;

    let is_main_axis = is_main_axis(axis, flex_direction);

    if is_main_axis {
        Some(flex_main_axis_offset(axis))
    } else {
        // Cross axis offset depends on align-items, default to start (0)
        static ZERO: Formula = Formula::Constant(0);
        Some(&ZERO)
    }
}

fn flex_main_axis_offset(axis: Axis) -> &'static Formula {
    // Main axis offset is sum of previous siblings
    match axis {
        Axis::Horizontal => {
            static PREV_SIBLINGS: FormulaList =
                FormulaList::CssValue(MultiRelationship::PrevSiblings, PropertyId::Width);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &PREV_SIBLINGS);
            &RESULT
        }
        Axis::Vertical => {
            static PREV_SIBLINGS: FormulaList =
                FormulaList::CssValue(MultiRelationship::PrevSiblings, PropertyId::Height);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &PREV_SIBLINGS);
            &RESULT
        }
    }
}
