//! Block formatting context implementation using the formula system.

use lightningcss::properties::PropertyId;
use rewrite_core::{
    Aggregation, Axis, Formula, FormulaList, MultiRelationship, SingleRelationship,
};

/// Compute BFC root size formula.
pub fn bfc_size(axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => {
            // BFC width is content-based or fills parent
            static PARENT_WIDTH: Formula =
                Formula::RelatedValue(SingleRelationship::Parent, PropertyId::Width);
            &PARENT_WIDTH
        }
        Axis::Vertical => {
            // BFC height is sum of children
            static CHILDREN: FormulaList =
                FormulaList::CssValue(MultiRelationship::Children, PropertyId::Height);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &CHILDREN);
            &RESULT
        }
    }
}

/// Compute offset within a BFC.
pub fn bfc_offset(axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => {
            // X offset from BFC edge
            static RESULT: Formula = Formula::Constant(0);
            &RESULT
        }
        Axis::Vertical => {
            // Y offset is sum of previous siblings
            static PREV_SIBLINGS: FormulaList =
                FormulaList::CssValue(MultiRelationship::PrevSiblings, PropertyId::Height);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &PREV_SIBLINGS);
            &RESULT
        }
    }
}
