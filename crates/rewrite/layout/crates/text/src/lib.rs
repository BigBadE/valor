//! Text layout implementation using the formula system.

use rewrite_core::*;

/// Compute text node size formula.
pub fn text_size(_scoped: &mut ScopedDb, axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => {
            // Text width would be measured by text shaping
            // For now, return a constant placeholder
            static RESULT: Formula = Formula::Constant(0);
            &RESULT
        }
        Axis::Vertical => {
            // Text height is typically line-height
            static RESULT: Formula = Formula::Value(CssValueProperty::LineHeight);
            &RESULT
        }
    }
}

/// Compute text node offset formula.
pub fn text_offset(_scoped: &mut ScopedDb, axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => {
            // Text X offset is sum of previous siblings
            static PREV_SIBLINGS: FormulaList = FormulaList::RelatedValue(
                MultiRelationship::PrevSiblings,
                CssValueProperty::Size(Axis::Horizontal),
            );
            static RESULT: Formula = Formula::List(PREV_SIBLINGS);
            &RESULT
        }
        Axis::Vertical => {
            // Text Y offset is sum of previous siblings
            static PREV_SIBLINGS: FormulaList = FormulaList::RelatedValue(
                MultiRelationship::PrevSiblings,
                CssValueProperty::Size(Axis::Vertical),
            );
            static RESULT: Formula = Formula::List(PREV_SIBLINGS);
            &RESULT
        }
    }
}
