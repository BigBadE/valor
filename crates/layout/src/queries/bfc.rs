//! Block formatting context formulas.
//!
//! Ported from `crates/layout/crates/bfc/src/lib.rs`.

use rewrite_core::{Aggregation, Axis, MultiRelationship, SingleRelationship};
use rewrite_css::{Formula, FormulaList};

use super::size::{size_query_horizontal, size_query_vertical};

/// Compute BFC root size formula.
pub fn bfc_size(axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => {
            // BFC width fills parent's computed width
            static PARENT_WIDTH: Formula =
                Formula::Related(SingleRelationship::Parent, size_query_horizontal);
            &PARENT_WIDTH
        }
        Axis::Vertical => {
            // BFC height is sum of children's computed heights
            static CHILDREN: FormulaList =
                FormulaList::Related(MultiRelationship::Children, size_query_vertical);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &CHILDREN);
            &RESULT
        }
    }
}

/// Compute offset within a BFC.
pub fn bfc_offset(axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => {
            static RESULT: Formula = Formula::Constant(0);
            &RESULT
        }
        Axis::Vertical => {
            // Y offset is sum of previous siblings' computed heights
            static PREV_SIBLINGS: FormulaList =
                FormulaList::Related(MultiRelationship::PrevSiblings, size_query_vertical);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &PREV_SIBLINGS);
            &RESULT
        }
    }
}
