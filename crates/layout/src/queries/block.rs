//! Block layout formulas.
//!
//! Block elements fill their parent's content box horizontally and stack
//! vertically. Positions account for margins, and sizes account for
//! padding and border.

use lightningcss::properties::PropertyId;
use rewrite_core::{Aggregation, Axis, MultiRelationship, Operation, SingleRelationship};
use rewrite_css::{Formula, FormulaList};

use super::size::{content_size_query_horizontal, size_query_horizontal, size_query_vertical};

// ============================================================================
// Size formulas
// ============================================================================

/// Compute block size formula for the given axis.
pub fn block_size(axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => block_width(),
        Axis::Vertical => block_height(),
    }
}

fn block_width() -> &'static Formula {
    // Block auto width (border-box) = parent content-area width - own margins.
    // content_size_query_horizontal returns the parent's content area
    // (border-box minus padding and border), so we only subtract our margins.
    static PARENT_CONTENT: Formula =
        Formula::Related(SingleRelationship::Parent, content_size_query_horizontal);
    static ML: Formula = Formula::CssValueOrDefault(PropertyId::MarginLeft, 0);
    static MR: Formula = Formula::CssValueOrDefault(PropertyId::MarginRight, 0);

    static S1: Formula = Formula::Op(Operation::Sub, &PARENT_CONTENT, &ML);
    static RESULT: Formula = Formula::Op(Operation::Sub, &S1, &MR);

    &RESULT
}

fn block_height() -> &'static Formula {
    // Block auto height = sum of children heights + own padding + own border
    static CHILDREN: FormulaList =
        FormulaList::Related(MultiRelationship::Children, size_query_vertical);
    static CHILDREN_SUM: Formula = Formula::Aggregate(Aggregation::Sum, &CHILDREN);

    static PT: Formula = Formula::CssValueOrDefault(PropertyId::PaddingTop, 0);
    static PB: Formula = Formula::CssValueOrDefault(PropertyId::PaddingBottom, 0);
    static BT: Formula = Formula::CssValueOrDefault(PropertyId::BorderTopWidth, 0);
    static BB: Formula = Formula::CssValueOrDefault(PropertyId::BorderBottomWidth, 0);

    static S1: Formula = Formula::Op(Operation::Add, &CHILDREN_SUM, &PT);
    static S2: Formula = Formula::Op(Operation::Add, &S1, &PB);
    static S3: Formula = Formula::Op(Operation::Add, &S2, &BT);
    static RESULT: Formula = Formula::Op(Operation::Add, &S3, &BB);

    &RESULT
}

// ============================================================================
// Offset formulas
// ============================================================================

/// Compute block offset formula.
pub fn block_offset(axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => block_offset_x(),
        Axis::Vertical => block_offset_y(),
    }
}

fn block_offset_x() -> &'static Formula {
    // X = own margin-left (parent padding/border is already in the
    // coordinate space established by the parent's content box)
    static ML: Formula = Formula::CssValueOrDefault(PropertyId::MarginLeft, 0);
    &ML
}

fn block_offset_y() -> &'static Formula {
    // Y = own margin-top + sum of (prev sibling height + prev sibling margin-top
    //     + prev sibling margin-bottom)
    // For now, simplified: sum of previous siblings' heights + own margin-top.
    // (Siblings' margins are already part of their offset, not their size,
    //  so we need to add margins explicitly for stacking.)
    static PREV_SIBLINGS: FormulaList =
        FormulaList::Related(MultiRelationship::PrevSiblings, size_query_vertical);
    static PREV_SUM: Formula = Formula::Aggregate(Aggregation::Sum, &PREV_SIBLINGS);
    static MT: Formula = Formula::CssValueOrDefault(PropertyId::MarginTop, 0);
    static RESULT: Formula = Formula::Op(Operation::Add, &PREV_SUM, &MT);
    &RESULT
}

// ============================================================================
// Intrinsic size formulas
// ============================================================================

/// Min-content size for block elements.
pub fn block_min_content_size(axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => {
            static CHILDREN: FormulaList =
                FormulaList::Related(MultiRelationship::Children, size_query_horizontal);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Max, &CHILDREN);
            &RESULT
        }
        Axis::Vertical => {
            static CHILDREN: FormulaList =
                FormulaList::Related(MultiRelationship::Children, size_query_vertical);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &CHILDREN);
            &RESULT
        }
    }
}

/// Max-content size for block elements.
pub fn block_max_content_size(axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => {
            static CHILDREN: FormulaList =
                FormulaList::Related(MultiRelationship::Children, size_query_horizontal);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Max, &CHILDREN);
            &RESULT
        }
        Axis::Vertical => {
            static CHILDREN: FormulaList =
                FormulaList::Related(MultiRelationship::Children, size_query_vertical);
            static RESULT: Formula = Formula::Aggregate(Aggregation::Sum, &CHILDREN);
            &RESULT
        }
    }
}
