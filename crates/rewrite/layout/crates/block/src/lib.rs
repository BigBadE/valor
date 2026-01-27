//! Block layout implementation using the formula system.

use rewrite_core::*;
use rewrite_layout_size_impl::{LayoutProvider, SizeFormulaProvider};

/// Compute block size formula.
pub fn block_size(scoped: &mut ScopedDb, axis: Axis) -> &'static Formula {
    let provider = LayoutProvider;

    match axis {
        Axis::Horizontal => block_width(scoped, &provider),
        Axis::Vertical => block_height(scoped, &provider),
    }
}

fn block_width(_scoped: &mut ScopedDb, _provider: &LayoutProvider) -> &'static Formula {
    // Block width fills parent's content box
    static PARENT_WIDTH: Formula = Formula::RelatedValue(
        SingleRelationship::Parent,
        CssValueProperty::Size(Axis::Horizontal),
    );
    static PARENT_PADDING_LEFT: Formula = Formula::RelatedValue(
        SingleRelationship::Parent,
        CssValueProperty::Padding(Edge::Left),
    );
    static PARENT_PADDING_RIGHT: Formula = Formula::RelatedValue(
        SingleRelationship::Parent,
        CssValueProperty::Padding(Edge::Right),
    );
    static PARENT_BORDER_LEFT: Formula = Formula::RelatedValue(
        SingleRelationship::Parent,
        CssValueProperty::BorderWidth(Edge::Left),
    );
    static PARENT_BORDER_RIGHT: Formula = Formula::RelatedValue(
        SingleRelationship::Parent,
        CssValueProperty::BorderWidth(Edge::Right),
    );

    static STEP1: Formula = Formula::Sub(&PARENT_WIDTH, &PARENT_PADDING_LEFT);
    static STEP2: Formula = Formula::Sub(&STEP1, &PARENT_PADDING_RIGHT);
    static STEP3: Formula = Formula::Sub(&STEP2, &PARENT_BORDER_LEFT);
    static RESULT: Formula = Formula::Sub(&STEP3, &PARENT_BORDER_RIGHT);

    &RESULT
}

fn block_height(_scoped: &mut ScopedDb, _provider: &LayoutProvider) -> &'static Formula {
    // Block height is sum of children heights
    static CHILD_HEIGHT: Formula = Formula::Value(CssValueProperty::Size(Axis::Vertical));
    static CHILDREN: FormulaList = FormulaList::RelatedValue(
        MultiRelationship::Children,
        CssValueProperty::Size(Axis::Vertical),
    );
    static RESULT: Formula = Formula::List(CHILDREN);

    &RESULT
}

/// Compute block offset formula.
pub fn block_offset(scoped: &mut ScopedDb, axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => {
            // X offset is sum of previous siblings' widths
            static SIBLING_WIDTH: Formula =
                Formula::Value(CssValueProperty::Size(Axis::Horizontal));
            static PREV_SIBLINGS: FormulaList = FormulaList::RelatedValue(
                MultiRelationship::PrevSiblings,
                CssValueProperty::Size(Axis::Horizontal),
            );
            static RESULT: Formula = Formula::List(PREV_SIBLINGS);
            &RESULT
        }
        Axis::Vertical => {
            // Y offset is sum of previous siblings' heights
            static SIBLING_HEIGHT: Formula = Formula::Value(CssValueProperty::Size(Axis::Vertical));
            static PREV_SIBLINGS: FormulaList = FormulaList::RelatedValue(
                MultiRelationship::PrevSiblings,
                CssValueProperty::Size(Axis::Vertical),
            );
            static RESULT: Formula = Formula::List(PREV_SIBLINGS);
            &RESULT
        }
    }
}
