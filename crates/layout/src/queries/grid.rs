//! CSS Grid layout formulas.
//!
//! Ported from `crates/layout/crates/grid/src/lib.rs`.

use lightningcss::properties::PropertyId;
use rewrite_core::{Aggregation, Axis, MultiRelationship};
use rewrite_css::{Formula, FormulaList, NodeStylerContext};

use super::size::{size_query_horizontal, size_query_vertical};

/// Compute grid container size formula.
/// Returns `None` if grid properties aren't available yet.
pub fn grid_size(styler: &NodeStylerContext<'_>, axis: Axis) -> Option<&'static Formula> {
    // Require display property to be set (we know we're grid from caller)
    let _ = styler.get_css_property(&PropertyId::Display)?;

    // Sum of children sizes as placeholder
    match axis {
        Axis::Horizontal => {
            static CHILDREN: FormulaList =
                FormulaList::Related(MultiRelationship::Children, size_query_horizontal);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &CHILDREN);
            Some(&RESULT)
        }
        Axis::Vertical => {
            static CHILDREN: FormulaList =
                FormulaList::Related(MultiRelationship::Children, size_query_vertical);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &CHILDREN);
            Some(&RESULT)
        }
    }
}

/// Compute grid item offset formula.
/// Returns `None` if grid properties aren't available yet.
pub fn grid_offset(parent_styler: &NodeStylerContext<'_>, axis: Axis) -> Option<&'static Formula> {
    // Require parent's display property
    let _ = parent_styler.get_css_property(&PropertyId::Display)?;

    // Sum of previous siblings as placeholder
    match axis {
        Axis::Horizontal => {
            static PREV_SIBLINGS: FormulaList =
                FormulaList::Related(MultiRelationship::PrevSiblings, size_query_horizontal);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &PREV_SIBLINGS);
            Some(&RESULT)
        }
        Axis::Vertical => {
            static PREV_SIBLINGS: FormulaList =
                FormulaList::Related(MultiRelationship::PrevSiblings, size_query_vertical);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &PREV_SIBLINGS);
            Some(&RESULT)
        }
    }
}
