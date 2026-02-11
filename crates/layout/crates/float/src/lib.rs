//! Float layout implementation using the formula system.
//!
//! TODO: This is a simplified implementation that doesn't query CSS properties
//! at runtime. A proper implementation would need conditional formulas based
//! on the float property value.

use lightningcss::properties::PropertyId;
use rewrite_core::{Aggregation, Axis, Formula, FormulaList, MultiRelationship};

/// Compute float size formula.
pub fn float_size(axis: Axis) -> &'static Formula {
    // TODO: Implement proper float sizing algorithm
    // For now, use intrinsic content size
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

/// Compute float offset formula.
/// TODO: Should branch on float property value (left, right, none)
pub fn float_offset(axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => {
            // Simplified: position at left edge (float: left behavior)
            static RESULT: Formula = Formula::Constant(0);
            &RESULT
        }
        Axis::Vertical => {
            // Vertical offset based on previous floats
            static PREV_SIBLINGS: FormulaList =
                FormulaList::CssValue(MultiRelationship::PrevSiblings, PropertyId::Height);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &PREV_SIBLINGS);
            &RESULT
        }
    }
}
