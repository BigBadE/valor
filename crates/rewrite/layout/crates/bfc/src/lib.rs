//! Block formatting context implementation using the formula system.

use rewrite_core::*;

/// Compute BFC root size formula.
pub fn bfc_size(_scoped: &mut ScopedDb, axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => {
            // BFC width is content-based or fills parent
            static PARENT_WIDTH: Formula = Formula::RelatedValue(
                SingleRelationship::Parent,
                CssValueProperty::Size(Axis::Horizontal),
            );
            &PARENT_WIDTH
        }
        Axis::Vertical => {
            // BFC height is sum of children
            static CHILDREN: FormulaList = FormulaList::RelatedValue(
                MultiRelationship::Children,
                CssValueProperty::Size(Axis::Vertical),
            );
            static RESULT: Formula = Formula::List(CHILDREN);
            &RESULT
        }
    }
}

/// Compute offset within a BFC.
pub fn bfc_offset(_scoped: &mut ScopedDb, axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => {
            // X offset from BFC edge
            static RESULT: Formula = Formula::Constant(0);
            &RESULT
        }
        Axis::Vertical => {
            // Y offset is sum of previous siblings
            static PREV_SIBLINGS: FormulaList = FormulaList::RelatedValue(
                MultiRelationship::PrevSiblings,
                CssValueProperty::Size(Axis::Vertical),
            );
            static RESULT: Formula = Formula::List(PREV_SIBLINGS);
            &RESULT
        }
    }
}
