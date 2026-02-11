//! CSS Grid layout implementation using the formula system.
//!
//! Query functions return `Option<&'static Formula>` - returning `None` when
//! the required CSS properties aren't available yet (low confidence).

use lightningcss::properties::PropertyId;
use rewrite_core::{Aggregation, Axis, Formula, FormulaList, MultiRelationship};
use rewrite_css::ScopedStyler;

/// Compute grid container size formula.
/// TODO: Implement proper grid sizing based on grid-template-columns/rows.
/// Returns `None` if grid properties aren't available yet.
pub fn grid_size(styler: &ScopedStyler<'_>, axis: Axis) -> Option<&'static Formula> {
    // TODO: Query grid-template-columns/rows and compute based on track sizes
    // For now, require display property to be set (we know we're grid from caller)
    let _ = styler.get_css_property(&PropertyId::Display)?;

    // Sum of children sizes as placeholder
    match axis {
        Axis::Horizontal => {
            static CHILDREN: FormulaList =
                FormulaList::CssValue(MultiRelationship::Children, PropertyId::Width);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &CHILDREN);
            Some(&RESULT)
        }
        Axis::Vertical => {
            static CHILDREN: FormulaList =
                FormulaList::CssValue(MultiRelationship::Children, PropertyId::Height);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &CHILDREN);
            Some(&RESULT)
        }
    }
}

/// Compute grid item offset formula.
/// TODO: Implement proper grid positioning based on grid-column/grid-row.
/// Returns `None` if grid properties aren't available yet.
pub fn grid_offset(styler: &ScopedStyler<'_>, axis: Axis) -> Option<&'static Formula> {
    // TODO: Query grid-column/grid-row and compute based on track positions
    // For now, require parent's display property to be set
    let parent = styler.parent()?;
    let _ = parent.get_css_property(&PropertyId::Display)?;

    // Sum of previous siblings as placeholder
    match axis {
        Axis::Horizontal => {
            static PREV_SIBLINGS: FormulaList =
                FormulaList::CssValue(MultiRelationship::PrevSiblings, PropertyId::Width);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &PREV_SIBLINGS);
            Some(&RESULT)
        }
        Axis::Vertical => {
            static PREV_SIBLINGS: FormulaList =
                FormulaList::CssValue(MultiRelationship::PrevSiblings, PropertyId::Height);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &PREV_SIBLINGS);
            Some(&RESULT)
        }
    }
}
