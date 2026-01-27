//! Flexbox layout implementation using the formula system.

use rewrite_core::*;
use rewrite_layout_size_impl::SizeFormulaProvider;

/// Compute flex container size formula.
pub fn flex_size<P: SizeFormulaProvider>(
    scoped: &mut ScopedDb,
    axis: Axis,
    provider: &P,
) -> &'static Formula {
    let flex_direction = scoped.css(SingleRelationship::Self_, CssLayoutProperty::FlexDirection);

    // Determine main axis based on flex-direction
    let is_main_axis = match (flex_direction, axis) {
        (Keyword::Direction(Axis::Horizontal), Axis::Horizontal) => true,
        (Keyword::Direction(Axis::Vertical), Axis::Vertical) => true,
        (Keyword::Reverse(Axis::Horizontal), Axis::Horizontal) => true,
        (Keyword::Reverse(Axis::Vertical), Axis::Vertical) => true,
        _ => false,
    };

    if is_main_axis {
        flex_main_axis_size(scoped, axis, provider)
    } else {
        flex_cross_axis_size(scoped, axis, provider)
    }
}

fn flex_main_axis_size<P: SizeFormulaProvider>(
    _scoped: &mut ScopedDb,
    axis: Axis,
    _provider: &P,
) -> &'static Formula {
    // Main axis size is sum of flex items
    match axis {
        Axis::Horizontal => {
            static CHILDREN: FormulaList = FormulaList::RelatedValue(
                MultiRelationship::Children,
                CssValueProperty::Size(Axis::Horizontal),
            );
            static RESULT: Formula = Formula::List(CHILDREN);
            &RESULT
        }
        Axis::Vertical => {
            static CHILDREN: FormulaList = FormulaList::RelatedValue(
                MultiRelationship::Children,
                CssValueProperty::Size(Axis::Vertical),
            );
            static RESULT: Formula = Formula::List(CHILDREN);
            &RESULT
        }
    }
}

fn flex_cross_axis_size<P: SizeFormulaProvider>(
    scoped: &mut ScopedDb,
    axis: Axis,
    _provider: &P,
) -> &'static Formula {
    let align_items = scoped.css(SingleRelationship::Self_, CssLayoutProperty::AlignItems);

    match align_items {
        Keyword::Stretch => {
            // Stretch to fill parent's cross axis
            match axis {
                Axis::Horizontal => {
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
                    static STEP1: Formula = Formula::Sub(&PARENT_WIDTH, &PARENT_PADDING_LEFT);
                    static RESULT: Formula = Formula::Sub(&STEP1, &PARENT_PADDING_RIGHT);
                    &RESULT
                }
                Axis::Vertical => {
                    static PARENT_HEIGHT: Formula = Formula::RelatedValue(
                        SingleRelationship::Parent,
                        CssValueProperty::Size(Axis::Vertical),
                    );
                    static PARENT_PADDING_TOP: Formula = Formula::RelatedValue(
                        SingleRelationship::Parent,
                        CssValueProperty::Padding(Edge::Top),
                    );
                    static PARENT_PADDING_BOTTOM: Formula = Formula::RelatedValue(
                        SingleRelationship::Parent,
                        CssValueProperty::Padding(Edge::Bottom),
                    );
                    static STEP1: Formula = Formula::Sub(&PARENT_HEIGHT, &PARENT_PADDING_TOP);
                    static RESULT: Formula = Formula::Sub(&STEP1, &PARENT_PADDING_BOTTOM);
                    &RESULT
                }
            }
        }
        _ => {
            // Content-based cross axis size
            match axis {
                Axis::Horizontal => {
                    static CHILDREN: FormulaList = FormulaList::RelatedValue(
                        MultiRelationship::Children,
                        CssValueProperty::Size(Axis::Horizontal),
                    );
                    static RESULT: Formula = Formula::List(CHILDREN);
                    &RESULT
                }
                Axis::Vertical => {
                    static CHILDREN: FormulaList = FormulaList::RelatedValue(
                        MultiRelationship::Children,
                        CssValueProperty::Size(Axis::Vertical),
                    );
                    static RESULT: Formula = Formula::List(CHILDREN);
                    &RESULT
                }
            }
        }
    }
}

/// Compute flex item offset formula.
pub fn flex_offset<P: SizeFormulaProvider>(
    scoped: &mut ScopedDb,
    axis: Axis,
    _provider: &P,
) -> &'static Formula {
    let flex_direction = scoped.css(SingleRelationship::Self_, CssLayoutProperty::FlexDirection);

    let is_main_axis = match (flex_direction, axis) {
        (Keyword::Direction(Axis::Horizontal), Axis::Horizontal) => true,
        (Keyword::Direction(Axis::Vertical), Axis::Vertical) => true,
        (Keyword::Reverse(Axis::Horizontal), Axis::Horizontal) => true,
        (Keyword::Reverse(Axis::Vertical), Axis::Vertical) => true,
        _ => false,
    };

    if is_main_axis {
        flex_main_axis_offset(scoped, axis)
    } else {
        flex_cross_axis_offset(scoped, axis)
    }
}

fn flex_main_axis_offset(_scoped: &mut ScopedDb, axis: Axis) -> &'static Formula {
    // Main axis offset is sum of previous siblings on main axis
    match axis {
        Axis::Horizontal => {
            static PREV_SIBLINGS: FormulaList = FormulaList::RelatedValue(
                MultiRelationship::PrevSiblings,
                CssValueProperty::Size(Axis::Horizontal),
            );
            static RESULT: Formula = Formula::List(PREV_SIBLINGS);
            &RESULT
        }
        Axis::Vertical => {
            static PREV_SIBLINGS: FormulaList = FormulaList::RelatedValue(
                MultiRelationship::PrevSiblings,
                CssValueProperty::Size(Axis::Vertical),
            );
            static RESULT: Formula = Formula::List(PREV_SIBLINGS);
            &RESULT
        }
    }
}

fn flex_cross_axis_offset(scoped: &mut ScopedDb, axis: Axis) -> &'static Formula {
    let align_items = scoped.css(SingleRelationship::Self_, CssLayoutProperty::AlignItems);

    match align_items {
        Keyword::FlexStart | Keyword::SelfStart | Keyword::Stretch => {
            // Align to start of cross axis
            static RESULT: Formula = Formula::Constant(0);
            &RESULT
        }
        Keyword::FlexEnd | Keyword::SelfEnd => {
            // Align to end of cross axis (parent size - item size)
            match axis {
                Axis::Horizontal => {
                    static PARENT_WIDTH: Formula = Formula::RelatedValue(
                        SingleRelationship::Parent,
                        CssValueProperty::Size(Axis::Horizontal),
                    );
                    static SELF_WIDTH: Formula =
                        Formula::Value(CssValueProperty::Size(Axis::Horizontal));
                    static RESULT: Formula = Formula::Sub(&PARENT_WIDTH, &SELF_WIDTH);
                    &RESULT
                }
                Axis::Vertical => {
                    static PARENT_HEIGHT: Formula = Formula::RelatedValue(
                        SingleRelationship::Parent,
                        CssValueProperty::Size(Axis::Vertical),
                    );
                    static SELF_HEIGHT: Formula =
                        Formula::Value(CssValueProperty::Size(Axis::Vertical));
                    static RESULT: Formula = Formula::Sub(&PARENT_HEIGHT, &SELF_HEIGHT);
                    &RESULT
                }
            }
        }
        Keyword::Center => {
            // Center on cross axis
            match axis {
                Axis::Horizontal => {
                    static PARENT_WIDTH: Formula = Formula::RelatedValue(
                        SingleRelationship::Parent,
                        CssValueProperty::Size(Axis::Horizontal),
                    );
                    static SELF_WIDTH: Formula =
                        Formula::Value(CssValueProperty::Size(Axis::Horizontal));
                    static DIFF: Formula = Formula::Sub(&PARENT_WIDTH, &SELF_WIDTH);
                    static TWO: Formula = Formula::Constant(2);
                    static RESULT: Formula = Formula::Div(&DIFF, &TWO);
                    &RESULT
                }
                Axis::Vertical => {
                    static PARENT_HEIGHT: Formula = Formula::RelatedValue(
                        SingleRelationship::Parent,
                        CssValueProperty::Size(Axis::Vertical),
                    );
                    static SELF_HEIGHT: Formula =
                        Formula::Value(CssValueProperty::Size(Axis::Vertical));
                    static DIFF: Formula = Formula::Sub(&PARENT_HEIGHT, &SELF_HEIGHT);
                    static TWO: Formula = Formula::Constant(2);
                    static RESULT: Formula = Formula::Div(&DIFF, &TWO);
                    &RESULT
                }
            }
        }
        Keyword::Baseline => {
            // Baseline alignment - simplified to flex-start for now
            static RESULT: Formula = Formula::Constant(0);
            &RESULT
        }
        _ => {
            static RESULT: Formula = Formula::Constant(0);
            &RESULT
        }
    }
}
