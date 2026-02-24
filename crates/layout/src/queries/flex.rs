//! Flexbox layout formulas (CSS Flexbox Level 1).
//!
//! Implements the flex layout algorithm for sizing flex containers and
//! positioning flex items. The algorithm follows CSS Flexible Box Layout
//! Module Level 1 (§9).
//!
//! # Architecture
//!
//! Flex container sizing and item offset queries use the formula system:
//! - Container size: sum of children's bases + gaps (main axis), max of cross sizes
//! - Item offset: sum of previous siblings' sizes + gaps + justify-content spacing
//! - Item size: flex-basis + distributed free space via grow/shrink
//!
//! # Supported features
//!
//! - flex-grow, flex-shrink, flex-basis
//! - flex-direction (row, column, row-reverse, column-reverse)
//! - flex-wrap (wrap, nowrap, wrap-reverse)
//! - justify-content (flex-start, flex-end, center, space-between, space-around, space-evenly)
//! - align-items, align-self (flex-start, flex-end, center, stretch)
//! - gap (row-gap, column-gap)

use lightningcss::properties::flex::{FlexDirection, FlexWrap};
use lightningcss::properties::{Property, PropertyId};
use lightningcss::values::length::LengthPercentageOrAuto;
use lightningcss::vendor_prefix::VendorPrefix;
use rewrite_core::{
    Axis, Formula, MultiRelationship, NodeId, QueryFn, SingleRelationship, StylerAccess, Subpixel,
};

use super::size::size_query;

// ============================================================================
// Static formulas for flex CSS properties
// ============================================================================

/// flex-basis formula (resolves length/percentage, None if auto).
static FLEX_BASIS: Formula = Formula::CssValue(PropertyId::FlexBasis(VendorPrefix::None));

// ============================================================================
// Internal formula macros
//
// These must be macros (not functions) because they are used inside other
// macros that create `static` items — statics cannot call non-const functions.
// Each invocation produces its own distinct `static Formula`.
// ============================================================================

/// Justify-content free space for horizontal row containers.
/// free = parent_content - sum(children's resolved sizes) - total_gaps
macro_rules! justify_free_h_row {
    () => {
        sub!(
            sub!(
                related!(Parent, size_query, Axis::Horizontal),
                related_val!(Parent, css_prop!(PaddingLeft)),
                related_val!(Parent, css_prop!(PaddingRight)),
                related_val!(Parent, css_prop!(BorderLeftWidth)),
                related_val!(Parent, css_prop!(BorderRightWidth))
            ),
            related_val!(
                Parent,
                aggregate!(Sum, OrderedChildren, flex_item_main_query, Axis::Horizontal)
            ),
            related_val!(
                Parent,
                mul!(
                    css_prop!(ColumnGap),
                    max!(
                        sub!(
                            aggregate!(Count, OrderedChildren, always_query),
                            constant!(Subpixel::raw(1))
                        ),
                        constant!(Subpixel::ZERO)
                    )
                )
            )
        )
    };
}

/// Justify-content free space for vertical column containers.
macro_rules! justify_free_v_col {
    () => {
        sub!(
            sub!(
                related!(Parent, size_query, Axis::Vertical),
                related_val!(Parent, css_prop!(PaddingTop)),
                related_val!(Parent, css_prop!(PaddingBottom)),
                related_val!(Parent, css_prop!(BorderTopWidth)),
                related_val!(Parent, css_prop!(BorderBottomWidth))
            ),
            related_val!(
                Parent,
                aggregate!(Sum, OrderedChildren, flex_item_main_query, Axis::Vertical)
            ),
            related_val!(
                Parent,
                mul!(
                    css_prop!(RowGap),
                    max!(
                        sub!(
                            aggregate!(Count, OrderedChildren, always_query),
                            constant!(Subpixel::raw(1))
                        ),
                        constant!(Subpixel::ZERO)
                    )
                )
            )
        )
    };
}

// ============================================================================
// Public API — called from DisplayType::size() and DisplayType::offset()
// ============================================================================

/// Size formula for a flex container (when no explicit CSS size is set).
///
/// Main axis: sum of all flex items' hypothetical main sizes + gaps.
/// Cross axis: max of all flex items' cross sizes (nowrap) or sum of
/// per-line max cross sizes (wrap).
pub fn flex_size(
    flex_direction: FlexDirection,
    axis: Axis,
    styler: &dyn StylerAccess,
) -> &'static Formula {
    let wrap = flex_wrap_of(styler);

    if is_main_axis(axis, flex_direction) {
        flex_container_main_size(flex_direction, axis)
    } else if is_wrapping(wrap) {
        // Wrap: cross size = sum of per-line max cross sizes + cross gaps.
        flex_container_cross_size_wrap(flex_direction, axis)
    } else {
        // Nowrap: cross size = max of all children's cross sizes.
        aggregate!(Max, OrderedChildren, flex_item_cross_query, axis)
    }
}

/// Min-content size formula for a flex container (CSS Sizing §4 + Flexbox §9.9).
///
/// Main axis:
///   - nowrap: sum of items' min-content contributions (single line).
///   - wrap: max of items' min-content contributions (each on its own line).
/// Cross axis: same as auto cross size.
pub fn flex_min_content_size(
    flex_direction: FlexDirection,
    axis: Axis,
    styler: &dyn StylerAccess,
) -> &'static Formula {
    let wrap = flex_wrap_of(styler);
    if is_main_axis(axis, flex_direction) {
        if is_wrapping(wrap) {
            // Wrap: each item can go on its own line → largest item's
            // min-content contribution.
            aggregate!(Max, OrderedChildren, flex_item_min_content_main_query, axis)
        } else {
            // Nowrap: all items on one line → sum of flex base sizes.
            // Uses the same per-item basis query as max-content, because on
            // a single line the container can't be narrower than the sum of
            // all items' hypothetical main sizes.
            flex_container_main_size(flex_direction, axis)
        }
    } else if is_wrapping(wrap) {
        flex_container_cross_size_wrap(flex_direction, axis)
    } else {
        aggregate!(Max, OrderedChildren, flex_item_cross_query, axis)
    }
}

/// Per-item min-content contribution for the main axis.
/// Returns the item's min-content size plus its padding and border.
fn flex_item_min_content_main_query(
    styler: &dyn StylerAccess,
    axis: Axis,
) -> Option<&'static Formula> {
    if is_flex_excluded(styler) {
        return None;
    }
    Some(match axis {
        Axis::Horizontal => add!(
            min_content_width!(),
            css_prop!(PaddingLeft),
            css_prop!(PaddingRight),
            css_prop!(BorderLeftWidth),
            css_prop!(BorderRightWidth),
        ),
        Axis::Vertical => add!(
            min_content_height!(),
            css_prop!(PaddingTop),
            css_prop!(PaddingBottom),
            css_prop!(BorderTopWidth),
            css_prop!(BorderBottomWidth),
        ),
    })
}

/// Offset formula for a flex item within its container.
pub fn flex_offset(parent_direction: FlexDirection, axis: Axis) -> &'static Formula {
    // Dispatch through a query so we can read flex-wrap at runtime.
    if is_main_axis(axis, parent_direction) {
        flex_main_offset(parent_direction, axis)
    } else {
        // Cross offset: dispatched through query to handle wrap.
        flex_cross_offset(parent_direction, axis)
    }
}

/// Size formula for a flex item (called from size_query when parent is flex).
///
/// Main axis: flex-basis + distributed free space via grow/shrink.
/// Cross axis: the item's natural size (content or explicit CSS), or stretch.
pub fn flex_item_size(parent_direction: FlexDirection, axis: Axis) -> &'static Formula {
    if is_main_axis(axis, parent_direction) {
        // Main axis: dispatched through query to handle wrap at runtime.
        related!(Self_, flex_item_main_dispatch_query, axis)
    } else {
        // Cross axis: defer to query function which handles stretch/explicit/content.
        related!(Self_, flex_item_cross_query, axis)
    }
}

/// Query that dispatches to the imperative §9.7 resolver for main-axis sizing.
fn flex_item_main_dispatch_query(
    styler: &dyn StylerAccess,
    axis: Axis,
) -> Option<&'static Formula> {
    let wrap = parent_flex_wrap(styler);
    let parent = styler.related(SingleRelationship::Parent);
    let parent_display = super::DisplayType::of_element(parent.as_ref());
    let direction = match parent_display {
        Some(super::DisplayType::Flex(dir)) => dir,
        _ => return None,
    };

    Some(flex_item_main_imperative(
        direction,
        axis,
        is_wrapping(wrap),
    ))
}

/// Returns the imperative formula for flex item main-axis sizing.
///
/// Dispatches to one of 4 static imperative formulas based on
/// direction (row/column) and wrap (nowrap/wrap).
fn flex_item_main_imperative(
    direction: FlexDirection,
    axis: Axis,
    wrapping: bool,
) -> &'static Formula {
    match (direction, axis, wrapping) {
        (FlexDirection::Row | FlexDirection::RowReverse, Axis::Horizontal, false) => {
            imperative!(flex_resolve_main_row)
        }
        (FlexDirection::Row | FlexDirection::RowReverse, Axis::Horizontal, true) => {
            imperative!(flex_resolve_main_row_wrap)
        }
        (FlexDirection::Column | FlexDirection::ColumnReverse, Axis::Vertical, false) => {
            imperative!(flex_resolve_main_col)
        }
        (FlexDirection::Column | FlexDirection::ColumnReverse, Axis::Vertical, true) => {
            imperative!(flex_resolve_main_col_wrap)
        }
        _ => unreachable!("flex_item_main_imperative called on cross axis"),
    }
}

// ============================================================================
// Axis helpers
// ============================================================================

fn is_main_axis(axis: Axis, direction: FlexDirection) -> bool {
    match direction {
        FlexDirection::Row | FlexDirection::RowReverse => axis == Axis::Horizontal,
        FlexDirection::Column | FlexDirection::ColumnReverse => axis == Axis::Vertical,
    }
}

fn is_reversed(direction: FlexDirection) -> bool {
    matches!(
        direction,
        FlexDirection::RowReverse | FlexDirection::ColumnReverse
    )
}

/// Read the flex-wrap value from the container's CSS.
fn flex_wrap_of(styler: &dyn StylerAccess) -> FlexWrap {
    match styler.get_css_property(&PropertyId::FlexWrap(VendorPrefix::None)) {
        Some(lightningcss::properties::Property::FlexWrap(wrap, _)) => wrap,
        _ => FlexWrap::NoWrap,
    }
}

/// Read the flex-wrap value from an item's parent container.
fn parent_flex_wrap(styler: &dyn StylerAccess) -> FlexWrap {
    let parent = styler.related(SingleRelationship::Parent);
    flex_wrap_of(parent.as_ref())
}

/// Whether wrapping is enabled (wrap or wrap-reverse).
fn is_wrapping(wrap: FlexWrap) -> bool {
    matches!(wrap, FlexWrap::Wrap | FlexWrap::WrapReverse)
}

// ============================================================================
// Line-breaking parameters for flex-wrap
//
// These statics define the available main-axis space and gap formulas
// used by LineAggregate and LineItemAggregate for flex-wrap line breaking.
// ============================================================================

/// Available main-axis content width for row containers (for line breaking).
/// = self.width - padding - border
static ROW_AVAILABLE_MAIN: Formula = Formula::BinOp(
    rewrite_core::Operation::Sub,
    &Formula::BinOp(
        rewrite_core::Operation::Sub,
        &Formula::BinOp(
            rewrite_core::Operation::Sub,
            &Formula::BinOp(
                rewrite_core::Operation::Sub,
                &Formula::Related(rewrite_core::SingleRelationship::Self_, {
                    fn wrap(sty: &dyn StylerAccess) -> Option<&'static Formula> {
                        size_query(sty, Axis::Horizontal)
                    }
                    wrap as QueryFn
                }),
                &Formula::CssValueOrDefault(PropertyId::PaddingLeft, Subpixel::ZERO),
            ),
            &Formula::CssValueOrDefault(PropertyId::PaddingRight, Subpixel::ZERO),
        ),
        &Formula::CssValueOrDefault(PropertyId::BorderLeftWidth, Subpixel::ZERO),
    ),
    &Formula::CssValueOrDefault(PropertyId::BorderRightWidth, Subpixel::ZERO),
);

/// Available main-axis content height for column containers (for line breaking).
static COL_AVAILABLE_MAIN: Formula = Formula::BinOp(
    rewrite_core::Operation::Sub,
    &Formula::BinOp(
        rewrite_core::Operation::Sub,
        &Formula::BinOp(
            rewrite_core::Operation::Sub,
            &Formula::BinOp(
                rewrite_core::Operation::Sub,
                &Formula::Related(rewrite_core::SingleRelationship::Self_, {
                    fn wrap(sty: &dyn StylerAccess) -> Option<&'static Formula> {
                        size_query(sty, Axis::Vertical)
                    }
                    wrap as QueryFn
                }),
                &Formula::CssValueOrDefault(PropertyId::PaddingTop, Subpixel::ZERO),
            ),
            &Formula::CssValueOrDefault(PropertyId::PaddingBottom, Subpixel::ZERO),
        ),
        &Formula::CssValueOrDefault(PropertyId::BorderTopWidth, Subpixel::ZERO),
    ),
    &Formula::CssValueOrDefault(PropertyId::BorderBottomWidth, Subpixel::ZERO),
);

/// Column gap formula (for row containers' line-breaking gap parameter).
static COLUMN_GAP: Formula = Formula::CssValueOrDefault(PropertyId::ColumnGap, Subpixel::ZERO);

/// Row gap formula (for column containers' line-breaking gap parameter).
static ROW_GAP: Formula = Formula::CssValueOrDefault(PropertyId::RowGap, Subpixel::ZERO);

// Line-breaking parameter accessors for row/column directions.
//
// These are expression-macros because their results are used inside other
// macros that create `static` items. Using `let` bindings to destructure
// is not possible because `let` bindings are never const in Rust.

/// Available main-axis space for line breaking.
macro_rules! lbp_available_main {
    (Row) => {
        &ROW_AVAILABLE_MAIN
    };
    (Column) => {
        &COL_AVAILABLE_MAIN
    };
}

/// Gap between items along the main axis (for line breaking).
macro_rules! lbp_main_gap {
    (Row) => {
        &COLUMN_GAP
    }; // row main-axis gap = column-gap
    (Column) => {
        &ROW_GAP
    }; // column main-axis gap = row-gap
}

/// Gap between lines along the cross axis.
macro_rules! lbp_cross_gap {
    (Row) => {
        &ROW_GAP
    }; // row cross-axis gap = row-gap
    (Column) => {
        &COLUMN_GAP
    }; // column cross-axis gap = column-gap
}

/// Query function returning each item's main-axis size (for line breaking).
///
/// Uses `flex_item_basis_query_for_lines` instead of `flex_item_basis_query`
/// so that whitespace text nodes return `Some(ZERO)` rather than `None`.
/// Returning `None` from the line-breaking query would force a line break
/// (that behaviour is for block children in inline formatting contexts).
macro_rules! lbp_item_main_size {
    (Row) => {{
        fn q(sty: &dyn StylerAccess) -> Option<&'static Formula> {
            flex_item_basis_query_for_lines(sty, Axis::Horizontal)
        }
        q as QueryFn
    }};
    (Column) => {{
        fn q(sty: &dyn StylerAccess) -> Option<&'static Formula> {
            flex_item_basis_query_for_lines(sty, Axis::Vertical)
        }
        q as QueryFn
    }};
}

/// Query function returning each item's cross-axis size.
macro_rules! lbp_cross_query {
    (Row) => {{
        fn q(sty: &dyn StylerAccess) -> Option<&'static Formula> {
            flex_item_cross_query(sty, Axis::Vertical)
        }
        q as QueryFn
    }};
    (Column) => {{
        fn q(sty: &dyn StylerAccess) -> Option<&'static Formula> {
            flex_item_cross_query(sty, Axis::Horizontal)
        }
        q as QueryFn
    }};
}

/// Query function returning each item's baseline offset (for baseline alignment).
macro_rules! lbp_baseline_query {
    (Row) => {{
        fn q(sty: &dyn StylerAccess) -> Option<&'static Formula> {
            flex_item_baseline_query(sty, Axis::Vertical)
        }
        q as QueryFn
    }};
    (Column) => {{
        fn q(sty: &dyn StylerAccess) -> Option<&'static Formula> {
            flex_item_baseline_query(sty, Axis::Horizontal)
        }
        q as QueryFn
    }};
}

// ============================================================================
// Reading flex CSS properties (for query-time dispatch)
// ============================================================================

/// Check if the item has an explicit `flex-basis` (not `auto`).
fn has_explicit_flex_basis(styler: &dyn StylerAccess) -> bool {
    use lightningcss::values::length::LengthPercentageOrAuto;
    match styler.get_css_property(&PropertyId::FlexBasis(VendorPrefix::None)) {
        Some(lightningcss::properties::Property::FlexBasis(basis, _)) => {
            !matches!(basis, LengthPercentageOrAuto::Auto)
        }
        _ => false,
    }
}

// ============================================================================
// Auto margin helpers
// ============================================================================

/// Check if a margin property is `auto`.
fn is_margin_auto(styler: &dyn StylerAccess, prop_id: &PropertyId<'static>) -> bool {
    match styler.get_css_property(prop_id) {
        Some(lightningcss::properties::Property::MarginLeft(LengthPercentageOrAuto::Auto))
        | Some(lightningcss::properties::Property::MarginRight(LengthPercentageOrAuto::Auto))
        | Some(lightningcss::properties::Property::MarginTop(LengthPercentageOrAuto::Auto))
        | Some(lightningcss::properties::Property::MarginBottom(LengthPercentageOrAuto::Auto)) => {
            true
        }
        _ => false,
    }
}

// ============================================================================
// Main-axis auto margin queries
// ============================================================================

/// Query: returns the number of auto margins on the main axis for this item.
/// Used in aggregate(Sum, OrderedChildren, ...) to get total auto margin count.
///
/// Row direction: checks margin-left and margin-right.
fn flex_auto_margin_count_row_query(styler: &dyn StylerAccess) -> Option<&'static Formula> {
    if is_flex_excluded(styler) {
        return None;
    }
    let ml = is_margin_auto(styler, &PropertyId::MarginLeft);
    let mr = is_margin_auto(styler, &PropertyId::MarginRight);
    match (ml, mr) {
        (true, true) => Some(constant!(Subpixel::raw(2))),
        (true, false) | (false, true) => Some(constant!(Subpixel::raw(1))),
        (false, false) => Some(constant!(Subpixel::ZERO)),
    }
}

/// Column direction: checks margin-top and margin-bottom.
fn flex_auto_margin_count_col_query(styler: &dyn StylerAccess) -> Option<&'static Formula> {
    if is_flex_excluded(styler) {
        return None;
    }
    let mt = is_margin_auto(styler, &PropertyId::MarginTop);
    let mb = is_margin_auto(styler, &PropertyId::MarginBottom);
    match (mt, mb) {
        (true, true) => Some(constant!(Subpixel::raw(2))),
        (true, false) | (false, true) => Some(constant!(Subpixel::raw(1))),
        (false, false) => Some(constant!(Subpixel::ZERO)),
    }
}

/// Query: returns the total auto margin space consumed by this item on the main axis.
/// = auto_count * (max(0, free_space) / max(1, total_auto_count))
///
/// Row direction (horizontal main axis).
fn flex_item_auto_margin_row_query(styler: &dyn StylerAccess) -> Option<&'static Formula> {
    if is_flex_excluded(styler) {
        return None;
    }
    let ml = is_margin_auto(styler, &PropertyId::MarginLeft);
    let mr = is_margin_auto(styler, &PropertyId::MarginRight);

    macro_rules! per_auto_row {
        () => {
            div!(
                max!(justify_free_h_row!(), constant!(Subpixel::ZERO)),
                max!(
                    related_val!(
                        Parent,
                        aggregate!(Sum, OrderedChildren, flex_auto_margin_count_row_query)
                    ),
                    constant!(Subpixel::raw(1))
                )
            )
        };
    }

    match (ml, mr) {
        (true, true) => Some(mul!(constant!(Subpixel::raw(2)), per_auto_row!())),
        (true, false) | (false, true) => Some(per_auto_row!()),
        (false, false) => Some(constant!(Subpixel::ZERO)),
    }
}

/// Column direction (vertical main axis).
fn flex_item_auto_margin_col_query(styler: &dyn StylerAccess) -> Option<&'static Formula> {
    if is_flex_excluded(styler) {
        return None;
    }
    let mt = is_margin_auto(styler, &PropertyId::MarginTop);
    let mb = is_margin_auto(styler, &PropertyId::MarginBottom);

    macro_rules! per_auto_col {
        () => {
            div!(
                max!(justify_free_v_col!(), constant!(Subpixel::ZERO)),
                max!(
                    related_val!(
                        Parent,
                        aggregate!(Sum, OrderedChildren, flex_auto_margin_count_col_query)
                    ),
                    constant!(Subpixel::raw(1))
                )
            )
        };
    }

    match (mt, mb) {
        (true, true) => Some(mul!(constant!(Subpixel::raw(2)), per_auto_col!())),
        (true, false) | (false, true) => Some(per_auto_col!()),
        (false, false) => Some(constant!(Subpixel::ZERO)),
    }
}

// ============================================================================
// Public flex auto margin value queries (used by property.rs)
// ============================================================================

/// Compute the used auto margin value for a flex item on the given side.
///
/// Returns `None` if the element is not a flex item or the margin is not auto.
/// Returns `Some(formula)` that computes the used auto margin px value.
pub fn flex_auto_margin_value(
    styler: &dyn StylerAccess,
    prop_id: &PropertyId<'static>,
) -> Option<&'static Formula> {
    // Check parent is flex
    let parent = styler.related(SingleRelationship::Parent);
    let parent_display = super::DisplayType::of_element(parent.as_ref())?;
    let direction = match parent_display {
        super::DisplayType::Flex(dir) => dir,
        _ => return None,
    };

    // Check the specific margin property is auto
    if !is_margin_auto(styler, prop_id) {
        return None;
    }

    // Determine if this is a main-axis or cross-axis margin
    let is_main_axis = match (prop_id, direction) {
        (
            PropertyId::MarginLeft | PropertyId::MarginRight,
            FlexDirection::Row | FlexDirection::RowReverse,
        ) => true,
        (
            PropertyId::MarginTop | PropertyId::MarginBottom,
            FlexDirection::Column | FlexDirection::ColumnReverse,
        ) => true,
        _ => false,
    };

    if is_main_axis {
        // Main-axis auto margin: free_space / total_auto_count
        let per_auto = match direction {
            FlexDirection::Row | FlexDirection::RowReverse => {
                div!(
                    max!(justify_free_h_row!(), constant!(Subpixel::ZERO)),
                    max!(
                        related_val!(
                            Parent,
                            aggregate!(Sum, OrderedChildren, flex_auto_margin_count_row_query)
                        ),
                        constant!(Subpixel::raw(1))
                    )
                )
            }
            FlexDirection::Column | FlexDirection::ColumnReverse => {
                div!(
                    max!(justify_free_v_col!(), constant!(Subpixel::ZERO)),
                    max!(
                        related_val!(
                            Parent,
                            aggregate!(Sum, OrderedChildren, flex_auto_margin_count_col_query)
                        ),
                        constant!(Subpixel::raw(1))
                    )
                )
            }
        };
        Some(per_auto)
    } else {
        // Cross-axis auto margin: (cross_content - item_cross) / auto_count
        // Must expand fully into match arms to avoid runtime values in static formulas.

        // Cross free space macros for each axis
        macro_rules! cross_free_v {
            () => {
                sub!(
                    sub!(
                        related!(Parent, size_query, Axis::Vertical),
                        related_val!(Parent, css_prop!(PaddingTop)),
                        related_val!(Parent, css_prop!(PaddingBottom)),
                        related_val!(Parent, css_prop!(BorderTopWidth)),
                        related_val!(Parent, css_prop!(BorderBottomWidth))
                    ),
                    related!(Self_, flex_item_cross_query, Axis::Vertical)
                )
            };
        }
        macro_rules! cross_free_h {
            () => {
                sub!(
                    sub!(
                        related!(Parent, size_query, Axis::Horizontal),
                        related_val!(Parent, css_prop!(PaddingLeft)),
                        related_val!(Parent, css_prop!(PaddingRight)),
                        related_val!(Parent, css_prop!(BorderLeftWidth)),
                        related_val!(Parent, css_prop!(BorderRightWidth))
                    ),
                    related!(Self_, flex_item_cross_query, Axis::Horizontal)
                )
            };
        }

        let is_start = matches!(prop_id, PropertyId::MarginTop | PropertyId::MarginLeft);
        let both_auto = match direction {
            FlexDirection::Row | FlexDirection::RowReverse => {
                // Cross is vertical
                is_margin_auto(styler, &PropertyId::MarginTop)
                    && is_margin_auto(styler, &PropertyId::MarginBottom)
            }
            FlexDirection::Column | FlexDirection::ColumnReverse => {
                // Cross is horizontal
                is_margin_auto(styler, &PropertyId::MarginLeft)
                    && is_margin_auto(styler, &PropertyId::MarginRight)
            }
        };

        match (both_auto, direction) {
            // Both auto: each gets half the cross free space
            (true, FlexDirection::Row | FlexDirection::RowReverse) => Some(max!(
                div!(cross_free_v!(), constant!(Subpixel::raw(2))),
                constant!(Subpixel::ZERO)
            )),
            (true, FlexDirection::Column | FlexDirection::ColumnReverse) => Some(max!(
                div!(cross_free_h!(), constant!(Subpixel::raw(2))),
                constant!(Subpixel::ZERO)
            )),
            // Single auto: gets all the cross free space
            (false, FlexDirection::Row | FlexDirection::RowReverse) => {
                if is_start {
                    // start-only auto: push to end → all free space
                    Some(max!(cross_free_v!(), constant!(Subpixel::ZERO)))
                } else {
                    // end-only auto: stays at start → all free space
                    Some(max!(cross_free_v!(), constant!(Subpixel::ZERO)))
                }
            }
            (false, FlexDirection::Column | FlexDirection::ColumnReverse) => {
                if is_start {
                    Some(max!(cross_free_h!(), constant!(Subpixel::ZERO)))
                } else {
                    Some(max!(cross_free_h!(), constant!(Subpixel::ZERO)))
                }
            }
        }
    }
}

// ============================================================================
// Alignment helpers
// ============================================================================

/// Simplified cross-alignment enum for formula dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CrossAlign {
    FlexStart,
    FlexEnd,
    Center,
    Stretch,
    Baseline,
}

/// Simplified justify-content enum for formula dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JustifyMode {
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    Stretch,
}

/// Simplified align-content enum for formula dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlignContentMode {
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    Stretch,
}

fn self_position_to_cross(pos: lightningcss::properties::align::SelfPosition) -> CrossAlign {
    use lightningcss::properties::align::SelfPosition;
    match pos {
        SelfPosition::Center => CrossAlign::Center,
        SelfPosition::End | SelfPosition::FlexEnd | SelfPosition::SelfEnd => CrossAlign::FlexEnd,
        SelfPosition::Start | SelfPosition::FlexStart | SelfPosition::SelfStart => {
            CrossAlign::FlexStart
        }
    }
}

/// Determine the effective cross-axis alignment for a flex item.
/// Checks align-self first; if auto/absent, falls back to parent's align-items.
fn effective_cross_alignment(styler: &dyn StylerAccess) -> CrossAlign {
    use lightningcss::properties::Property;
    use lightningcss::properties::align::*;

    if let Some(Property::AlignSelf(align_self, _)) =
        styler.get_css_property(&PropertyId::AlignSelf(VendorPrefix::None))
    {
        match align_self {
            AlignSelf::Auto | AlignSelf::Normal => {}
            AlignSelf::Stretch => return CrossAlign::Stretch,
            AlignSelf::BaselinePosition(_) => return CrossAlign::Baseline,
            AlignSelf::SelfPosition { value, .. } => return self_position_to_cross(value),
        }
    }

    let parent = styler.related(SingleRelationship::Parent);
    if let Some(Property::AlignItems(align_items, _)) =
        parent.get_css_property(&PropertyId::AlignItems(VendorPrefix::None))
    {
        match align_items {
            AlignItems::Normal | AlignItems::Stretch => return CrossAlign::Stretch,
            AlignItems::BaselinePosition(_) => return CrossAlign::Baseline,
            AlignItems::SelfPosition { value, .. } => return self_position_to_cross(value),
        }
    }

    CrossAlign::Stretch
}

/// Determine the justify-content mode from the parent's CSS.
fn justify_content_of(styler: &dyn StylerAccess) -> JustifyMode {
    use lightningcss::properties::Property;
    use lightningcss::properties::align::*;

    let parent = styler.related(SingleRelationship::Parent);
    if let Some(Property::JustifyContent(jc, _)) =
        parent.get_css_property(&PropertyId::JustifyContent(VendorPrefix::None))
    {
        match jc {
            JustifyContent::Normal => JustifyMode::FlexStart,
            JustifyContent::ContentDistribution(dist) => match dist {
                ContentDistribution::SpaceBetween => JustifyMode::SpaceBetween,
                ContentDistribution::SpaceAround => JustifyMode::SpaceAround,
                ContentDistribution::SpaceEvenly => JustifyMode::SpaceEvenly,
                ContentDistribution::Stretch => JustifyMode::Stretch,
            },
            JustifyContent::ContentPosition { value, .. } => match value {
                ContentPosition::Center => JustifyMode::Center,
                ContentPosition::End | ContentPosition::FlexEnd => JustifyMode::FlexEnd,
                ContentPosition::Start | ContentPosition::FlexStart => JustifyMode::FlexStart,
            },
            JustifyContent::Left { .. } => JustifyMode::FlexStart,
            JustifyContent::Right { .. } => JustifyMode::FlexEnd,
        }
    } else {
        JustifyMode::FlexStart
    }
}

/// Determine the effective align-content mode for a flex container.
/// Reads align-content from the parent container.
fn align_content_of_parent(styler: &dyn StylerAccess) -> AlignContentMode {
    use lightningcss::properties::Property;
    use lightningcss::properties::align::*;

    let parent = styler.related(SingleRelationship::Parent);
    if let Some(Property::AlignContent(ac, _)) =
        parent.get_css_property(&PropertyId::AlignContent(VendorPrefix::None))
    {
        match ac {
            AlignContent::Normal => AlignContentMode::Stretch,
            AlignContent::ContentDistribution(dist) => match dist {
                ContentDistribution::SpaceBetween => AlignContentMode::SpaceBetween,
                ContentDistribution::SpaceAround => AlignContentMode::SpaceAround,
                ContentDistribution::SpaceEvenly => AlignContentMode::SpaceEvenly,
                ContentDistribution::Stretch => AlignContentMode::Stretch,
            },
            AlignContent::ContentPosition { value, .. } => match value {
                ContentPosition::Center => AlignContentMode::Center,
                ContentPosition::End | ContentPosition::FlexEnd => AlignContentMode::FlexEnd,
                ContentPosition::Start | ContentPosition::FlexStart => AlignContentMode::FlexStart,
            },
            AlignContent::BaselinePosition(_) => AlignContentMode::FlexStart,
        }
    } else {
        // Default align-content is normal, which behaves as stretch for flex.
        AlignContentMode::Stretch
    }
}

// ============================================================================
// Query functions (used in Aggregate formulas)
// ============================================================================

/// CSS Flexbox §4: certain children of a flex container should be excluded
/// from flex item aggregation:
/// - Whitespace-only text runs (§4, collapsed anonymous items)
/// - Absolutely/fixed positioned children (§4.1, out-of-flow)
fn is_flex_excluded(styler: &dyn StylerAccess) -> bool {
    // Whitespace-only text nodes
    if styler.is_intrinsic() && styler.text_content().is_none() {
        return true;
    }
    // Absolutely or fixed positioned children are out-of-flow per §4.1
    matches!(
        styler.get_css_property(&PropertyId::Position),
        Some(lightningcss::properties::Property::Position(
            lightningcss::properties::position::Position::Absolute
                | lightningcss::properties::position::Position::Fixed
        ))
    )
}

/// Query: always returns Some (used for Count aggregation to count children).
/// Excludes whitespace-only text nodes per CSS Flexbox §4.
fn always_query(styler: &dyn StylerAccess) -> Option<&'static Formula> {
    if is_flex_excluded(styler) {
        return None;
    }
    Some(constant!(Subpixel::ZERO))
}

/// Query: returns the flex-basis of an item for aggregation.
fn flex_item_basis_query(styler: &dyn StylerAccess, axis: Axis) -> Option<&'static Formula> {
    if is_flex_excluded(styler) {
        return None;
    }
    if has_explicit_flex_basis(styler) {
        return Some(&FLEX_BASIS);
    }

    let size_prop = match axis {
        Axis::Horizontal => PropertyId::Width,
        Axis::Vertical => PropertyId::Height,
    };
    if styler.get_css_property(&size_prop).is_some() {
        return Some(match axis {
            Axis::Horizontal => css_val!(Width),
            Axis::Vertical => css_val!(Height),
        });
    }

    content_based_size(styler, axis)
}

/// Like `flex_item_basis_query` but returns `Some(ZERO)` for whitespace
/// text nodes instead of `None`. Used for line-breaking in
/// `compute_line_assignments` where `None` means "force a line break"
/// (intended for block children in inline formatting contexts, not for
/// excluded flex items).
fn flex_item_basis_query_for_lines(
    styler: &dyn StylerAccess,
    axis: Axis,
) -> Option<&'static Formula> {
    if is_flex_excluded(styler) {
        return Some(constant!(Subpixel::ZERO));
    }
    flex_item_basis_query(styler, axis)
}

/// Query: returns the item's resolved main size for offset aggregation.
fn flex_item_main_query(styler: &dyn StylerAccess, axis: Axis) -> Option<&'static Formula> {
    if is_flex_excluded(styler) {
        return None;
    }
    size_query(styler, axis)
}

/// Query: returns the item's cross size, handling stretch.
fn flex_item_cross_query(styler: &dyn StylerAccess, axis: Axis) -> Option<&'static Formula> {
    if is_flex_excluded(styler) {
        return None;
    }
    let size_prop = match axis {
        Axis::Horizontal => PropertyId::Width,
        Axis::Vertical => PropertyId::Height,
    };
    if styler.get_css_property(&size_prop).is_some() {
        return Some(match axis {
            Axis::Horizontal => css_val!(Width),
            Axis::Vertical => css_val!(Height),
        });
    }

    // Stretch: fill the container's cross content size.
    if effective_cross_alignment(styler) == CrossAlign::Stretch {
        return Some(match axis {
            Axis::Horizontal => sub!(
                related!(Parent, size_query, Axis::Horizontal),
                related_val!(Parent, css_prop!(PaddingLeft)),
                related_val!(Parent, css_prop!(PaddingRight)),
                related_val!(Parent, css_prop!(BorderLeftWidth)),
                related_val!(Parent, css_prop!(BorderRightWidth)),
            ),
            Axis::Vertical => sub!(
                related!(Parent, size_query, Axis::Vertical),
                related_val!(Parent, css_prop!(PaddingTop)),
                related_val!(Parent, css_prop!(PaddingBottom)),
                related_val!(Parent, css_prop!(BorderTopWidth)),
                related_val!(Parent, css_prop!(BorderBottomWidth)),
            ),
        });
    }

    content_based_size(styler, axis)
}

/// Baseline query: returns the item's baseline distance from its top
/// (cross-start) edge.
///
/// Per CSS Flexbox §9.4 / CSS Alignment §9.1:
/// - Text nodes: ascent (distance from top to first baseline)
/// - Elements with text content: first baseline from inline content
/// - Elements without any text descendants: synthesized baseline = cross size
///
/// Only returns a value for items that participate in baseline alignment
/// (i.e. `align-self: baseline`). Other items return `None` so they are
/// excluded from the max-baseline aggregation.
fn flex_item_baseline_query(styler: &dyn StylerAccess, axis: Axis) -> Option<&'static Formula> {
    if is_flex_excluded(styler) {
        return None;
    }
    // Only include items that participate in baseline alignment.
    if effective_cross_alignment(styler) != CrossAlign::Baseline {
        return None;
    }
    flex_item_baseline_value(styler, axis)
}

/// Returns the baseline value for a flex item regardless of its alignment.
/// Used both by `flex_item_baseline_query` (for baseline-aligned items)
/// and by the cross-offset formula (for `related!(Self_, ...)`).
fn flex_item_baseline_value(styler: &dyn StylerAccess, axis: Axis) -> Option<&'static Formula> {
    // Intrinsic (text) nodes: the baseline is the ascent.
    if styler.is_intrinsic() {
        return Some(inline_baseline!());
    }
    // Non-intrinsic element: check if it has any children.
    // If it does, the baseline is the first child's text baseline
    // plus padding and border on the cross-start edge.
    let children = styler.related_iter(MultiRelationship::Children);
    if !children.is_empty() {
        return Some(match axis {
            Axis::Vertical => add!(
                inline_baseline!(),
                css_prop!(PaddingTop),
                css_prop!(BorderTopWidth),
            ),
            Axis::Horizontal => add!(
                inline_baseline!(),
                css_prop!(PaddingLeft),
                css_prop!(BorderLeftWidth),
            ),
        });
    }
    // Empty element: synthesized baseline = item's cross size.
    flex_item_cross_query(styler, axis)
}

/// Max baseline query: resolved on the flex container (parent),
/// returns the maximum baseline across all ordered children.
fn max_baseline_query(_styler: &dyn StylerAccess, axis: Axis) -> Option<&'static Formula> {
    Some(aggregate!(
        Max,
        OrderedChildren,
        flex_item_baseline_query,
        axis
    ))
}

// ============================================================================
// Container main-axis size
// ============================================================================

/// Container auto main size = sum of children's bases + total gaps.
/// Total gaps = gap * max(0, count - 1).
fn flex_container_main_size(direction: FlexDirection, axis: Axis) -> &'static Formula {
    match (direction, axis) {
        (FlexDirection::Row | FlexDirection::RowReverse, Axis::Horizontal) => add!(
            aggregate!(
                Sum,
                OrderedChildren,
                flex_item_basis_query,
                Axis::Horizontal
            ),
            mul!(
                css_prop!(ColumnGap),
                max!(
                    sub!(
                        aggregate!(Count, OrderedChildren, always_query),
                        constant!(Subpixel::raw(1))
                    ),
                    constant!(Subpixel::ZERO)
                )
            )
        ),
        (FlexDirection::Column | FlexDirection::ColumnReverse, Axis::Vertical) => add!(
            aggregate!(Sum, OrderedChildren, flex_item_basis_query, Axis::Vertical),
            mul!(
                css_prop!(RowGap),
                max!(
                    sub!(
                        aggregate!(Count, OrderedChildren, always_query),
                        constant!(Subpixel::raw(1))
                    ),
                    constant!(Subpixel::ZERO)
                )
            )
        ),
        _ => unreachable!("flex_container_main_size called on cross axis"),
    }
}

// ============================================================================
// Container cross-axis size (flex-wrap)
// ============================================================================

/// Cross-axis auto size for a wrapping flex container.
/// = sum of per-line max cross sizes + cross gaps between lines.
fn flex_container_cross_size_wrap(direction: FlexDirection, axis: Axis) -> &'static Formula {
    match (direction, axis) {
        // Row direction → cross axis is vertical.
        (FlexDirection::Row | FlexDirection::RowReverse, Axis::Vertical) => {
            line_aggregate!(
                line_agg: Sum,
                within_line_agg: Max,
                item_main_size: lbp_item_main_size!(Row),
                item_value: lbp_cross_query!(Row),
                available_main: lbp_available_main!(Row),
                gap: lbp_main_gap!(Row),
                line_gap: lbp_cross_gap!(Row),
            )
        }
        // Column direction → cross axis is horizontal.
        (FlexDirection::Column | FlexDirection::ColumnReverse, Axis::Horizontal) => {
            line_aggregate!(
                line_agg: Sum,
                within_line_agg: Max,
                item_main_size: lbp_item_main_size!(Column),
                item_value: lbp_cross_query!(Column),
                available_main: lbp_available_main!(Column),
                gap: lbp_main_gap!(Column),
                line_gap: lbp_cross_gap!(Column),
            )
        }
        _ => unreachable!("flex_container_cross_size_wrap called on main axis"),
    }
}

// ============================================================================
// Flex item main-axis sizing — imperative §9.7 resolvers
// ============================================================================

/// Per-item data collected for the §9.7 algorithm.
struct FlexItemInfo {
    node_id: NodeId,
    basis: f32,
    grow: f32,
    shrink: f32,
    min_main: f32,
    max_main: f32,
    frozen: bool,
    target: f32,
}

/// Large sentinel for unconstrained max-width/max-height.
const MAX_MAIN_SENTINEL: f32 = 1_000_000.0;

/// Compute the automatic minimum size for a flex item per CSS Flexbox §4.5.
///
/// When `min-width` / `min-height` is `auto` (returns `None` from
/// `get_property`), the automatic minimum is:
/// - 0 if the item's `overflow` is not `visible`
/// - Otherwise: `min(content_size, flex_basis)` (clamped to any specified
///   max constraint)
///
/// The content size is the min-content size in the main axis.
fn auto_minimum_size(
    child: &dyn StylerAccess,
    main_axis: Axis,
    _basis: f32,
    max_main: f32,
    resolve: &mut dyn FnMut(&'static Formula, NodeId, &dyn StylerAccess) -> Option<Subpixel>,
) -> f32 {
    // §4.5: If the item's overflow is not visible in the main axis,
    // the automatic minimum size is 0.
    let overflow_prop = match main_axis {
        Axis::Horizontal => PropertyId::OverflowX,
        Axis::Vertical => PropertyId::OverflowY,
    };
    if let Some(Property::OverflowX(ov)) | Some(Property::OverflowY(ov)) =
        child.get_css_property(&overflow_prop)
    {
        use lightningcss::properties::overflow::OverflowKeyword;
        if ov != OverflowKeyword::Visible {
            return 0.0;
        }
    }

    // Compute min-content size in the main axis.
    // For intrinsic (text) nodes, use min-content measurement directly.
    // For element nodes, compute the min-content size of their children.
    let content_size = if child.is_intrinsic() {
        let content_formula = match main_axis {
            Axis::Horizontal => min_content_width!(),
            Axis::Vertical => min_content_height!(),
        };
        resolve(content_formula, child.node_id(), child)
            .unwrap_or(Subpixel::ZERO)
            .to_f32()
    } else if let Some(super::DisplayType::Flex(child_dir)) = super::DisplayType::of_element(child)
    {
        // Flex container child: use flex min-content formula.
        let content_formula = flex_min_content_size(child_dir, main_axis, child);
        resolve(content_formula, child.node_id(), child)
            .unwrap_or(Subpixel::ZERO)
            .to_f32()
    } else {
        // Element node: compute min-content from children.
        // Walk children and find the max min-content width (for the
        // horizontal axis) since each child is a potential line-breaker.
        let el_children = child.related_iter(MultiRelationship::Children);
        if el_children.is_empty() {
            0.0
        } else {
            let min_content_formula = match main_axis {
                Axis::Horizontal => min_content_width!(),
                Axis::Vertical => min_content_height!(),
            };
            let mut max_child_min: f32 = 0.0;
            for grandchild in &el_children {
                if let Some(val) = resolve(
                    min_content_formula,
                    grandchild.node_id(),
                    grandchild.as_ref(),
                ) {
                    max_child_min = max_child_min.max(val.to_f32());
                }
            }
            max_child_min
        }
    };

    // §4.5: The automatic minimum is:
    //   min(specified_size_suggestion, content_size_suggestion)
    // where specified_size_suggestion is the item's explicit width/height
    // (NOT the flex-basis). If no explicit size is set, the auto minimum
    // is just the content size.
    let size_prop = match main_axis {
        Axis::Horizontal => PropertyId::Width,
        Axis::Vertical => PropertyId::Height,
    };
    let specified_size = child.get_property(&size_prop);

    let min = match specified_size {
        Some(specified) => content_size.min(specified.to_f32()),
        None => content_size,
    };

    // Clamp to the specified max constraint if any.
    min.min(max_main)
}

/// Collect flex item info for §9.7 from a set of children.
///
/// `main_axis` determines which CSS properties to read (Width vs Height,
/// MinWidth vs MinHeight, MaxWidth vs MaxHeight).
///
/// Implements CSS Flexbox §4.5 for automatic minimum sizes when
/// `min-width`/`min-height` is `auto`.
fn collect_flex_items(
    children: &[Box<dyn StylerAccess>],
    main_axis: Axis,
    resolve: &mut dyn FnMut(&'static Formula, NodeId, &dyn StylerAccess) -> Option<Subpixel>,
) -> Vec<FlexItemInfo> {
    let (min_prop, max_prop) = match main_axis {
        Axis::Horizontal => (PropertyId::MinWidth, PropertyId::MaxWidth),
        Axis::Vertical => (PropertyId::MinHeight, PropertyId::MaxHeight),
    };

    let mut items = Vec::with_capacity(children.len());
    for child in children {
        if is_flex_excluded(child.as_ref()) {
            continue;
        }

        let basis_formula = flex_item_basis_query(child.as_ref(), main_axis);
        let basis = basis_formula
            .and_then(|f| resolve(f, child.node_id(), child.as_ref()))
            .unwrap_or(Subpixel::ZERO)
            .to_f32();

        let grow = child
            .get_property(&PropertyId::FlexGrow(VendorPrefix::None))
            .unwrap_or(Subpixel::ZERO)
            .to_f32();

        let shrink = child
            .get_property(&PropertyId::FlexShrink(VendorPrefix::None))
            .unwrap_or(Subpixel::from_f32(1.0))
            .to_f32();

        // Explicit max constraint (unset → effectively infinity).
        let max_main = child
            .get_property(&max_prop)
            .map_or(MAX_MAIN_SENTINEL, Subpixel::to_f32);

        // Min constraint: explicit value or §4.5 automatic minimum.
        let min_main = match child.get_property(&min_prop) {
            Some(explicit) => explicit.to_f32(),
            None => auto_minimum_size(child.as_ref(), main_axis, basis, max_main, resolve),
        };

        items.push(FlexItemInfo {
            node_id: child.node_id(),
            basis,
            grow,
            shrink,
            min_main,
            max_main,
            frozen: false,
            target: basis,
        });
    }
    items
}

/// Run the §9.7 freeze-and-redistribute algorithm on a set of items.
///
/// `container_main` is the available content-box main size.
/// `gap` is the gap between items.
fn resolve_flexible_lengths(items: &mut [FlexItemInfo], container_main: f32, gap: f32) {
    let total_gaps = gap * items.len().saturating_sub(1) as f32;

    for _ in 0..items.len() {
        // Compute free space: container - frozen targets - unfrozen bases - gaps.
        let frozen_sum: f32 = items.iter().filter(|i| i.frozen).map(|i| i.target).sum();
        let unfrozen_basis_sum: f32 = items.iter().filter(|i| !i.frozen).map(|i| i.basis).sum();
        let free = container_main - frozen_sum - unfrozen_basis_sum - total_gaps;

        let growing = free >= 0.0;

        let total_factor: f32 = items
            .iter()
            .filter(|i| !i.frozen)
            .map(|i| if growing { i.grow } else { i.shrink * i.basis })
            .sum();

        // If no flex factor among unfrozen items, freeze them all at basis.
        if total_factor <= 0.0 {
            for item in items.iter_mut().filter(|i| !i.frozen) {
                item.target = item.basis;
                item.frozen = true;
            }
            break;
        }

        // Distribute free space proportionally.
        for item in items.iter_mut().filter(|i| !i.frozen) {
            let ratio = if growing {
                item.grow
            } else {
                item.shrink * item.basis
            };
            item.target = item.basis + free * ratio / total_factor;
        }

        // Check for min/max violations and freeze.
        let mut any_violation = false;
        for item in items.iter_mut().filter(|i| !i.frozen) {
            if item.target < item.min_main {
                item.target = item.min_main;
                item.frozen = true;
                any_violation = true;
            } else if item.target > item.max_main {
                item.target = item.max_main;
                item.frozen = true;
                any_violation = true;
            }
        }

        if !any_violation {
            break;
        }
    }

    // Floor all targets at zero.
    for item in items.iter_mut() {
        if item.target < 0.0 {
            item.target = 0.0;
        }
    }
}

/// Compute the parent's content-box main size for a given axis.
fn resolve_container_content_main(
    parent: &dyn StylerAccess,
    axis: Axis,
    resolve: &mut dyn FnMut(&'static Formula, NodeId, &dyn StylerAccess) -> Option<Subpixel>,
) -> Option<f32> {
    let size_formula = size_query(parent, axis)?;
    let total = resolve(size_formula, parent.node_id(), parent)?.to_f32();

    let (pad_a, pad_b, bdr_a, bdr_b) = match axis {
        Axis::Horizontal => (
            PropertyId::PaddingLeft,
            PropertyId::PaddingRight,
            PropertyId::BorderLeftWidth,
            PropertyId::BorderRightWidth,
        ),
        Axis::Vertical => (
            PropertyId::PaddingTop,
            PropertyId::PaddingBottom,
            PropertyId::BorderTopWidth,
            PropertyId::BorderBottomWidth,
        ),
    };

    let pa = parent
        .get_property(&pad_a)
        .unwrap_or(Subpixel::ZERO)
        .to_f32();
    let pb = parent
        .get_property(&pad_b)
        .unwrap_or(Subpixel::ZERO)
        .to_f32();
    let ba = parent
        .get_property(&bdr_a)
        .unwrap_or(Subpixel::ZERO)
        .to_f32();
    let bb = parent
        .get_property(&bdr_b)
        .unwrap_or(Subpixel::ZERO)
        .to_f32();

    Some((total - pa - pb - ba - bb).max(0.0))
}

/// Get the gap value for the main axis.
fn resolve_main_gap(parent: &dyn StylerAccess, axis: Axis) -> f32 {
    let gap_prop = match axis {
        Axis::Horizontal => PropertyId::ColumnGap,
        Axis::Vertical => PropertyId::RowGap,
    };
    parent
        .get_property(&gap_prop)
        .unwrap_or(Subpixel::ZERO)
        .to_f32()
}

/// Build the batch result Vec from resolved items, snapping to Subpixel.
fn build_batch_result(items: &[FlexItemInfo]) -> Vec<(NodeId, Subpixel)> {
    items
        .iter()
        .map(|item| (item.node_id, Subpixel::from_f32(item.target)))
        .collect()
}

// --- Nowrap imperative resolvers ---

/// §9.7 resolver for row nowrap.
fn flex_resolve_main_row(
    _node: NodeId,
    styler: &dyn StylerAccess,
    resolve: &mut dyn FnMut(&'static Formula, NodeId, &dyn StylerAccess) -> Option<Subpixel>,
) -> Option<Vec<(NodeId, Subpixel)>> {
    flex_resolve_main_nowrap_impl(styler, Axis::Horizontal, resolve)
}

/// §9.7 resolver for column nowrap.
fn flex_resolve_main_col(
    _node: NodeId,
    styler: &dyn StylerAccess,
    resolve: &mut dyn FnMut(&'static Formula, NodeId, &dyn StylerAccess) -> Option<Subpixel>,
) -> Option<Vec<(NodeId, Subpixel)>> {
    flex_resolve_main_nowrap_impl(styler, Axis::Vertical, resolve)
}

/// Shared nowrap implementation for both row and column.
fn flex_resolve_main_nowrap_impl(
    styler: &dyn StylerAccess,
    axis: Axis,
    resolve: &mut dyn FnMut(&'static Formula, NodeId, &dyn StylerAccess) -> Option<Subpixel>,
) -> Option<Vec<(NodeId, Subpixel)>> {
    let parent = styler.related(SingleRelationship::Parent);
    let children = parent.related_iter(MultiRelationship::OrderedChildren);

    let mut items = collect_flex_items(&children, axis, resolve);
    if items.is_empty() {
        return Some(Vec::new());
    }

    let container_main = resolve_container_content_main(parent.as_ref(), axis, resolve)?;
    let gap = resolve_main_gap(parent.as_ref(), axis);

    resolve_flexible_lengths(&mut items, container_main, gap);
    Some(build_batch_result(&items))
}

// --- Wrap imperative resolvers ---

/// §9.7 resolver for row wrap.
fn flex_resolve_main_row_wrap(
    _node: NodeId,
    styler: &dyn StylerAccess,
    resolve: &mut dyn FnMut(&'static Formula, NodeId, &dyn StylerAccess) -> Option<Subpixel>,
) -> Option<Vec<(NodeId, Subpixel)>> {
    flex_resolve_main_wrap_impl(styler, Axis::Horizontal, resolve)
}

/// §9.7 resolver for column wrap.
fn flex_resolve_main_col_wrap(
    _node: NodeId,
    styler: &dyn StylerAccess,
    resolve: &mut dyn FnMut(&'static Formula, NodeId, &dyn StylerAccess) -> Option<Subpixel>,
) -> Option<Vec<(NodeId, Subpixel)>> {
    flex_resolve_main_wrap_impl(styler, Axis::Vertical, resolve)
}

/// Shared wrap implementation for both row and column.
///
/// Computes line assignments (greedy line breaking on basis sizes),
/// then runs §9.7 independently per line.
fn flex_resolve_main_wrap_impl(
    styler: &dyn StylerAccess,
    axis: Axis,
    resolve: &mut dyn FnMut(&'static Formula, NodeId, &dyn StylerAccess) -> Option<Subpixel>,
) -> Option<Vec<(NodeId, Subpixel)>> {
    let parent = styler.related(SingleRelationship::Parent);
    let children = parent.related_iter(MultiRelationship::OrderedChildren);

    let all_items = collect_flex_items(&children, axis, resolve);
    if all_items.is_empty() {
        return Some(Vec::new());
    }

    let container_main = resolve_container_content_main(parent.as_ref(), axis, resolve)?;
    let gap = resolve_main_gap(parent.as_ref(), axis);

    // Greedy line breaking: accumulate basis sizes, break when exceeding
    // available main. This matches the resolver's compute_line_assignments.
    let mut lines: Vec<Vec<usize>> = vec![Vec::new()];
    let mut line_used: f32 = 0.0;

    for (idx, item) in all_items.iter().enumerate() {
        let needed = if lines.last().unwrap().is_empty() {
            item.basis
        } else {
            item.basis + gap
        };

        if !lines.last().unwrap().is_empty() && line_used + needed > container_main {
            lines.push(Vec::new());
            line_used = item.basis;
        } else {
            line_used += needed;
        }

        lines.last_mut().unwrap().push(idx);
    }

    // Run §9.7 independently per line.
    let mut results: Vec<(NodeId, Subpixel)> = Vec::with_capacity(all_items.len());

    for line_indices in &lines {
        let mut line_items: Vec<FlexItemInfo> = line_indices
            .iter()
            .map(|&idx| {
                let item = &all_items[idx];
                FlexItemInfo {
                    node_id: item.node_id,
                    basis: item.basis,
                    grow: item.grow,
                    shrink: item.shrink,
                    min_main: item.min_main,
                    max_main: item.max_main,
                    frozen: false,
                    target: item.basis,
                }
            })
            .collect();

        resolve_flexible_lengths(&mut line_items, container_main, gap);

        for item in &line_items {
            results.push((item.node_id, Subpixel::from_f32(item.target)));
        }
    }

    Some(results)
}

// ============================================================================
// Main-axis offset
// ============================================================================

/// Main-axis offset for a flex item. Dispatches based on direction.
fn flex_main_offset(direction: FlexDirection, axis: Axis) -> &'static Formula {
    if is_reversed(direction) {
        // Reversed: dispatch through query to handle justify-content at runtime.
        match (direction, axis) {
            (FlexDirection::RowReverse, Axis::Horizontal) => {
                related!(Self_, flex_offset_row_reverse_query, Axis::Horizontal)
            }
            (FlexDirection::ColumnReverse, Axis::Vertical) => {
                related!(Self_, flex_offset_col_reverse_query, Axis::Vertical)
            }
            _ => unreachable!(),
        }
    } else {
        // Normal: dispatch through query to handle justify-content at runtime.
        match (direction, axis) {
            (FlexDirection::Row, Axis::Horizontal) => {
                related!(Self_, flex_offset_row_query, Axis::Horizontal)
            }
            (FlexDirection::Column, Axis::Vertical) => {
                related!(Self_, flex_offset_col_query, Axis::Vertical)
            }
            _ => unreachable!(),
        }
    }
}

/// Offset query for row (horizontal main, normal order).
fn flex_offset_row_query(styler: &dyn StylerAccess, _axis: Axis) -> Option<&'static Formula> {
    // Per CSS Flexbox §8.1: auto margins on the main axis absorb free space
    // before justify-content is applied.
    let ml_auto = is_margin_auto(styler, &PropertyId::MarginLeft);
    let mr_auto = is_margin_auto(styler, &PropertyId::MarginRight);
    if ml_auto || mr_auto {
        return Some(build_main_auto_margin_offset_h_row(ml_auto));
    }

    let jc = justify_content_of(styler);
    let wrap = parent_flex_wrap(styler);
    if is_wrapping(wrap) {
        Some(build_main_offset_wrap_h_row(jc))
    } else {
        Some(build_main_offset_normal_h_row(jc))
    }
}

/// Offset query for column (vertical main, normal order).
fn flex_offset_col_query(styler: &dyn StylerAccess, _axis: Axis) -> Option<&'static Formula> {
    // Per CSS Flexbox §8.1: auto margins on the main axis.
    let mt_auto = is_margin_auto(styler, &PropertyId::MarginTop);
    let mb_auto = is_margin_auto(styler, &PropertyId::MarginBottom);
    if mt_auto || mb_auto {
        return Some(build_main_auto_margin_offset_v_col(mt_auto));
    }

    let jc = justify_content_of(styler);
    let wrap = parent_flex_wrap(styler);
    if is_wrapping(wrap) {
        Some(build_main_offset_wrap_v_col(jc))
    } else {
        Some(build_main_offset_normal_v_col(jc))
    }
}

/// Offset query for row-reverse.
fn flex_offset_row_reverse_query(
    styler: &dyn StylerAccess,
    _axis: Axis,
) -> Option<&'static Formula> {
    let jc = justify_content_of(styler);
    let wrap = parent_flex_wrap(styler);
    if is_wrapping(wrap) {
        Some(build_main_offset_wrap_reversed_h_row(jc))
    } else {
        Some(build_main_offset_reversed_h_row(jc))
    }
}

/// Offset query for column-reverse.
fn flex_offset_col_reverse_query(
    styler: &dyn StylerAccess,
    _axis: Axis,
) -> Option<&'static Formula> {
    let jc = justify_content_of(styler);
    let wrap = parent_flex_wrap(styler);
    if is_wrapping(wrap) {
        Some(build_main_offset_wrap_reversed_v_col(jc))
    } else {
        Some(build_main_offset_reversed_v_col(jc))
    }
}

// ============================================================================
// Main-axis auto margin offset formulas
// ============================================================================

/// Main-axis offset for items with auto margins (row = horizontal main).
///
/// offset = sum(prev_main) + sum(prev_auto_margins) + start_auto_margin + prev_count * gap
///
/// `start_auto`: whether this item's margin-left is auto.
fn build_main_auto_margin_offset_h_row(start_auto: bool) -> &'static Formula {
    // per_auto = max(0, free) / max(1, total_auto_count)
    macro_rules! per_auto_h {
        () => {
            div!(
                max!(justify_free_h_row!(), constant!(Subpixel::ZERO)),
                max!(
                    related_val!(
                        Parent,
                        aggregate!(Sum, OrderedChildren, flex_auto_margin_count_row_query)
                    ),
                    constant!(Subpixel::raw(1))
                )
            )
        };
    }

    if start_auto {
        // Include this item's start auto margin in offset
        add!(
            aggregate!(
                Sum,
                OrderedPrevSiblings,
                flex_item_main_query,
                Axis::Horizontal
            ),
            aggregate!(Sum, OrderedPrevSiblings, flex_item_auto_margin_row_query),
            per_auto_h!(),
            mul!(
                aggregate!(Count, OrderedPrevSiblings, always_query),
                related_val!(Parent, css_prop!(ColumnGap))
            )
        )
    } else {
        // margin-right only auto: no start margin offset, but prev siblings may have auto margins
        add!(
            aggregate!(
                Sum,
                OrderedPrevSiblings,
                flex_item_main_query,
                Axis::Horizontal
            ),
            aggregate!(Sum, OrderedPrevSiblings, flex_item_auto_margin_row_query),
            mul!(
                aggregate!(Count, OrderedPrevSiblings, always_query),
                related_val!(Parent, css_prop!(ColumnGap))
            )
        )
    }
}

/// Main-axis offset for items with auto margins (column = vertical main).
fn build_main_auto_margin_offset_v_col(start_auto: bool) -> &'static Formula {
    macro_rules! per_auto_v {
        () => {
            div!(
                max!(justify_free_v_col!(), constant!(Subpixel::ZERO)),
                max!(
                    related_val!(
                        Parent,
                        aggregate!(Sum, OrderedChildren, flex_auto_margin_count_col_query)
                    ),
                    constant!(Subpixel::raw(1))
                )
            )
        };
    }

    if start_auto {
        add!(
            aggregate!(
                Sum,
                OrderedPrevSiblings,
                flex_item_main_query,
                Axis::Vertical
            ),
            aggregate!(Sum, OrderedPrevSiblings, flex_item_auto_margin_col_query),
            per_auto_v!(),
            mul!(
                aggregate!(Count, OrderedPrevSiblings, always_query),
                related_val!(Parent, css_prop!(RowGap))
            )
        )
    } else {
        add!(
            aggregate!(
                Sum,
                OrderedPrevSiblings,
                flex_item_main_query,
                Axis::Vertical
            ),
            aggregate!(Sum, OrderedPrevSiblings, flex_item_auto_margin_col_query),
            mul!(
                aggregate!(Count, OrderedPrevSiblings, always_query),
                related_val!(Parent, css_prop!(RowGap))
            )
        )
    }
}

// ============================================================================
// Normal main-axis offset formulas (row = horizontal main)
// ============================================================================

/// Build main-axis offset for row direction based on justify-content.
///
/// base = sum(prev_siblings' main sizes) + prev_count * gap
/// Then add justify-content spacing.
fn build_main_offset_normal_h_row(jc: JustifyMode) -> &'static Formula {
    // Shared sub-formulas as inline macro calls (each produces a distinct static).
    match jc {
        JustifyMode::FlexStart | JustifyMode::Stretch => {
            // offset = prev_sizes + prev_count * gap
            add!(
                aggregate!(
                    Sum,
                    OrderedPrevSiblings,
                    flex_item_main_query,
                    Axis::Horizontal
                ),
                mul!(
                    aggregate!(Count, OrderedPrevSiblings, always_query),
                    related_val!(Parent, css_prop!(ColumnGap))
                )
            )
        }
        JustifyMode::FlexEnd => {
            // offset = justify_free + prev_sizes + prev_count * gap
            add!(
                max!(justify_free_h_row!(), constant!(Subpixel::ZERO)),
                aggregate!(
                    Sum,
                    OrderedPrevSiblings,
                    flex_item_main_query,
                    Axis::Horizontal
                ),
                mul!(
                    aggregate!(Count, OrderedPrevSiblings, always_query),
                    related_val!(Parent, css_prop!(ColumnGap))
                )
            )
        }
        JustifyMode::Center => {
            add!(
                div!(
                    max!(justify_free_h_row!(), constant!(Subpixel::ZERO)),
                    constant!(Subpixel::raw(2))
                ),
                aggregate!(
                    Sum,
                    OrderedPrevSiblings,
                    flex_item_main_query,
                    Axis::Horizontal
                ),
                mul!(
                    aggregate!(Count, OrderedPrevSiblings, always_query),
                    related_val!(Parent, css_prop!(ColumnGap))
                )
            )
        }
        JustifyMode::SpaceBetween => {
            // per_gap = free / max(1, count - 1)
            // offset = prev_sizes + prev_count * (gap + per_gap)
            add!(
                aggregate!(
                    Sum,
                    OrderedPrevSiblings,
                    flex_item_main_query,
                    Axis::Horizontal
                ),
                mul!(
                    aggregate!(Count, OrderedPrevSiblings, always_query),
                    add!(
                        related_val!(Parent, css_prop!(ColumnGap)),
                        div!(
                            max!(justify_free_h_row!(), constant!(Subpixel::ZERO)),
                            max!(
                                sub!(
                                    related_val!(
                                        Parent,
                                        aggregate!(Count, OrderedChildren, always_query)
                                    ),
                                    constant!(Subpixel::raw(1))
                                ),
                                constant!(Subpixel::raw(1))
                            )
                        )
                    )
                )
            )
        }
        JustifyMode::SpaceAround => {
            // per_gap = free / max(1, count)
            // start = per_gap / 2
            // offset = start + prev_sizes + prev_count * (gap + per_gap)
            add!(
                div!(
                    div!(
                        max!(justify_free_h_row!(), constant!(Subpixel::ZERO)),
                        max!(
                            related_val!(Parent, aggregate!(Count, OrderedChildren, always_query)),
                            constant!(Subpixel::raw(1))
                        )
                    ),
                    constant!(Subpixel::raw(2))
                ),
                aggregate!(
                    Sum,
                    OrderedPrevSiblings,
                    flex_item_main_query,
                    Axis::Horizontal
                ),
                mul!(
                    aggregate!(Count, OrderedPrevSiblings, always_query),
                    add!(
                        related_val!(Parent, css_prop!(ColumnGap)),
                        div!(
                            max!(justify_free_h_row!(), constant!(Subpixel::ZERO)),
                            max!(
                                related_val!(
                                    Parent,
                                    aggregate!(Count, OrderedChildren, always_query)
                                ),
                                constant!(Subpixel::raw(1))
                            )
                        )
                    )
                )
            )
        }
        JustifyMode::SpaceEvenly => {
            // per_gap = free / max(1, count + 1)
            // start = per_gap (one gap before first item)
            // offset = (prev_count + 1) * per_gap + prev_sizes + prev_count * gap
            add!(
                mul!(
                    add!(
                        aggregate!(Count, OrderedPrevSiblings, always_query),
                        constant!(Subpixel::raw(1))
                    ),
                    div!(
                        max!(justify_free_h_row!(), constant!(Subpixel::ZERO)),
                        max!(
                            add!(
                                related_val!(
                                    Parent,
                                    aggregate!(Count, OrderedChildren, always_query)
                                ),
                                constant!(Subpixel::raw(1))
                            ),
                            constant!(Subpixel::raw(1))
                        )
                    )
                ),
                aggregate!(
                    Sum,
                    OrderedPrevSiblings,
                    flex_item_main_query,
                    Axis::Horizontal
                ),
                mul!(
                    aggregate!(Count, OrderedPrevSiblings, always_query),
                    related_val!(Parent, css_prop!(ColumnGap))
                )
            )
        }
    }
}

// ============================================================================
// Normal main-axis offset formulas (column = vertical main)
// ============================================================================

fn build_main_offset_normal_v_col(jc: JustifyMode) -> &'static Formula {
    match jc {
        JustifyMode::FlexStart | JustifyMode::Stretch => {
            add!(
                aggregate!(
                    Sum,
                    OrderedPrevSiblings,
                    flex_item_main_query,
                    Axis::Vertical
                ),
                mul!(
                    aggregate!(Count, OrderedPrevSiblings, always_query),
                    related_val!(Parent, css_prop!(RowGap))
                )
            )
        }
        JustifyMode::FlexEnd => {
            add!(
                max!(justify_free_v_col!(), constant!(Subpixel::ZERO)),
                aggregate!(
                    Sum,
                    OrderedPrevSiblings,
                    flex_item_main_query,
                    Axis::Vertical
                ),
                mul!(
                    aggregate!(Count, OrderedPrevSiblings, always_query),
                    related_val!(Parent, css_prop!(RowGap))
                )
            )
        }
        JustifyMode::Center => {
            add!(
                div!(
                    max!(justify_free_v_col!(), constant!(Subpixel::ZERO)),
                    constant!(Subpixel::raw(2))
                ),
                aggregate!(
                    Sum,
                    OrderedPrevSiblings,
                    flex_item_main_query,
                    Axis::Vertical
                ),
                mul!(
                    aggregate!(Count, OrderedPrevSiblings, always_query),
                    related_val!(Parent, css_prop!(RowGap))
                )
            )
        }
        JustifyMode::SpaceBetween => {
            add!(
                aggregate!(
                    Sum,
                    OrderedPrevSiblings,
                    flex_item_main_query,
                    Axis::Vertical
                ),
                mul!(
                    aggregate!(Count, OrderedPrevSiblings, always_query),
                    add!(
                        related_val!(Parent, css_prop!(RowGap)),
                        div!(
                            max!(justify_free_v_col!(), constant!(Subpixel::ZERO)),
                            max!(
                                sub!(
                                    related_val!(
                                        Parent,
                                        aggregate!(Count, OrderedChildren, always_query)
                                    ),
                                    constant!(Subpixel::raw(1))
                                ),
                                constant!(Subpixel::raw(1))
                            )
                        )
                    )
                )
            )
        }
        JustifyMode::SpaceAround => {
            add!(
                div!(
                    div!(
                        max!(justify_free_v_col!(), constant!(Subpixel::ZERO)),
                        max!(
                            related_val!(Parent, aggregate!(Count, OrderedChildren, always_query)),
                            constant!(Subpixel::raw(1))
                        )
                    ),
                    constant!(Subpixel::raw(2))
                ),
                aggregate!(
                    Sum,
                    OrderedPrevSiblings,
                    flex_item_main_query,
                    Axis::Vertical
                ),
                mul!(
                    aggregate!(Count, OrderedPrevSiblings, always_query),
                    add!(
                        related_val!(Parent, css_prop!(RowGap)),
                        div!(
                            max!(justify_free_v_col!(), constant!(Subpixel::ZERO)),
                            max!(
                                related_val!(
                                    Parent,
                                    aggregate!(Count, OrderedChildren, always_query)
                                ),
                                constant!(Subpixel::raw(1))
                            )
                        )
                    )
                )
            )
        }
        JustifyMode::SpaceEvenly => {
            add!(
                mul!(
                    add!(
                        aggregate!(Count, OrderedPrevSiblings, always_query),
                        constant!(Subpixel::raw(1))
                    ),
                    div!(
                        max!(justify_free_v_col!(), constant!(Subpixel::ZERO)),
                        max!(
                            add!(
                                related_val!(
                                    Parent,
                                    aggregate!(Count, OrderedChildren, always_query)
                                ),
                                constant!(Subpixel::raw(1))
                            ),
                            constant!(Subpixel::raw(1))
                        )
                    )
                ),
                aggregate!(
                    Sum,
                    OrderedPrevSiblings,
                    flex_item_main_query,
                    Axis::Vertical
                ),
                mul!(
                    aggregate!(Count, OrderedPrevSiblings, always_query),
                    related_val!(Parent, css_prop!(RowGap))
                )
            )
        }
    }
}

// ============================================================================
// Wrap main-axis offset formulas
// ============================================================================

// Per-line free space macros for justify-content with wrapping.
// These compute free_space = parent_content - line_sum(item_sizes) - line_gaps,
// scoped to the current line via line_item_aggregate.

/// Per-line free space for row-direction wrapping.
macro_rules! justify_free_wrap_h_row {
    () => {
        sub!(
            sub!(
                related!(Parent, size_query, Axis::Horizontal),
                related_val!(Parent, css_prop!(PaddingLeft)),
                related_val!(Parent, css_prop!(PaddingRight)),
                related_val!(Parent, css_prop!(BorderLeftWidth)),
                related_val!(Parent, css_prop!(BorderRightWidth))
            ),
            line_item_aggregate!(
                agg: Sum,
                rel: OrderedChildren,
                query: {
                    fn q(sty: &dyn StylerAccess) -> Option<&'static Formula> {
                        flex_item_main_query(sty, Axis::Horizontal)
                    }
                    q as QueryFn
                },
                item_main_size: lbp_item_main_size!(Row),
                available_main: lbp_available_main!(Row),
                gap: lbp_main_gap!(Row),
            ),
            mul!(
                related_val!(Parent, css_prop!(ColumnGap)),
                max!(
                    sub!(
                        line_item_aggregate!(
                            agg: Count,
                            rel: OrderedChildren,
                            query: always_query,
                            item_main_size: lbp_item_main_size!(Row),
                            available_main: lbp_available_main!(Row),
                            gap: lbp_main_gap!(Row),
                        ),
                        constant!(Subpixel::raw(1))
                    ),
                    constant!(Subpixel::ZERO)
                )
            )
        )
    };
}

/// Per-line free space for column-direction wrapping.
macro_rules! justify_free_wrap_v_col {
    () => {
        sub!(
            sub!(
                related!(Parent, size_query, Axis::Vertical),
                related_val!(Parent, css_prop!(PaddingTop)),
                related_val!(Parent, css_prop!(PaddingBottom)),
                related_val!(Parent, css_prop!(BorderTopWidth)),
                related_val!(Parent, css_prop!(BorderBottomWidth))
            ),
            line_item_aggregate!(
                agg: Sum,
                rel: OrderedChildren,
                query: {
                    fn q(sty: &dyn StylerAccess) -> Option<&'static Formula> {
                        flex_item_main_query(sty, Axis::Vertical)
                    }
                    q as QueryFn
                },
                item_main_size: lbp_item_main_size!(Column),
                available_main: lbp_available_main!(Column),
                gap: lbp_main_gap!(Column),
            ),
            mul!(
                related_val!(Parent, css_prop!(RowGap)),
                max!(
                    sub!(
                        line_item_aggregate!(
                            agg: Count,
                            rel: OrderedChildren,
                            query: always_query,
                            item_main_size: lbp_item_main_size!(Column),
                            available_main: lbp_available_main!(Column),
                            gap: lbp_main_gap!(Column),
                        ),
                        constant!(Subpixel::raw(1))
                    ),
                    constant!(Subpixel::ZERO)
                )
            )
        )
    };
}

/// Build main-axis offset for row direction with wrapping.
/// Uses LineItemAggregate to only sum same-line prev siblings.
/// Supports all justify-content modes with per-line free space.
fn build_main_offset_wrap_h_row(jc: JustifyMode) -> &'static Formula {
    // Shared sub-expressions as macros (line-scoped versions of the nowrap formulas).
    macro_rules! prev_sum {
        () => {
            line_item_aggregate!(
                agg: Sum,
                rel: OrderedPrevSiblings,
                query: {
                    fn q(sty: &dyn StylerAccess) -> Option<&'static Formula> {
                        flex_item_main_query(sty, Axis::Horizontal)
                    }
                    q as QueryFn
                },
                item_main_size: lbp_item_main_size!(Row),
                available_main: lbp_available_main!(Row),
                gap: lbp_main_gap!(Row),
            )
        };
    }
    macro_rules! prev_count {
        () => {
            line_item_aggregate!(
                agg: Count,
                rel: OrderedPrevSiblings,
                query: always_query,
                item_main_size: lbp_item_main_size!(Row),
                available_main: lbp_available_main!(Row),
                gap: lbp_main_gap!(Row),
            )
        };
    }
    macro_rules! line_count {
        () => {
            line_item_aggregate!(
                agg: Count,
                rel: OrderedChildren,
                query: always_query,
                item_main_size: lbp_item_main_size!(Row),
                available_main: lbp_available_main!(Row),
                gap: lbp_main_gap!(Row),
            )
        };
    }

    match jc {
        JustifyMode::FlexStart | JustifyMode::Stretch => {
            add!(
                prev_sum!(),
                mul!(prev_count!(), related_val!(Parent, css_prop!(ColumnGap)))
            )
        }
        JustifyMode::FlexEnd => {
            add!(
                max!(justify_free_wrap_h_row!(), constant!(Subpixel::ZERO)),
                prev_sum!(),
                mul!(prev_count!(), related_val!(Parent, css_prop!(ColumnGap)))
            )
        }
        JustifyMode::Center => {
            add!(
                div!(
                    max!(justify_free_wrap_h_row!(), constant!(Subpixel::ZERO)),
                    constant!(Subpixel::raw(2))
                ),
                prev_sum!(),
                mul!(prev_count!(), related_val!(Parent, css_prop!(ColumnGap)))
            )
        }
        JustifyMode::SpaceBetween => {
            add!(
                prev_sum!(),
                mul!(
                    prev_count!(),
                    add!(
                        related_val!(Parent, css_prop!(ColumnGap)),
                        div!(
                            max!(justify_free_wrap_h_row!(), constant!(Subpixel::ZERO)),
                            max!(
                                sub!(line_count!(), constant!(Subpixel::raw(1))),
                                constant!(Subpixel::raw(1))
                            )
                        )
                    )
                )
            )
        }
        JustifyMode::SpaceAround => {
            add!(
                div!(
                    div!(
                        max!(justify_free_wrap_h_row!(), constant!(Subpixel::ZERO)),
                        max!(line_count!(), constant!(Subpixel::raw(1)))
                    ),
                    constant!(Subpixel::raw(2))
                ),
                prev_sum!(),
                mul!(
                    prev_count!(),
                    add!(
                        related_val!(Parent, css_prop!(ColumnGap)),
                        div!(
                            max!(justify_free_wrap_h_row!(), constant!(Subpixel::ZERO)),
                            max!(line_count!(), constant!(Subpixel::raw(1)))
                        )
                    )
                )
            )
        }
        JustifyMode::SpaceEvenly => {
            add!(
                mul!(
                    add!(prev_count!(), constant!(Subpixel::raw(1))),
                    div!(
                        max!(justify_free_wrap_h_row!(), constant!(Subpixel::ZERO)),
                        max!(
                            add!(line_count!(), constant!(Subpixel::raw(1))),
                            constant!(Subpixel::raw(1))
                        )
                    )
                ),
                prev_sum!(),
                mul!(prev_count!(), related_val!(Parent, css_prop!(ColumnGap)))
            )
        }
    }
}

/// Build main-axis offset for column direction with wrapping.
/// Supports all justify-content modes with per-line free space.
fn build_main_offset_wrap_v_col(jc: JustifyMode) -> &'static Formula {
    macro_rules! prev_sum {
        () => {
            line_item_aggregate!(
                agg: Sum,
                rel: OrderedPrevSiblings,
                query: {
                    fn q(sty: &dyn StylerAccess) -> Option<&'static Formula> {
                        flex_item_main_query(sty, Axis::Vertical)
                    }
                    q as QueryFn
                },
                item_main_size: lbp_item_main_size!(Column),
                available_main: lbp_available_main!(Column),
                gap: lbp_main_gap!(Column),
            )
        };
    }
    macro_rules! prev_count {
        () => {
            line_item_aggregate!(
                agg: Count,
                rel: OrderedPrevSiblings,
                query: always_query,
                item_main_size: lbp_item_main_size!(Column),
                available_main: lbp_available_main!(Column),
                gap: lbp_main_gap!(Column),
            )
        };
    }
    macro_rules! line_count {
        () => {
            line_item_aggregate!(
                agg: Count,
                rel: OrderedChildren,
                query: always_query,
                item_main_size: lbp_item_main_size!(Column),
                available_main: lbp_available_main!(Column),
                gap: lbp_main_gap!(Column),
            )
        };
    }

    match jc {
        JustifyMode::FlexStart | JustifyMode::Stretch => {
            add!(
                prev_sum!(),
                mul!(prev_count!(), related_val!(Parent, css_prop!(RowGap)))
            )
        }
        JustifyMode::FlexEnd => {
            add!(
                max!(justify_free_wrap_v_col!(), constant!(Subpixel::ZERO)),
                prev_sum!(),
                mul!(prev_count!(), related_val!(Parent, css_prop!(RowGap)))
            )
        }
        JustifyMode::Center => {
            add!(
                div!(
                    max!(justify_free_wrap_v_col!(), constant!(Subpixel::ZERO)),
                    constant!(Subpixel::raw(2))
                ),
                prev_sum!(),
                mul!(prev_count!(), related_val!(Parent, css_prop!(RowGap)))
            )
        }
        JustifyMode::SpaceBetween => {
            add!(
                prev_sum!(),
                mul!(
                    prev_count!(),
                    add!(
                        related_val!(Parent, css_prop!(RowGap)),
                        div!(
                            max!(justify_free_wrap_v_col!(), constant!(Subpixel::ZERO)),
                            max!(
                                sub!(line_count!(), constant!(Subpixel::raw(1))),
                                constant!(Subpixel::raw(1))
                            )
                        )
                    )
                )
            )
        }
        JustifyMode::SpaceAround => {
            add!(
                div!(
                    div!(
                        max!(justify_free_wrap_v_col!(), constant!(Subpixel::ZERO)),
                        max!(line_count!(), constant!(Subpixel::raw(1)))
                    ),
                    constant!(Subpixel::raw(2))
                ),
                prev_sum!(),
                mul!(
                    prev_count!(),
                    add!(
                        related_val!(Parent, css_prop!(RowGap)),
                        div!(
                            max!(justify_free_wrap_v_col!(), constant!(Subpixel::ZERO)),
                            max!(line_count!(), constant!(Subpixel::raw(1)))
                        )
                    )
                )
            )
        }
        JustifyMode::SpaceEvenly => {
            add!(
                mul!(
                    add!(prev_count!(), constant!(Subpixel::raw(1))),
                    div!(
                        max!(justify_free_wrap_v_col!(), constant!(Subpixel::ZERO)),
                        max!(
                            add!(line_count!(), constant!(Subpixel::raw(1))),
                            constant!(Subpixel::raw(1))
                        )
                    )
                ),
                prev_sum!(),
                mul!(prev_count!(), related_val!(Parent, css_prop!(RowGap)))
            )
        }
    }
}

// ============================================================================
// Wrap + reversed main-axis offset formulas
// ============================================================================

/// Build main-axis offset for row-reverse with wrapping.
/// Combines the wrap pattern (line_item_aggregate) with the reversed pattern
/// (content_size - self_size - prev_sum - gaps - justify_adjust).
/// Supports all justify-content modes with per-line free space.
fn build_main_offset_wrap_reversed_h_row(jc: JustifyMode) -> &'static Formula {
    macro_rules! content_h {
        () => {
            sub!(
                related!(Parent, size_query, Axis::Horizontal),
                related_val!(Parent, css_prop!(PaddingLeft)),
                related_val!(Parent, css_prop!(PaddingRight)),
                related_val!(Parent, css_prop!(BorderLeftWidth)),
                related_val!(Parent, css_prop!(BorderRightWidth))
            )
        };
    }
    macro_rules! prev_sum {
        () => {
            line_item_aggregate!(
                agg: Sum,
                rel: OrderedPrevSiblings,
                query: {
                    fn q(sty: &dyn StylerAccess) -> Option<&'static Formula> {
                        flex_item_main_query(sty, Axis::Horizontal)
                    }
                    q as QueryFn
                },
                item_main_size: lbp_item_main_size!(Row),
                available_main: lbp_available_main!(Row),
                gap: lbp_main_gap!(Row),
            )
        };
    }
    macro_rules! prev_count {
        () => {
            line_item_aggregate!(
                agg: Count,
                rel: OrderedPrevSiblings,
                query: always_query,
                item_main_size: lbp_item_main_size!(Row),
                available_main: lbp_available_main!(Row),
                gap: lbp_main_gap!(Row),
            )
        };
    }
    macro_rules! line_count {
        () => {
            line_item_aggregate!(
                agg: Count,
                rel: OrderedChildren,
                query: always_query,
                item_main_size: lbp_item_main_size!(Row),
                available_main: lbp_available_main!(Row),
                gap: lbp_main_gap!(Row),
            )
        };
    }

    match jc {
        JustifyMode::FlexStart | JustifyMode::Stretch => {
            sub!(
                content_h!(),
                related!(Self_, flex_item_main_query, Axis::Horizontal),
                prev_sum!(),
                mul!(prev_count!(), related_val!(Parent, css_prop!(ColumnGap)))
            )
        }
        JustifyMode::FlexEnd => {
            sub!(
                content_h!(),
                related!(Self_, flex_item_main_query, Axis::Horizontal),
                prev_sum!(),
                mul!(prev_count!(), related_val!(Parent, css_prop!(ColumnGap))),
                max!(justify_free_wrap_h_row!(), constant!(Subpixel::ZERO))
            )
        }
        JustifyMode::Center => {
            sub!(
                content_h!(),
                related!(Self_, flex_item_main_query, Axis::Horizontal),
                prev_sum!(),
                mul!(prev_count!(), related_val!(Parent, css_prop!(ColumnGap))),
                div!(
                    max!(justify_free_wrap_h_row!(), constant!(Subpixel::ZERO)),
                    constant!(Subpixel::raw(2))
                )
            )
        }
        JustifyMode::SpaceBetween => {
            sub!(
                content_h!(),
                related!(Self_, flex_item_main_query, Axis::Horizontal),
                prev_sum!(),
                mul!(
                    prev_count!(),
                    add!(
                        related_val!(Parent, css_prop!(ColumnGap)),
                        div!(
                            max!(justify_free_wrap_h_row!(), constant!(Subpixel::ZERO)),
                            max!(
                                sub!(line_count!(), constant!(Subpixel::raw(1))),
                                constant!(Subpixel::raw(1))
                            )
                        )
                    )
                )
            )
        }
        JustifyMode::SpaceAround => {
            sub!(
                content_h!(),
                related!(Self_, flex_item_main_query, Axis::Horizontal),
                prev_sum!(),
                mul!(
                    prev_count!(),
                    add!(
                        related_val!(Parent, css_prop!(ColumnGap)),
                        div!(
                            max!(justify_free_wrap_h_row!(), constant!(Subpixel::ZERO)),
                            max!(line_count!(), constant!(Subpixel::raw(1)))
                        )
                    )
                ),
                div!(
                    div!(
                        max!(justify_free_wrap_h_row!(), constant!(Subpixel::ZERO)),
                        max!(line_count!(), constant!(Subpixel::raw(1)))
                    ),
                    constant!(Subpixel::raw(2))
                )
            )
        }
        JustifyMode::SpaceEvenly => {
            sub!(
                content_h!(),
                related!(Self_, flex_item_main_query, Axis::Horizontal),
                prev_sum!(),
                mul!(prev_count!(), related_val!(Parent, css_prop!(ColumnGap))),
                mul!(
                    add!(prev_count!(), constant!(Subpixel::raw(1))),
                    div!(
                        max!(justify_free_wrap_h_row!(), constant!(Subpixel::ZERO)),
                        max!(
                            add!(line_count!(), constant!(Subpixel::raw(1))),
                            constant!(Subpixel::raw(1))
                        )
                    )
                )
            )
        }
    }
}

/// Build main-axis offset for column-reverse with wrapping.
/// Supports all justify-content modes with per-line free space.
fn build_main_offset_wrap_reversed_v_col(jc: JustifyMode) -> &'static Formula {
    macro_rules! content_v {
        () => {
            sub!(
                related!(Parent, size_query, Axis::Vertical),
                related_val!(Parent, css_prop!(PaddingTop)),
                related_val!(Parent, css_prop!(PaddingBottom)),
                related_val!(Parent, css_prop!(BorderTopWidth)),
                related_val!(Parent, css_prop!(BorderBottomWidth))
            )
        };
    }
    macro_rules! prev_sum {
        () => {
            line_item_aggregate!(
                agg: Sum,
                rel: OrderedPrevSiblings,
                query: {
                    fn q(sty: &dyn StylerAccess) -> Option<&'static Formula> {
                        flex_item_main_query(sty, Axis::Vertical)
                    }
                    q as QueryFn
                },
                item_main_size: lbp_item_main_size!(Column),
                available_main: lbp_available_main!(Column),
                gap: lbp_main_gap!(Column),
            )
        };
    }
    macro_rules! prev_count {
        () => {
            line_item_aggregate!(
                agg: Count,
                rel: OrderedPrevSiblings,
                query: always_query,
                item_main_size: lbp_item_main_size!(Column),
                available_main: lbp_available_main!(Column),
                gap: lbp_main_gap!(Column),
            )
        };
    }
    macro_rules! line_count {
        () => {
            line_item_aggregate!(
                agg: Count,
                rel: OrderedChildren,
                query: always_query,
                item_main_size: lbp_item_main_size!(Column),
                available_main: lbp_available_main!(Column),
                gap: lbp_main_gap!(Column),
            )
        };
    }

    match jc {
        JustifyMode::FlexStart | JustifyMode::Stretch => {
            sub!(
                content_v!(),
                related!(Self_, flex_item_main_query, Axis::Vertical),
                prev_sum!(),
                mul!(prev_count!(), related_val!(Parent, css_prop!(RowGap)))
            )
        }
        JustifyMode::FlexEnd => {
            sub!(
                content_v!(),
                related!(Self_, flex_item_main_query, Axis::Vertical),
                prev_sum!(),
                mul!(prev_count!(), related_val!(Parent, css_prop!(RowGap))),
                max!(justify_free_wrap_v_col!(), constant!(Subpixel::ZERO))
            )
        }
        JustifyMode::Center => {
            sub!(
                content_v!(),
                related!(Self_, flex_item_main_query, Axis::Vertical),
                prev_sum!(),
                mul!(prev_count!(), related_val!(Parent, css_prop!(RowGap))),
                div!(
                    max!(justify_free_wrap_v_col!(), constant!(Subpixel::ZERO)),
                    constant!(Subpixel::raw(2))
                )
            )
        }
        JustifyMode::SpaceBetween => {
            sub!(
                content_v!(),
                related!(Self_, flex_item_main_query, Axis::Vertical),
                prev_sum!(),
                mul!(
                    prev_count!(),
                    add!(
                        related_val!(Parent, css_prop!(RowGap)),
                        div!(
                            max!(justify_free_wrap_v_col!(), constant!(Subpixel::ZERO)),
                            max!(
                                sub!(line_count!(), constant!(Subpixel::raw(1))),
                                constant!(Subpixel::raw(1))
                            )
                        )
                    )
                )
            )
        }
        JustifyMode::SpaceAround => {
            sub!(
                content_v!(),
                related!(Self_, flex_item_main_query, Axis::Vertical),
                prev_sum!(),
                mul!(
                    prev_count!(),
                    add!(
                        related_val!(Parent, css_prop!(RowGap)),
                        div!(
                            max!(justify_free_wrap_v_col!(), constant!(Subpixel::ZERO)),
                            max!(line_count!(), constant!(Subpixel::raw(1)))
                        )
                    )
                ),
                div!(
                    div!(
                        max!(justify_free_wrap_v_col!(), constant!(Subpixel::ZERO)),
                        max!(line_count!(), constant!(Subpixel::raw(1)))
                    ),
                    constant!(Subpixel::raw(2))
                )
            )
        }
        JustifyMode::SpaceEvenly => {
            sub!(
                content_v!(),
                related!(Self_, flex_item_main_query, Axis::Vertical),
                prev_sum!(),
                mul!(prev_count!(), related_val!(Parent, css_prop!(RowGap))),
                mul!(
                    add!(prev_count!(), constant!(Subpixel::raw(1))),
                    div!(
                        max!(justify_free_wrap_v_col!(), constant!(Subpixel::ZERO)),
                        max!(
                            add!(line_count!(), constant!(Subpixel::raw(1))),
                            constant!(Subpixel::raw(1))
                        )
                    )
                )
            )
        }
    }
}

// ============================================================================
// Reversed main-axis offset formulas
// ============================================================================

/// Reversed row: items pack from the right edge.
/// offset = parent_content - my_size - sum(prev_siblings' sizes) - prev_count * gap - justify_offset
///
/// In reverse layout the first DOM child is placed at the far end. Its offset
/// depends on the items that precede it in DOM order (which appear visually
/// after it). Hence we aggregate over `OrderedPrevSiblings`, not `NextSiblings`.
fn build_main_offset_reversed_h_row(jc: JustifyMode) -> &'static Formula {
    match jc {
        JustifyMode::FlexStart | JustifyMode::Stretch => {
            sub!(
                sub!(
                    related!(Parent, size_query, Axis::Horizontal),
                    related_val!(Parent, css_prop!(PaddingLeft)),
                    related_val!(Parent, css_prop!(PaddingRight)),
                    related_val!(Parent, css_prop!(BorderLeftWidth)),
                    related_val!(Parent, css_prop!(BorderRightWidth))
                ),
                related!(Self_, flex_item_main_query, Axis::Horizontal),
                aggregate!(
                    Sum,
                    OrderedPrevSiblings,
                    flex_item_main_query,
                    Axis::Horizontal
                ),
                mul!(
                    aggregate!(Count, OrderedPrevSiblings, always_query),
                    related_val!(Parent, css_prop!(ColumnGap))
                )
            )
        }
        JustifyMode::FlexEnd => {
            sub!(
                sub!(
                    related!(Parent, size_query, Axis::Horizontal),
                    related_val!(Parent, css_prop!(PaddingLeft)),
                    related_val!(Parent, css_prop!(PaddingRight)),
                    related_val!(Parent, css_prop!(BorderLeftWidth)),
                    related_val!(Parent, css_prop!(BorderRightWidth))
                ),
                related!(Self_, flex_item_main_query, Axis::Horizontal),
                aggregate!(
                    Sum,
                    OrderedPrevSiblings,
                    flex_item_main_query,
                    Axis::Horizontal
                ),
                mul!(
                    aggregate!(Count, OrderedPrevSiblings, always_query),
                    related_val!(Parent, css_prop!(ColumnGap))
                ),
                max!(justify_free_h_row!(), constant!(Subpixel::ZERO))
            )
        }
        JustifyMode::Center => {
            sub!(
                sub!(
                    related!(Parent, size_query, Axis::Horizontal),
                    related_val!(Parent, css_prop!(PaddingLeft)),
                    related_val!(Parent, css_prop!(PaddingRight)),
                    related_val!(Parent, css_prop!(BorderLeftWidth)),
                    related_val!(Parent, css_prop!(BorderRightWidth))
                ),
                related!(Self_, flex_item_main_query, Axis::Horizontal),
                aggregate!(
                    Sum,
                    OrderedPrevSiblings,
                    flex_item_main_query,
                    Axis::Horizontal
                ),
                mul!(
                    aggregate!(Count, OrderedPrevSiblings, always_query),
                    related_val!(Parent, css_prop!(ColumnGap))
                ),
                div!(
                    max!(justify_free_h_row!(), constant!(Subpixel::ZERO)),
                    constant!(Subpixel::raw(2))
                )
            )
        }
        JustifyMode::SpaceBetween => {
            sub!(
                sub!(
                    related!(Parent, size_query, Axis::Horizontal),
                    related_val!(Parent, css_prop!(PaddingLeft)),
                    related_val!(Parent, css_prop!(PaddingRight)),
                    related_val!(Parent, css_prop!(BorderLeftWidth)),
                    related_val!(Parent, css_prop!(BorderRightWidth))
                ),
                related!(Self_, flex_item_main_query, Axis::Horizontal),
                aggregate!(
                    Sum,
                    OrderedPrevSiblings,
                    flex_item_main_query,
                    Axis::Horizontal
                ),
                mul!(
                    aggregate!(Count, OrderedPrevSiblings, always_query),
                    add!(
                        related_val!(Parent, css_prop!(ColumnGap)),
                        div!(
                            max!(justify_free_h_row!(), constant!(Subpixel::ZERO)),
                            max!(
                                sub!(
                                    related_val!(
                                        Parent,
                                        aggregate!(Count, OrderedChildren, always_query)
                                    ),
                                    constant!(Subpixel::raw(1))
                                ),
                                constant!(Subpixel::raw(1))
                            )
                        )
                    )
                )
            )
        }
        JustifyMode::SpaceAround => {
            sub!(
                sub!(
                    related!(Parent, size_query, Axis::Horizontal),
                    related_val!(Parent, css_prop!(PaddingLeft)),
                    related_val!(Parent, css_prop!(PaddingRight)),
                    related_val!(Parent, css_prop!(BorderLeftWidth)),
                    related_val!(Parent, css_prop!(BorderRightWidth))
                ),
                related!(Self_, flex_item_main_query, Axis::Horizontal),
                aggregate!(
                    Sum,
                    OrderedPrevSiblings,
                    flex_item_main_query,
                    Axis::Horizontal
                ),
                mul!(
                    aggregate!(Count, OrderedPrevSiblings, always_query),
                    add!(
                        related_val!(Parent, css_prop!(ColumnGap)),
                        div!(
                            max!(justify_free_h_row!(), constant!(Subpixel::ZERO)),
                            max!(
                                related_val!(
                                    Parent,
                                    aggregate!(Count, OrderedChildren, always_query)
                                ),
                                constant!(Subpixel::raw(1))
                            )
                        )
                    )
                ),
                div!(
                    div!(
                        max!(justify_free_h_row!(), constant!(Subpixel::ZERO)),
                        max!(
                            related_val!(Parent, aggregate!(Count, OrderedChildren, always_query)),
                            constant!(Subpixel::raw(1))
                        )
                    ),
                    constant!(Subpixel::raw(2))
                )
            )
        }
        JustifyMode::SpaceEvenly => {
            sub!(
                sub!(
                    related!(Parent, size_query, Axis::Horizontal),
                    related_val!(Parent, css_prop!(PaddingLeft)),
                    related_val!(Parent, css_prop!(PaddingRight)),
                    related_val!(Parent, css_prop!(BorderLeftWidth)),
                    related_val!(Parent, css_prop!(BorderRightWidth))
                ),
                related!(Self_, flex_item_main_query, Axis::Horizontal),
                aggregate!(
                    Sum,
                    OrderedPrevSiblings,
                    flex_item_main_query,
                    Axis::Horizontal
                ),
                mul!(
                    aggregate!(Count, OrderedPrevSiblings, always_query),
                    related_val!(Parent, css_prop!(ColumnGap))
                ),
                mul!(
                    add!(
                        aggregate!(Count, OrderedPrevSiblings, always_query),
                        constant!(Subpixel::raw(1))
                    ),
                    div!(
                        max!(justify_free_h_row!(), constant!(Subpixel::ZERO)),
                        max!(
                            add!(
                                related_val!(
                                    Parent,
                                    aggregate!(Count, OrderedChildren, always_query)
                                ),
                                constant!(Subpixel::raw(1))
                            ),
                            constant!(Subpixel::raw(1))
                        )
                    )
                )
            )
        }
    }
}

/// Reversed column: items pack from the bottom edge.
/// Uses `OrderedPrevSiblings` (see `build_main_offset_reversed_h_row` for rationale).
fn build_main_offset_reversed_v_col(jc: JustifyMode) -> &'static Formula {
    match jc {
        JustifyMode::FlexStart | JustifyMode::Stretch => {
            sub!(
                sub!(
                    related!(Parent, size_query, Axis::Vertical),
                    related_val!(Parent, css_prop!(PaddingTop)),
                    related_val!(Parent, css_prop!(PaddingBottom)),
                    related_val!(Parent, css_prop!(BorderTopWidth)),
                    related_val!(Parent, css_prop!(BorderBottomWidth))
                ),
                related!(Self_, flex_item_main_query, Axis::Vertical),
                aggregate!(
                    Sum,
                    OrderedPrevSiblings,
                    flex_item_main_query,
                    Axis::Vertical
                ),
                mul!(
                    aggregate!(Count, OrderedPrevSiblings, always_query),
                    related_val!(Parent, css_prop!(RowGap))
                )
            )
        }
        JustifyMode::FlexEnd => {
            sub!(
                sub!(
                    related!(Parent, size_query, Axis::Vertical),
                    related_val!(Parent, css_prop!(PaddingTop)),
                    related_val!(Parent, css_prop!(PaddingBottom)),
                    related_val!(Parent, css_prop!(BorderTopWidth)),
                    related_val!(Parent, css_prop!(BorderBottomWidth))
                ),
                related!(Self_, flex_item_main_query, Axis::Vertical),
                aggregate!(
                    Sum,
                    OrderedPrevSiblings,
                    flex_item_main_query,
                    Axis::Vertical
                ),
                mul!(
                    aggregate!(Count, OrderedPrevSiblings, always_query),
                    related_val!(Parent, css_prop!(RowGap))
                ),
                max!(justify_free_v_col!(), constant!(Subpixel::ZERO))
            )
        }
        JustifyMode::Center => {
            sub!(
                sub!(
                    related!(Parent, size_query, Axis::Vertical),
                    related_val!(Parent, css_prop!(PaddingTop)),
                    related_val!(Parent, css_prop!(PaddingBottom)),
                    related_val!(Parent, css_prop!(BorderTopWidth)),
                    related_val!(Parent, css_prop!(BorderBottomWidth))
                ),
                related!(Self_, flex_item_main_query, Axis::Vertical),
                aggregate!(
                    Sum,
                    OrderedPrevSiblings,
                    flex_item_main_query,
                    Axis::Vertical
                ),
                mul!(
                    aggregate!(Count, OrderedPrevSiblings, always_query),
                    related_val!(Parent, css_prop!(RowGap))
                ),
                div!(
                    max!(justify_free_v_col!(), constant!(Subpixel::ZERO)),
                    constant!(Subpixel::raw(2))
                )
            )
        }
        JustifyMode::SpaceBetween => {
            sub!(
                sub!(
                    related!(Parent, size_query, Axis::Vertical),
                    related_val!(Parent, css_prop!(PaddingTop)),
                    related_val!(Parent, css_prop!(PaddingBottom)),
                    related_val!(Parent, css_prop!(BorderTopWidth)),
                    related_val!(Parent, css_prop!(BorderBottomWidth))
                ),
                related!(Self_, flex_item_main_query, Axis::Vertical),
                aggregate!(
                    Sum,
                    OrderedPrevSiblings,
                    flex_item_main_query,
                    Axis::Vertical
                ),
                mul!(
                    aggregate!(Count, OrderedPrevSiblings, always_query),
                    add!(
                        related_val!(Parent, css_prop!(RowGap)),
                        div!(
                            max!(justify_free_v_col!(), constant!(Subpixel::ZERO)),
                            max!(
                                sub!(
                                    related_val!(
                                        Parent,
                                        aggregate!(Count, OrderedChildren, always_query)
                                    ),
                                    constant!(Subpixel::raw(1))
                                ),
                                constant!(Subpixel::raw(1))
                            )
                        )
                    )
                )
            )
        }
        JustifyMode::SpaceAround => {
            sub!(
                sub!(
                    related!(Parent, size_query, Axis::Vertical),
                    related_val!(Parent, css_prop!(PaddingTop)),
                    related_val!(Parent, css_prop!(PaddingBottom)),
                    related_val!(Parent, css_prop!(BorderTopWidth)),
                    related_val!(Parent, css_prop!(BorderBottomWidth))
                ),
                related!(Self_, flex_item_main_query, Axis::Vertical),
                aggregate!(
                    Sum,
                    OrderedPrevSiblings,
                    flex_item_main_query,
                    Axis::Vertical
                ),
                mul!(
                    aggregate!(Count, OrderedPrevSiblings, always_query),
                    add!(
                        related_val!(Parent, css_prop!(RowGap)),
                        div!(
                            max!(justify_free_v_col!(), constant!(Subpixel::ZERO)),
                            max!(
                                related_val!(
                                    Parent,
                                    aggregate!(Count, OrderedChildren, always_query)
                                ),
                                constant!(Subpixel::raw(1))
                            )
                        )
                    )
                ),
                div!(
                    div!(
                        max!(justify_free_v_col!(), constant!(Subpixel::ZERO)),
                        max!(
                            related_val!(Parent, aggregate!(Count, OrderedChildren, always_query)),
                            constant!(Subpixel::raw(1))
                        )
                    ),
                    constant!(Subpixel::raw(2))
                )
            )
        }
        JustifyMode::SpaceEvenly => {
            sub!(
                sub!(
                    related!(Parent, size_query, Axis::Vertical),
                    related_val!(Parent, css_prop!(PaddingTop)),
                    related_val!(Parent, css_prop!(PaddingBottom)),
                    related_val!(Parent, css_prop!(BorderTopWidth)),
                    related_val!(Parent, css_prop!(BorderBottomWidth))
                ),
                related!(Self_, flex_item_main_query, Axis::Vertical),
                aggregate!(
                    Sum,
                    OrderedPrevSiblings,
                    flex_item_main_query,
                    Axis::Vertical
                ),
                mul!(
                    aggregate!(Count, OrderedPrevSiblings, always_query),
                    related_val!(Parent, css_prop!(RowGap))
                ),
                mul!(
                    add!(
                        aggregate!(Count, OrderedPrevSiblings, always_query),
                        constant!(Subpixel::raw(1))
                    ),
                    div!(
                        max!(justify_free_v_col!(), constant!(Subpixel::ZERO)),
                        max!(
                            add!(
                                related_val!(
                                    Parent,
                                    aggregate!(Count, OrderedChildren, always_query)
                                ),
                                constant!(Subpixel::raw(1))
                            ),
                            constant!(Subpixel::raw(1))
                        )
                    )
                )
            )
        }
    }
}

// ============================================================================
// Cross-axis offset
// ============================================================================

/// Cross-axis offset for a flex item. Dispatches via query for alignment.
/// For wrapping containers, also adds the sum of previous lines' cross sizes.
fn flex_cross_offset(parent_direction: FlexDirection, axis: Axis) -> &'static Formula {
    // Dispatch through a query that checks wrap at runtime.
    match (parent_direction, axis) {
        // Row direction → cross axis is vertical
        (FlexDirection::Row | FlexDirection::RowReverse, Axis::Vertical) => {
            related!(Self_, flex_cross_offset_row_query, Axis::Vertical)
        }
        // Column direction → cross axis is horizontal
        (FlexDirection::Column | FlexDirection::ColumnReverse, Axis::Horizontal) => {
            related!(Self_, flex_cross_offset_col_query, Axis::Horizontal)
        }
        _ => unreachable!("flex_cross_offset called on main axis"),
    }
}

/// Cross-offset query for row containers (cross = vertical).
fn flex_cross_offset_row_query(styler: &dyn StylerAccess, _axis: Axis) -> Option<&'static Formula> {
    // Per CSS Flexbox §8.1: auto margins on the cross axis are resolved
    // before alignment via align-self.
    let mt_auto = is_margin_auto(styler, &PropertyId::MarginTop);
    let mb_auto = is_margin_auto(styler, &PropertyId::MarginBottom);
    if mt_auto || mb_auto {
        return Some(build_cross_auto_margin_offset(
            mt_auto,
            mb_auto,
            Axis::Vertical,
        ));
    }

    let wrap = parent_flex_wrap(styler);
    let alignment = effective_cross_alignment(styler);
    let reverse = wrap == FlexWrap::WrapReverse;

    if is_wrapping(wrap) {
        let ac = align_content_of_parent(styler);
        Some(build_cross_offset_wrap_row(alignment, ac, reverse))
    } else {
        Some(build_cross_offset_nowrap(alignment, Axis::Vertical))
    }
}

/// Cross-offset query for column containers (cross = horizontal).
fn flex_cross_offset_col_query(styler: &dyn StylerAccess, _axis: Axis) -> Option<&'static Formula> {
    // Per CSS Flexbox §8.1: auto margins on the cross axis.
    let ml_auto = is_margin_auto(styler, &PropertyId::MarginLeft);
    let mr_auto = is_margin_auto(styler, &PropertyId::MarginRight);
    if ml_auto || mr_auto {
        return Some(build_cross_auto_margin_offset(
            ml_auto,
            mr_auto,
            Axis::Horizontal,
        ));
    }

    let wrap = parent_flex_wrap(styler);
    let alignment = effective_cross_alignment(styler);
    let reverse = wrap == FlexWrap::WrapReverse;

    if is_wrapping(wrap) {
        let ac = align_content_of_parent(styler);
        Some(build_cross_offset_wrap_col(alignment, ac, reverse))
    } else {
        Some(build_cross_offset_nowrap(alignment, Axis::Horizontal))
    }
}

/// Cross-axis offset when the item has auto margins on the cross axis.
///
/// Per CSS Flexbox §8.1, auto cross margins absorb free cross-space
/// before align-self is applied.
///
/// - Both auto: center → `(cross_content - item_cross) / 2`
/// - Start auto only: push to end → `cross_content - item_cross`
/// - End auto only: stays at start → `0`
fn build_cross_auto_margin_offset(
    start_auto: bool,
    end_auto: bool,
    axis: Axis,
) -> &'static Formula {
    match (start_auto, end_auto, axis) {
        // Both auto → center
        (true, true, Axis::Vertical) => div!(
            sub!(
                sub!(
                    related!(Parent, size_query, Axis::Vertical),
                    related_val!(Parent, css_prop!(PaddingTop)),
                    related_val!(Parent, css_prop!(PaddingBottom)),
                    related_val!(Parent, css_prop!(BorderTopWidth)),
                    related_val!(Parent, css_prop!(BorderBottomWidth))
                ),
                related!(Self_, flex_item_cross_query, Axis::Vertical)
            ),
            constant!(Subpixel::raw(2))
        ),
        (true, true, Axis::Horizontal) => div!(
            sub!(
                sub!(
                    related!(Parent, size_query, Axis::Horizontal),
                    related_val!(Parent, css_prop!(PaddingLeft)),
                    related_val!(Parent, css_prop!(PaddingRight)),
                    related_val!(Parent, css_prop!(BorderLeftWidth)),
                    related_val!(Parent, css_prop!(BorderRightWidth))
                ),
                related!(Self_, flex_item_cross_query, Axis::Horizontal)
            ),
            constant!(Subpixel::raw(2))
        ),
        // Only start auto → push to end
        (true, false, Axis::Vertical) => sub!(
            sub!(
                related!(Parent, size_query, Axis::Vertical),
                related_val!(Parent, css_prop!(PaddingTop)),
                related_val!(Parent, css_prop!(PaddingBottom)),
                related_val!(Parent, css_prop!(BorderTopWidth)),
                related_val!(Parent, css_prop!(BorderBottomWidth))
            ),
            related!(Self_, flex_item_cross_query, Axis::Vertical)
        ),
        (true, false, Axis::Horizontal) => sub!(
            sub!(
                related!(Parent, size_query, Axis::Horizontal),
                related_val!(Parent, css_prop!(PaddingLeft)),
                related_val!(Parent, css_prop!(PaddingRight)),
                related_val!(Parent, css_prop!(BorderLeftWidth)),
                related_val!(Parent, css_prop!(BorderRightWidth))
            ),
            related!(Self_, flex_item_cross_query, Axis::Horizontal)
        ),
        // Only end auto → stays at start
        (false, true, _) => constant!(Subpixel::ZERO),
        // Neither auto → shouldn't be called, but return 0
        (false, false, _) => constant!(Subpixel::ZERO),
    }
}

/// Cross-axis offset for nowrap containers.
/// Alignment is relative to the full container cross content size.
fn build_cross_offset_nowrap(alignment: CrossAlign, axis: Axis) -> &'static Formula {
    match alignment {
        CrossAlign::FlexStart | CrossAlign::Stretch => constant!(Subpixel::ZERO),
        CrossAlign::FlexEnd => match axis {
            Axis::Horizontal => sub!(
                sub!(
                    related!(Parent, size_query, Axis::Horizontal),
                    related_val!(Parent, css_prop!(PaddingLeft)),
                    related_val!(Parent, css_prop!(PaddingRight)),
                    related_val!(Parent, css_prop!(BorderLeftWidth)),
                    related_val!(Parent, css_prop!(BorderRightWidth))
                ),
                related!(Self_, flex_item_cross_query, Axis::Horizontal)
            ),
            Axis::Vertical => sub!(
                sub!(
                    related!(Parent, size_query, Axis::Vertical),
                    related_val!(Parent, css_prop!(PaddingTop)),
                    related_val!(Parent, css_prop!(PaddingBottom)),
                    related_val!(Parent, css_prop!(BorderTopWidth)),
                    related_val!(Parent, css_prop!(BorderBottomWidth))
                ),
                related!(Self_, flex_item_cross_query, Axis::Vertical)
            ),
        },
        CrossAlign::Center => match axis {
            Axis::Horizontal => div!(
                sub!(
                    sub!(
                        related!(Parent, size_query, Axis::Horizontal),
                        related_val!(Parent, css_prop!(PaddingLeft)),
                        related_val!(Parent, css_prop!(PaddingRight)),
                        related_val!(Parent, css_prop!(BorderLeftWidth)),
                        related_val!(Parent, css_prop!(BorderRightWidth))
                    ),
                    related!(Self_, flex_item_cross_query, Axis::Horizontal)
                ),
                constant!(Subpixel::raw(2))
            ),
            Axis::Vertical => div!(
                sub!(
                    sub!(
                        related!(Parent, size_query, Axis::Vertical),
                        related_val!(Parent, css_prop!(PaddingTop)),
                        related_val!(Parent, css_prop!(PaddingBottom)),
                        related_val!(Parent, css_prop!(BorderTopWidth)),
                        related_val!(Parent, css_prop!(BorderBottomWidth))
                    ),
                    related!(Self_, flex_item_cross_query, Axis::Vertical)
                ),
                constant!(Subpixel::raw(2))
            ),
        },
        // Baseline alignment: offset = max_baseline - my_baseline
        // All items on the line are shifted so their baselines align.
        // The max baseline must be computed over the parent's OrderedChildren
        // (siblings), not the current item's children.
        CrossAlign::Baseline => match axis {
            Axis::Horizontal => sub!(
                related!(Parent, max_baseline_query, Axis::Horizontal),
                related!(Self_, flex_item_baseline_query, Axis::Horizontal)
            ),
            Axis::Vertical => sub!(
                related!(Parent, max_baseline_query, Axis::Vertical),
                related!(Self_, flex_item_baseline_query, Axis::Vertical)
            ),
        },
    }
}

/// Cross-axis offset for wrapping row containers (cross = vertical).
///
/// offset = sum of previous lines' max cross sizes + cross gaps
///        + within-line alignment offset
///        + align-content stretch adjustment.
fn build_cross_offset_wrap_row(
    alignment: CrossAlign,
    ac: AlignContentMode,
    reverse: bool,
) -> &'static Formula {
    // These must be macros (not `let` bindings) because they produce
    // `&'static Formula` via macros that create `static` items.
    macro_rules! line_position {
        () => {
            prev_lines_aggregate!(
                line_agg: Sum,
                within_line_agg: Max,
                item_main_size: lbp_item_main_size!(Row),
                item_value: lbp_cross_query!(Row),
                available_main: lbp_available_main!(Row),
                gap: lbp_main_gap!(Row),
                line_gap: lbp_cross_gap!(Row),
            )
        };
    }

    macro_rules! my_line_cross {
        () => {
            line_item_aggregate!(
                agg: Max,
                rel: OrderedChildren,
                query: lbp_cross_query!(Row),
                item_main_size: lbp_item_main_size!(Row),
                available_main: lbp_available_main!(Row),
                gap: lbp_main_gap!(Row),
            )
        };
    }

    // Helper macros for align-content distribution (shared across all modes).
    // These must be macros because they produce `&'static Formula`.

    /// Number of previous lines (raw count, no gap contribution).
    macro_rules! prev_line_count {
        () => {
            prev_lines_aggregate!(
                line_agg: Count,
                within_line_agg: Max,
                item_main_size: lbp_item_main_size!(Row),
                item_value: lbp_cross_query!(Row),
                available_main: lbp_available_main!(Row),
                gap: lbp_main_gap!(Row),
                line_gap: constant!(Subpixel::ZERO),
            )
        };
    }

    /// Total number of lines (raw count), resolved on parent.
    macro_rules! total_line_count {
        () => {
            related_val!(
                Parent,
                line_aggregate!(
                    line_agg: Count,
                    within_line_agg: Max,
                    item_main_size: lbp_item_main_size!(Row),
                    item_value: lbp_cross_query!(Row),
                    available_main: lbp_available_main!(Row),
                    gap: lbp_main_gap!(Row),
                    line_gap: constant!(Subpixel::ZERO),
                )
            )
        };
    }

    /// Sum of all lines' max cross sizes (no gaps).
    macro_rules! total_lines_cross {
        () => {
            related_val!(
                Parent,
                line_aggregate!(
                    line_agg: Sum,
                    within_line_agg: Max,
                    item_main_size: lbp_item_main_size!(Row),
                    item_value: lbp_cross_query!(Row),
                    available_main: lbp_available_main!(Row),
                    gap: lbp_main_gap!(Row),
                    line_gap: constant!(Subpixel::ZERO),
                )
            )
        };
    }

    /// Parent cross content size (vertical for row containers).
    macro_rules! cross_content {
        () => {
            sub!(
                related!(Parent, size_query, Axis::Vertical),
                related_val!(Parent, css_prop!(PaddingTop)),
                related_val!(Parent, css_prop!(PaddingBottom)),
                related_val!(Parent, css_prop!(BorderTopWidth)),
                related_val!(Parent, css_prop!(BorderBottomWidth))
            )
        };
    }

    /// free_cross = max(0, cross_content - total_lines_cross - (line_count - 1) * cross_gap)
    macro_rules! free_cross {
        () => {
            max!(
                sub!(
                    cross_content!(),
                    total_lines_cross!(),
                    mul!(
                        max!(
                            sub!(total_line_count!(), constant!(Subpixel::raw(1))),
                            constant!(Subpixel::ZERO)
                        ),
                        related_val!(Parent, css_prop!(RowGap))
                    )
                ),
                constant!(Subpixel::ZERO)
            )
        };
    }

    // Align-content offset formulas for each mode.
    // These must be separate macros (not a match on runtime `ac`) because
    // the formula macros create `static` items.

    macro_rules! ac_stretch {
        () => {
            mul!(
                prev_line_count!(),
                div!(
                    free_cross!(),
                    max!(total_line_count!(), constant!(Subpixel::raw(1)))
                )
            )
        };
    }

    macro_rules! ac_flex_end {
        () => {
            free_cross!()
        };
    }

    macro_rules! ac_center {
        () => {
            div!(free_cross!(), constant!(Subpixel::raw(2)))
        };
    }

    macro_rules! ac_space_between {
        () => {
            mul!(
                prev_line_count!(),
                div!(
                    free_cross!(),
                    max!(
                        sub!(total_line_count!(), constant!(Subpixel::raw(1))),
                        constant!(Subpixel::raw(1))
                    )
                )
            )
        };
    }

    macro_rules! ac_space_around {
        () => {
            mul!(
                add!(
                    mul!(prev_line_count!(), constant!(Subpixel::raw(2))),
                    constant!(Subpixel::raw(1))
                ),
                div!(
                    free_cross!(),
                    mul!(total_line_count!(), constant!(Subpixel::raw(2)))
                )
            )
        };
    }

    macro_rules! ac_space_evenly {
        () => {
            mul!(
                add!(prev_line_count!(), constant!(Subpixel::raw(1))),
                div!(
                    free_cross!(),
                    add!(total_line_count!(), constant!(Subpixel::raw(1)))
                )
            )
        };
    }

    // Within-line alignment offset macros (from align-items / align-self).
    macro_rules! wl_flex_end {
        () => {
            sub!(
                my_line_cross!(),
                related!(Self_, flex_item_cross_query, Axis::Vertical)
            )
        };
    }

    macro_rules! wl_center {
        () => {
            div!(
                sub!(
                    my_line_cross!(),
                    related!(Self_, flex_item_cross_query, Axis::Vertical)
                ),
                constant!(Subpixel::raw(2))
            )
        };
    }

    macro_rules! wl_baseline {
        () => {
            sub!(
                line_item_aggregate!(
                    agg: Max,
                    rel: OrderedChildren,
                    query: lbp_baseline_query!(Row),
                    item_main_size: lbp_item_main_size!(Row),
                    available_main: lbp_available_main!(Row),
                    gap: lbp_main_gap!(Row),
                ),
                related!(Self_, flex_item_baseline_query, Axis::Vertical)
            )
        };
    }

    // Helper to apply wrap-reverse mirroring.
    // reverse_offset = cross_content - normal - item_cross
    macro_rules! rev {
        ($normal:expr) => {
            sub!(
                cross_content!(),
                $normal,
                related!(Self_, flex_item_cross_query, Axis::Vertical)
            )
        };
    }

    // Helper that wraps a formula in reverse if needed.
    macro_rules! maybe_rev {
        ($normal:expr) => {
            if reverse { rev!($normal) } else { $normal }
        };
    }

    // Full cartesian product: (alignment, ac) → formula.
    // Each arm must be a top-level return so the match is outside all static contexts.
    match (alignment, ac) {
        (CrossAlign::FlexStart | CrossAlign::Stretch, AlignContentMode::FlexStart) => {
            maybe_rev!(line_position!())
        }
        (CrossAlign::FlexStart | CrossAlign::Stretch, AlignContentMode::Stretch) => {
            maybe_rev!(add!(line_position!(), ac_stretch!()))
        }
        (CrossAlign::FlexStart | CrossAlign::Stretch, AlignContentMode::FlexEnd) => {
            maybe_rev!(add!(line_position!(), ac_flex_end!()))
        }
        (CrossAlign::FlexStart | CrossAlign::Stretch, AlignContentMode::Center) => {
            maybe_rev!(add!(line_position!(), ac_center!()))
        }
        (CrossAlign::FlexStart | CrossAlign::Stretch, AlignContentMode::SpaceBetween) => {
            maybe_rev!(add!(line_position!(), ac_space_between!()))
        }
        (CrossAlign::FlexStart | CrossAlign::Stretch, AlignContentMode::SpaceAround) => {
            maybe_rev!(add!(line_position!(), ac_space_around!()))
        }
        (CrossAlign::FlexStart | CrossAlign::Stretch, AlignContentMode::SpaceEvenly) => {
            maybe_rev!(add!(line_position!(), ac_space_evenly!()))
        }
        (CrossAlign::FlexEnd, AlignContentMode::FlexStart) => {
            maybe_rev!(add!(line_position!(), wl_flex_end!()))
        }
        (CrossAlign::FlexEnd, AlignContentMode::Stretch) => {
            maybe_rev!(add!(line_position!(), ac_stretch!(), wl_flex_end!()))
        }
        (CrossAlign::FlexEnd, AlignContentMode::FlexEnd) => {
            maybe_rev!(add!(line_position!(), ac_flex_end!(), wl_flex_end!()))
        }
        (CrossAlign::FlexEnd, AlignContentMode::Center) => {
            maybe_rev!(add!(line_position!(), ac_center!(), wl_flex_end!()))
        }
        (CrossAlign::FlexEnd, AlignContentMode::SpaceBetween) => {
            maybe_rev!(add!(line_position!(), ac_space_between!(), wl_flex_end!()))
        }
        (CrossAlign::FlexEnd, AlignContentMode::SpaceAround) => {
            maybe_rev!(add!(line_position!(), ac_space_around!(), wl_flex_end!()))
        }
        (CrossAlign::FlexEnd, AlignContentMode::SpaceEvenly) => {
            maybe_rev!(add!(line_position!(), ac_space_evenly!(), wl_flex_end!()))
        }
        (CrossAlign::Center, AlignContentMode::FlexStart) => {
            maybe_rev!(add!(line_position!(), wl_center!()))
        }
        (CrossAlign::Center, AlignContentMode::Stretch) => {
            maybe_rev!(add!(line_position!(), ac_stretch!(), wl_center!()))
        }
        (CrossAlign::Center, AlignContentMode::FlexEnd) => {
            maybe_rev!(add!(line_position!(), ac_flex_end!(), wl_center!()))
        }
        (CrossAlign::Center, AlignContentMode::Center) => {
            maybe_rev!(add!(line_position!(), ac_center!(), wl_center!()))
        }
        (CrossAlign::Center, AlignContentMode::SpaceBetween) => {
            maybe_rev!(add!(line_position!(), ac_space_between!(), wl_center!()))
        }
        (CrossAlign::Center, AlignContentMode::SpaceAround) => {
            maybe_rev!(add!(line_position!(), ac_space_around!(), wl_center!()))
        }
        (CrossAlign::Center, AlignContentMode::SpaceEvenly) => {
            maybe_rev!(add!(line_position!(), ac_space_evenly!(), wl_center!()))
        }
        (CrossAlign::Baseline, AlignContentMode::FlexStart) => {
            maybe_rev!(add!(line_position!(), wl_baseline!()))
        }
        (CrossAlign::Baseline, AlignContentMode::Stretch) => {
            maybe_rev!(add!(line_position!(), ac_stretch!(), wl_baseline!()))
        }
        (CrossAlign::Baseline, AlignContentMode::FlexEnd) => {
            maybe_rev!(add!(line_position!(), ac_flex_end!(), wl_baseline!()))
        }
        (CrossAlign::Baseline, AlignContentMode::Center) => {
            maybe_rev!(add!(line_position!(), ac_center!(), wl_baseline!()))
        }
        (CrossAlign::Baseline, AlignContentMode::SpaceBetween) => {
            maybe_rev!(add!(line_position!(), ac_space_between!(), wl_baseline!()))
        }
        (CrossAlign::Baseline, AlignContentMode::SpaceAround) => {
            maybe_rev!(add!(line_position!(), ac_space_around!(), wl_baseline!()))
        }
        (CrossAlign::Baseline, AlignContentMode::SpaceEvenly) => {
            maybe_rev!(add!(line_position!(), ac_space_evenly!(), wl_baseline!()))
        }
    }
}

/// Cross-axis offset for wrapping column containers (cross = horizontal).
///
/// offset = sum of previous lines' max cross sizes + cross gaps
///        + within-line alignment offset
///        + align-content stretch adjustment.
fn build_cross_offset_wrap_col(
    alignment: CrossAlign,
    ac: AlignContentMode,
    reverse: bool,
) -> &'static Formula {
    macro_rules! line_position {
        () => {
            prev_lines_aggregate!(
                line_agg: Sum,
                within_line_agg: Max,
                item_main_size: lbp_item_main_size!(Column),
                item_value: lbp_cross_query!(Column),
                available_main: lbp_available_main!(Column),
                gap: lbp_main_gap!(Column),
                line_gap: lbp_cross_gap!(Column),
            )
        };
    }

    macro_rules! my_line_cross {
        () => {
            line_item_aggregate!(
                agg: Max,
                rel: OrderedChildren,
                query: lbp_cross_query!(Column),
                item_main_size: lbp_item_main_size!(Column),
                available_main: lbp_available_main!(Column),
                gap: lbp_main_gap!(Column),
            )
        };
    }

    // Helper macros for align-content distribution (shared across all modes).

    macro_rules! prev_line_count {
        () => {
            prev_lines_aggregate!(
                line_agg: Count,
                within_line_agg: Max,
                item_main_size: lbp_item_main_size!(Column),
                item_value: lbp_cross_query!(Column),
                available_main: lbp_available_main!(Column),
                gap: lbp_main_gap!(Column),
                line_gap: constant!(Subpixel::ZERO),
            )
        };
    }

    macro_rules! total_line_count {
        () => {
            related_val!(
                Parent,
                line_aggregate!(
                    line_agg: Count,
                    within_line_agg: Max,
                    item_main_size: lbp_item_main_size!(Column),
                    item_value: lbp_cross_query!(Column),
                    available_main: lbp_available_main!(Column),
                    gap: lbp_main_gap!(Column),
                    line_gap: constant!(Subpixel::ZERO),
                )
            )
        };
    }

    macro_rules! total_lines_cross {
        () => {
            related_val!(
                Parent,
                line_aggregate!(
                    line_agg: Sum,
                    within_line_agg: Max,
                    item_main_size: lbp_item_main_size!(Column),
                    item_value: lbp_cross_query!(Column),
                    available_main: lbp_available_main!(Column),
                    gap: lbp_main_gap!(Column),
                    line_gap: constant!(Subpixel::ZERO),
                )
            )
        };
    }

    macro_rules! cross_content {
        () => {
            sub!(
                related!(Parent, size_query, Axis::Horizontal),
                related_val!(Parent, css_prop!(PaddingLeft)),
                related_val!(Parent, css_prop!(PaddingRight)),
                related_val!(Parent, css_prop!(BorderLeftWidth)),
                related_val!(Parent, css_prop!(BorderRightWidth))
            )
        };
    }

    macro_rules! free_cross {
        () => {
            max!(
                sub!(
                    cross_content!(),
                    total_lines_cross!(),
                    mul!(
                        max!(
                            sub!(total_line_count!(), constant!(Subpixel::raw(1))),
                            constant!(Subpixel::ZERO)
                        ),
                        related_val!(Parent, css_prop!(ColumnGap))
                    )
                ),
                constant!(Subpixel::ZERO)
            )
        };
    }

    macro_rules! ac_stretch {
        () => {
            mul!(
                prev_line_count!(),
                div!(
                    free_cross!(),
                    max!(total_line_count!(), constant!(Subpixel::raw(1)))
                )
            )
        };
    }

    macro_rules! ac_flex_end {
        () => {
            free_cross!()
        };
    }

    macro_rules! ac_center {
        () => {
            div!(free_cross!(), constant!(Subpixel::raw(2)))
        };
    }

    macro_rules! ac_space_between {
        () => {
            mul!(
                prev_line_count!(),
                div!(
                    free_cross!(),
                    max!(
                        sub!(total_line_count!(), constant!(Subpixel::raw(1))),
                        constant!(Subpixel::raw(1))
                    )
                )
            )
        };
    }

    macro_rules! ac_space_around {
        () => {
            mul!(
                add!(
                    mul!(prev_line_count!(), constant!(Subpixel::raw(2))),
                    constant!(Subpixel::raw(1))
                ),
                div!(
                    free_cross!(),
                    mul!(total_line_count!(), constant!(Subpixel::raw(2)))
                )
            )
        };
    }

    macro_rules! ac_space_evenly {
        () => {
            mul!(
                add!(prev_line_count!(), constant!(Subpixel::raw(1))),
                div!(
                    free_cross!(),
                    add!(total_line_count!(), constant!(Subpixel::raw(1)))
                )
            )
        };
    }

    macro_rules! wl_flex_end {
        () => {
            sub!(
                my_line_cross!(),
                related!(Self_, flex_item_cross_query, Axis::Horizontal)
            )
        };
    }

    macro_rules! wl_center {
        () => {
            div!(
                sub!(
                    my_line_cross!(),
                    related!(Self_, flex_item_cross_query, Axis::Horizontal)
                ),
                constant!(Subpixel::raw(2))
            )
        };
    }

    macro_rules! wl_baseline {
        () => {
            sub!(
                line_item_aggregate!(
                    agg: Max,
                    rel: OrderedChildren,
                    query: lbp_baseline_query!(Column),
                    item_main_size: lbp_item_main_size!(Column),
                    available_main: lbp_available_main!(Column),
                    gap: lbp_main_gap!(Column),
                ),
                related!(Self_, flex_item_baseline_query, Axis::Horizontal)
            )
        };
    }

    macro_rules! rev {
        ($normal:expr) => {
            sub!(
                cross_content!(),
                $normal,
                related!(Self_, flex_item_cross_query, Axis::Horizontal)
            )
        };
    }

    macro_rules! maybe_rev {
        ($normal:expr) => {
            if reverse { rev!($normal) } else { $normal }
        };
    }

    match (alignment, ac) {
        (CrossAlign::FlexStart | CrossAlign::Stretch, AlignContentMode::FlexStart) => {
            maybe_rev!(line_position!())
        }
        (CrossAlign::FlexStart | CrossAlign::Stretch, AlignContentMode::Stretch) => {
            maybe_rev!(add!(line_position!(), ac_stretch!()))
        }
        (CrossAlign::FlexStart | CrossAlign::Stretch, AlignContentMode::FlexEnd) => {
            maybe_rev!(add!(line_position!(), ac_flex_end!()))
        }
        (CrossAlign::FlexStart | CrossAlign::Stretch, AlignContentMode::Center) => {
            maybe_rev!(add!(line_position!(), ac_center!()))
        }
        (CrossAlign::FlexStart | CrossAlign::Stretch, AlignContentMode::SpaceBetween) => {
            maybe_rev!(add!(line_position!(), ac_space_between!()))
        }
        (CrossAlign::FlexStart | CrossAlign::Stretch, AlignContentMode::SpaceAround) => {
            maybe_rev!(add!(line_position!(), ac_space_around!()))
        }
        (CrossAlign::FlexStart | CrossAlign::Stretch, AlignContentMode::SpaceEvenly) => {
            maybe_rev!(add!(line_position!(), ac_space_evenly!()))
        }
        (CrossAlign::FlexEnd, AlignContentMode::FlexStart) => {
            maybe_rev!(add!(line_position!(), wl_flex_end!()))
        }
        (CrossAlign::FlexEnd, AlignContentMode::Stretch) => {
            maybe_rev!(add!(line_position!(), ac_stretch!(), wl_flex_end!()))
        }
        (CrossAlign::FlexEnd, AlignContentMode::FlexEnd) => {
            maybe_rev!(add!(line_position!(), ac_flex_end!(), wl_flex_end!()))
        }
        (CrossAlign::FlexEnd, AlignContentMode::Center) => {
            maybe_rev!(add!(line_position!(), ac_center!(), wl_flex_end!()))
        }
        (CrossAlign::FlexEnd, AlignContentMode::SpaceBetween) => {
            maybe_rev!(add!(line_position!(), ac_space_between!(), wl_flex_end!()))
        }
        (CrossAlign::FlexEnd, AlignContentMode::SpaceAround) => {
            maybe_rev!(add!(line_position!(), ac_space_around!(), wl_flex_end!()))
        }
        (CrossAlign::FlexEnd, AlignContentMode::SpaceEvenly) => {
            maybe_rev!(add!(line_position!(), ac_space_evenly!(), wl_flex_end!()))
        }
        (CrossAlign::Center, AlignContentMode::FlexStart) => {
            maybe_rev!(add!(line_position!(), wl_center!()))
        }
        (CrossAlign::Center, AlignContentMode::Stretch) => {
            maybe_rev!(add!(line_position!(), ac_stretch!(), wl_center!()))
        }
        (CrossAlign::Center, AlignContentMode::FlexEnd) => {
            maybe_rev!(add!(line_position!(), ac_flex_end!(), wl_center!()))
        }
        (CrossAlign::Center, AlignContentMode::Center) => {
            maybe_rev!(add!(line_position!(), ac_center!(), wl_center!()))
        }
        (CrossAlign::Center, AlignContentMode::SpaceBetween) => {
            maybe_rev!(add!(line_position!(), ac_space_between!(), wl_center!()))
        }
        (CrossAlign::Center, AlignContentMode::SpaceAround) => {
            maybe_rev!(add!(line_position!(), ac_space_around!(), wl_center!()))
        }
        (CrossAlign::Center, AlignContentMode::SpaceEvenly) => {
            maybe_rev!(add!(line_position!(), ac_space_evenly!(), wl_center!()))
        }
        (CrossAlign::Baseline, AlignContentMode::FlexStart) => {
            maybe_rev!(add!(line_position!(), wl_baseline!()))
        }
        (CrossAlign::Baseline, AlignContentMode::Stretch) => {
            maybe_rev!(add!(line_position!(), ac_stretch!(), wl_baseline!()))
        }
        (CrossAlign::Baseline, AlignContentMode::FlexEnd) => {
            maybe_rev!(add!(line_position!(), ac_flex_end!(), wl_baseline!()))
        }
        (CrossAlign::Baseline, AlignContentMode::Center) => {
            maybe_rev!(add!(line_position!(), ac_center!(), wl_baseline!()))
        }
        (CrossAlign::Baseline, AlignContentMode::SpaceBetween) => {
            maybe_rev!(add!(line_position!(), ac_space_between!(), wl_baseline!()))
        }
        (CrossAlign::Baseline, AlignContentMode::SpaceAround) => {
            maybe_rev!(add!(line_position!(), ac_space_around!(), wl_baseline!()))
        }
        (CrossAlign::Baseline, AlignContentMode::SpaceEvenly) => {
            maybe_rev!(add!(line_position!(), ac_space_evenly!(), wl_baseline!()))
        }
    }
}

// ============================================================================
// Content-based sizing (avoids size_query to prevent recursion)
// ============================================================================

/// Get content-based size for a node without going through size_query.
///
/// For flex containers, returns the container's auto main/cross size formula
/// so that nested flex containers correctly report their content width.
fn content_based_size(styler: &dyn StylerAccess, axis: Axis) -> Option<&'static Formula> {
    if styler.is_intrinsic() {
        return Some(match axis {
            Axis::Horizontal => inline_width!(),
            Axis::Vertical => inline_height!(),
        });
    }

    // If this node is itself a flex container, compute its auto size
    // using flex layout (sum of children bases + gaps) rather than
    // falling through to block sizing.
    if let Some(super::DisplayType::Flex(dir)) = super::DisplayType::of_element(styler) {
        return Some(flex_size(dir, axis, styler));
    }

    // For block-level elements used as flex items, the horizontal
    // content-based size is the max-content width (the width the
    // element needs to display its content without wrapping) plus
    // padding and border.  block_size() would return "fill parent",
    // which is wrong for flex-basis auto.
    match axis {
        Axis::Horizontal => Some(add!(
            max_content_width!(),
            css_prop!(PaddingLeft),
            css_prop!(PaddingRight),
            css_prop!(BorderLeftWidth),
            css_prop!(BorderRightWidth),
        )),
        Axis::Vertical => Some(super::block::block_size(styler, axis)),
    }
}
