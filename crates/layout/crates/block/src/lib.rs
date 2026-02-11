//! Block layout implementation using the formula system.

use lightningcss::properties::PropertyId;
use rewrite_core::{
    Aggregation, Axis, Formula, FormulaList, MultiRelationship, Operation, SingleRelationship,
};

/// Compute block size formula.
pub fn block_size(axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => block_width(),
        Axis::Vertical => block_height(),
    }
}

fn block_width() -> &'static Formula {
    // Block width fills parent's content box
    // TODO: Need a way to reference parent's computed width, not just CSS property
    // For now, use Width property as a placeholder
    static PARENT_WIDTH: Formula =
        Formula::RelatedValue(SingleRelationship::Parent, PropertyId::Width);
    static PARENT_PADDING_LEFT: Formula =
        Formula::RelatedValue(SingleRelationship::Parent, PropertyId::PaddingLeft);
    static PARENT_PADDING_RIGHT: Formula =
        Formula::RelatedValue(SingleRelationship::Parent, PropertyId::PaddingRight);
    static PARENT_BORDER_LEFT: Formula =
        Formula::RelatedValue(SingleRelationship::Parent, PropertyId::BorderLeftWidth);
    static PARENT_BORDER_RIGHT: Formula =
        Formula::RelatedValue(SingleRelationship::Parent, PropertyId::BorderRightWidth);

    static STEP1: Formula = Formula::Op(Operation::Sub, &PARENT_WIDTH, &PARENT_PADDING_LEFT);
    static STEP2: Formula = Formula::Op(Operation::Sub, &STEP1, &PARENT_PADDING_RIGHT);
    static STEP3: Formula = Formula::Op(Operation::Sub, &STEP2, &PARENT_BORDER_LEFT);
    static RESULT: Formula = Formula::Op(Operation::Sub, &STEP3, &PARENT_BORDER_RIGHT);

    &RESULT
}

fn block_height() -> &'static Formula {
    // Block height is sum of children heights
    // TODO: Need a way to reference children's computed heights
    static CHILDREN: FormulaList =
        FormulaList::CssValue(MultiRelationship::Children, PropertyId::Height);
    static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &CHILDREN);

    &RESULT
}

/// Min-content size for block elements.
pub fn block_min_content_size(axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => {
            // Min-content width is max of children's min-content widths
            static CHILDREN: FormulaList =
                FormulaList::CssValue(MultiRelationship::Children, PropertyId::MinWidth);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Max, &CHILDREN);
            &RESULT
        }
        Axis::Vertical => {
            // Min-content height is sum of children's min-content heights
            static CHILDREN: FormulaList =
                FormulaList::CssValue(MultiRelationship::Children, PropertyId::MinHeight);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &CHILDREN);
            &RESULT
        }
    }
}

/// Max-content size for block elements.
pub fn block_max_content_size(axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => {
            // Max-content width is max of children's max-content widths
            static CHILDREN: FormulaList =
                FormulaList::CssValue(MultiRelationship::Children, PropertyId::MaxWidth);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Max, &CHILDREN);
            &RESULT
        }
        Axis::Vertical => {
            // Max-content height is sum of children's max-content heights
            static CHILDREN: FormulaList =
                FormulaList::CssValue(MultiRelationship::Children, PropertyId::MaxHeight);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &CHILDREN);
            &RESULT
        }
    }
}

/// Compute block offset formula.
pub fn block_offset(axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => {
            // X offset is 0 for block elements (they stack vertically)
            static RESULT: Formula = Formula::Constant(0);
            &RESULT
        }
        Axis::Vertical => {
            // Y offset is sum of previous siblings' heights
            // TODO: Need a way to reference siblings' computed heights
            static PREV_SIBLINGS: FormulaList =
                FormulaList::CssValue(MultiRelationship::PrevSiblings, PropertyId::Height);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &PREV_SIBLINGS);
            &RESULT
        }
    }
}
