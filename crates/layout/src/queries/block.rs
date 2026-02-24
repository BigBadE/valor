//! Block layout formulas.
//!
//! Block elements fill their parent's content box horizontally and stack
//! vertically. Positions account for margins, and sizes account for
//! padding and border.
//!
//! Auto margins (CSS 2.2 §10.3.3): when a block has an explicit width and
//! one or both horizontal margins are `auto`, the remaining space is
//! distributed to the auto margin(s).

use lightningcss::properties::PropertyId;
use lightningcss::values::length::LengthPercentageOrAuto;
use rewrite_core::{Axis, Formula, SingleRelationship, StylerAccess, Subpixel};

use super::size::{content_size_query, margin_box_size_query, size_query};

// ============================================================================
// Margin auto detection helpers
// ============================================================================

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

fn has_explicit_width(styler: &dyn StylerAccess) -> bool {
    styler.get_css_property(&PropertyId::Width).is_some()
}

// ============================================================================
// Size formulas
// ============================================================================

/// Compute block size formula for the given axis.
pub fn block_size(styler: &dyn StylerAccess, axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => block_width(styler),
        Axis::Vertical => block_height(styler),
    }
}

fn block_width(styler: &dyn StylerAccess) -> &'static Formula {
    // Block-in-inline: size relative to the nearest block container,
    // not the inline parent (CSS 2.2 §9.2.1.1).
    if super::is_block_in_inline(styler) {
        return sub!(
            related!(BlockContainer, content_size_query, Axis::Horizontal),
            css_prop!(MarginLeft),
            css_prop!(MarginRight),
        );
    }

    sub!(
        related!(Parent, content_size_query, Axis::Horizontal),
        css_prop!(MarginLeft),
        css_prop!(MarginRight),
    )
}

/// Per-child main-axis size query for inline line-breaking.
///
/// Returns `InlineWidth` for inline-level children (for line-breaking
/// decisions), `None` for block-level children (forces a line break
/// in the `LineAggregate` resolver).
fn inline_main_size_query(styler: &dyn StylerAccess) -> Option<&'static Formula> {
    if styler.is_intrinsic() {
        return Some(inline_width!());
    }
    if matches!(
        super::DisplayType::of_element(styler),
        Some(super::DisplayType::Inline)
    ) {
        return Some(inline_width!());
    }
    // Block-level child: return None to force line break.
    None
}

/// Per-child height query for block containers.
///
/// Returns `InlineHeight` for inline-level children so their height
/// is measured via text measurement. Returns collapsed-margin-aware
/// height for block-level children so they stack vertically with
/// proper margin collapsing between adjacent siblings.
fn block_child_height_query(styler: &dyn StylerAccess) -> Option<&'static Formula> {
    // Intrinsic nodes (text, replaced elements) are always inline content.
    if styler.is_intrinsic() {
        return Some(inline_height!());
    }
    // Inline elements are inline content.
    if matches!(
        super::DisplayType::of_element(styler),
        Some(super::DisplayType::Inline)
    ) {
        return Some(inline_height!());
    }
    // Block-level children use height with sibling margin collapse.
    collapsed_margin_box_height(styler)
}

/// Height contribution of a block child, accounting for sibling margin collapse.
///
/// For the first child: margin-box height (mt + height + mb)
/// For subsequent children: height + collapsed(prev.mb, this.mt) + mb
///   where collapsed margin replaces the raw mt to avoid double-counting.
///
/// The collapsed margin formula handles positive/negative margins per CSS spec:
///   collapsed(a, b) = max(max(a, b), 0) + min(min(a, b), 0)
fn collapsed_margin_box_height(styler: &dyn StylerAccess) -> Option<&'static Formula> {
    let prev = styler.related(SingleRelationship::PrevSibling);
    if prev.node_id() == styler.node_id() {
        // First child: full margin-box.
        return margin_box_size_query(styler, Axis::Vertical);
    }

    // Non-first child: replace mt with (collapsed_margin - prev.mb).
    // The sum includes prev.mb already, so our contribution is:
    //   height + mb + collapsed(prev.mb, this.mt) - prev.mb
    //
    // Equivalently: height + mb + extra, where:
    //   extra = max(max(prev.mb, mt), 0) + min(min(prev.mb, mt), 0) - prev.mb
    //
    // When both positive: extra = max(prev.mb, mt) - prev.mb = max(0, mt - prev.mb)
    // When both negative: extra = min(prev.mb, mt) - prev.mb = min(0, mt - prev.mb)
    // When mixed: extra = prev.mb + mt - prev.mb = mt (or similar)
    //
    // Simplified: contribution = height + mb + max(0, mt - prev.mb) for positive case.
    // For full spec compliance with negative margins, use the full formula.
    Some(add!(
        related!(Self_, size_query, Axis::Vertical),
        css_prop!(MarginBottom),
        // collapsed(prev.mb, mt) - prev.mb
        sub!(
            add!(
                max!(
                    max!(
                        related_val!(PrevSibling, css_prop!(MarginBottom)),
                        css_prop!(MarginTop),
                    ),
                    constant!(Subpixel::ZERO),
                ),
                min!(
                    min!(
                        related_val!(PrevSibling, css_prop!(MarginBottom)),
                        css_prop!(MarginTop),
                    ),
                    constant!(Subpixel::ZERO),
                ),
            ),
            related_val!(PrevSibling, css_prop!(MarginBottom)),
        ),
    ))
}

/// Available content width formula for the container (used for inline line-breaking).
static CONTENT_WIDTH: Formula = Formula::BinOp(
    rewrite_core::Operation::Sub,
    &Formula::BinOp(
        rewrite_core::Operation::Sub,
        &Formula::BinOp(
            rewrite_core::Operation::Sub,
            &Formula::BinOp(
                rewrite_core::Operation::Sub,
                &Formula::Related(rewrite_core::SingleRelationship::Self_, {
                    fn wrap(sty: &dyn StylerAccess) -> Option<&'static Formula> {
                        super::size::size_query(sty, Axis::Horizontal)
                    }
                    wrap as rewrite_core::QueryFn
                }),
                &Formula::CssValueOrDefault(PropertyId::PaddingLeft, Subpixel::ZERO),
            ),
            &Formula::CssValueOrDefault(PropertyId::PaddingRight, Subpixel::ZERO),
        ),
        &Formula::CssValueOrDefault(PropertyId::BorderLeftWidth, Subpixel::ZERO),
    ),
    &Formula::CssValueOrDefault(PropertyId::BorderRightWidth, Subpixel::ZERO),
);

/// Zero gap formula (inline has no gap between items).
static ZERO_GAP: Formula = Formula::Constant(Subpixel::ZERO);

/// Children height using line-breaking aggregation.
///
/// Groups inline children into line boxes (breaking when width exceeds
/// available), takes max height per line, sums across lines. Block
/// children force line breaks and contribute their margin-box height.
macro_rules! children_height_formula {
    () => {
        line_aggregate!(
            line_agg: Sum,
            within_line_agg: Max,
            item_main_size: inline_main_size_query,
            item_value: block_child_height_query,
            available_main: &CONTENT_WIDTH,
            gap: &ZERO_GAP,
            line_gap: &ZERO_GAP,
        )
    };
}

fn block_height(styler: &dyn StylerAccess) -> &'static Formula {
    let collapse_top = has_collapsing_first_child(styler);
    let collapse_bottom = has_collapsing_last_child(styler);

    match (collapse_top, collapse_bottom) {
        (true, true) => add!(
            sub!(
                children_height_formula!(),
                aggregate!(Max, Children, first_child_margin_top_query, Axis::Vertical),
                aggregate!(
                    Max,
                    Children,
                    last_child_margin_bottom_query,
                    Axis::Vertical
                ),
            ),
            css_prop!(PaddingTop),
            css_prop!(PaddingBottom),
            css_prop!(BorderTopWidth),
            css_prop!(BorderBottomWidth),
        ),
        (true, false) => add!(
            sub!(
                children_height_formula!(),
                aggregate!(Max, Children, first_child_margin_top_query, Axis::Vertical),
            ),
            css_prop!(PaddingTop),
            css_prop!(PaddingBottom),
            css_prop!(BorderTopWidth),
            css_prop!(BorderBottomWidth),
        ),
        (false, true) => add!(
            sub!(
                children_height_formula!(),
                aggregate!(
                    Max,
                    Children,
                    last_child_margin_bottom_query,
                    Axis::Vertical
                ),
            ),
            css_prop!(PaddingTop),
            css_prop!(PaddingBottom),
            css_prop!(BorderTopWidth),
            css_prop!(BorderBottomWidth),
        ),
        (false, false) => add!(
            children_height_formula!(),
            css_prop!(PaddingTop),
            css_prop!(PaddingBottom),
            css_prop!(BorderTopWidth),
            css_prop!(BorderBottomWidth),
        ),
    }
}

// ============================================================================
// Offset formulas
// ============================================================================

/// Compute block offset formula.
pub fn block_offset(styler: &dyn StylerAccess, axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => block_offset_x(styler),
        Axis::Vertical => block_offset_y(styler),
    }
}

/// Check if a node is a real element (has a Display property).
/// Whitespace-only text nodes are neither intrinsic (no visible text)
/// nor elements (no Display), so we need this explicit check for
/// margin collapsing where only elements participate.
fn is_element_node(styler: &dyn StylerAccess) -> bool {
    styler.get_css_property(&PropertyId::Display).is_some()
}

/// Check if overflow establishes a BFC (anything other than visible).
fn has_bfc_overflow(styler: &dyn StylerAccess) -> bool {
    if let Some(overflow) = styler.get_css_property(&PropertyId::OverflowY) {
        use lightningcss::properties::overflow::OverflowKeyword;
        if matches!(
            &overflow,
            lightningcss::properties::Property::OverflowY(kw)
                if !matches!(kw, OverflowKeyword::Visible)
        ) {
            return true;
        }
    }
    false
}

/// Check if display type establishes a new formatting context (flex, grid).
/// Flex and grid containers establish a new BFC for their contents,
/// preventing margin collapsing with ancestors.
fn establishes_formatting_context(styler: &dyn StylerAccess) -> bool {
    matches!(
        super::DisplayType::of_element(styler),
        Some(super::DisplayType::Flex(_, _) | super::DisplayType::Grid)
    )
}

/// Check if a block element prevents top margin collapsing with its first child.
/// Per CSS 2.2 §8.3.1: no collapsing if parent has padding-top, border-top,
/// or establishes a new BFC (overflow != visible, flex, grid).
fn prevents_top_margin_collapse(styler: &dyn StylerAccess) -> bool {
    let has_padding = styler
        .get_property(&PropertyId::PaddingTop)
        .is_some_and(|v| v != Subpixel::ZERO);
    let has_border = styler
        .get_property(&PropertyId::BorderTopWidth)
        .is_some_and(|v| v != Subpixel::ZERO);
    has_padding || has_border || has_bfc_overflow(styler) || establishes_formatting_context(styler)
}

/// Check if a block element should have parent-child margin collapsing
/// with its first child: no padding-top, no border-top, no BFC establishment,
/// and has element children.
///
/// Per CSS 2.2 §8.3.1: margins do not collapse if the parent establishes
/// a new block formatting context (e.g. overflow != visible).
fn has_collapsing_first_child(styler: &dyn StylerAccess) -> bool {
    if prevents_top_margin_collapse(styler) {
        return false;
    }

    // Check if there are any element children at all.
    // We always collapse the first child's top margin when structural
    // conditions are met, since the formula will resolve to zero
    // subtraction if the margin is actually zero.
    use rewrite_core::MultiRelationship;
    let children = styler.related_iter(MultiRelationship::Children);
    children.iter().any(|child| is_element_node(child.as_ref()))
}

/// Check if a block element prevents bottom margin collapsing with its last child.
/// Per CSS 2.2 §8.3.1: no collapsing if parent has padding-bottom, border-bottom,
/// or establishes a new BFC. Additionally requires 'auto' computed height
/// (i.e., no explicit height set).
fn prevents_bottom_margin_collapse(styler: &dyn StylerAccess) -> bool {
    // Explicit height prevents bottom collapse per CSS 2.2 §8.3.1
    if styler.get_css_property(&PropertyId::Height).is_some() {
        return true;
    }
    let has_padding = styler
        .get_property(&PropertyId::PaddingBottom)
        .is_some_and(|v| v != Subpixel::ZERO);
    let has_border = styler
        .get_property(&PropertyId::BorderBottomWidth)
        .is_some_and(|v| v != Subpixel::ZERO);
    has_padding || has_border || has_bfc_overflow(styler)
}

/// Check if a block element should have parent-child margin collapsing
/// with its last child: no padding-bottom, no border-bottom, no BFC,
/// auto height, and has element children.
fn has_collapsing_last_child(styler: &dyn StylerAccess) -> bool {
    if prevents_bottom_margin_collapse(styler) {
        return false;
    }
    use rewrite_core::MultiRelationship;
    let children = styler.related_iter(MultiRelationship::Children);
    children.iter().any(|child| is_element_node(child.as_ref()))
}

/// Query that returns the last element child's margin-bottom for
/// parent-child bottom collapse. Returns None for non-last or non-element children.
fn last_child_margin_bottom_query(
    styler: &dyn StylerAccess,
    _axis: Axis,
) -> Option<&'static Formula> {
    // Only real elements participate in margin collapse.
    if !is_element_node(styler) {
        return None;
    }
    // Check if this is the last element child by looking at next siblings.
    use rewrite_core::MultiRelationship;
    let next = styler.related_iter(MultiRelationship::NextSiblings);
    let has_next_element = next.iter().any(|sib| is_element_node(sib.as_ref()));
    if has_next_element {
        return None; // Not the last element child
    }
    Some(css_prop!(MarginBottom))
}

/// Query that returns this element's effective margin-top,
/// accounting for parent-child margin collapse. If this element
/// has a first child whose margin collapses through, the effective
/// margin is max(self.mt, first_child.mt).
fn effective_margin_top_query(styler: &dyn StylerAccess, _axis: Axis) -> Option<&'static Formula> {
    if has_collapsing_first_child(styler) {
        Some(max!(
            css_prop!(MarginTop),
            aggregate!(Max, Children, first_child_margin_top_query, Axis::Vertical),
        ))
    } else {
        Some(css_prop!(MarginTop))
    }
}

/// Query that returns the first element child's effective margin-top for
/// parent-child collapse. Returns None for non-first or non-element children.
/// Uses effective_margin_top_query to handle recursive collapse chains
/// (e.g., grandchild margin collapsing through child and parent).
fn first_child_margin_top_query(
    styler: &dyn StylerAccess,
    _axis: Axis,
) -> Option<&'static Formula> {
    // Only real elements participate in margin collapse.
    if !is_element_node(styler) {
        return None;
    }
    // Only the first element child participates in parent-child collapse.
    let prev = styler.related(SingleRelationship::PrevSibling);
    if prev.node_id() != styler.node_id() {
        return None; // Not the first child
    }
    // Return the effective margin (which may include this child's own
    // first child's margin, recursively).
    effective_margin_top_query(styler, Axis::Vertical)
}

/// Block horizontal offset with auto margin support.
fn block_offset_x(styler: &dyn StylerAccess) -> &'static Formula {
    let ml_auto = is_margin_auto(styler, &PropertyId::MarginLeft);
    let mr_auto = is_margin_auto(styler, &PropertyId::MarginRight);
    let has_width = has_explicit_width(styler);

    if has_width {
        match (ml_auto, mr_auto) {
            (true, true) => {
                return div!(
                    sub!(
                        related!(Parent, content_size_query, Axis::Horizontal),
                        css_val!(Width),
                    ),
                    constant!(Subpixel::raw(2)),
                );
            }
            (true, false) => {
                return sub!(
                    related!(Parent, content_size_query, Axis::Horizontal),
                    css_val!(Width),
                    css_prop!(MarginRight),
                );
            }
            (false, true) => {
                return css_prop!(MarginLeft);
            }
            (false, false) => {}
        }
    }

    css_prop!(MarginLeft)
}

/// Block vertical offset with sibling margin collapsing.
///
/// When there is no previous element sibling:
///   y = margin_top + sum(prev_siblings.margin_box_size)
///
/// When there is a previous element sibling, adjacent margins collapse:
///   y = sum(prev_siblings.margin_box_size) - prev.mb + collapsed(prev.mb, this.mt)
///
/// The collapsed margin follows the CSS spec:
///   - Both positive: max(a, b)
///   - Both negative: min(a, b) (most negative)
///   - Mixed: a + b
///   = max(max(a, b), 0) + min(min(a, b), 0)
fn block_offset_y(styler: &dyn StylerAccess) -> &'static Formula {
    let prev = styler.related(SingleRelationship::PrevSibling);
    if prev.node_id() == styler.node_id() {
        // No previous element sibling — check for parent-child margin collapse.
        let parent = styler.related(SingleRelationship::Parent);
        if prevents_top_margin_collapse(parent.as_ref()) {
            // Parent prevents collapse — use effective margin normally.
            return add!(
                related!(Self_, effective_margin_top_query, Axis::Vertical),
                aggregate!(Sum, PrevSiblings, margin_box_size_query, Axis::Vertical),
            );
        }

        // Parent-child collapse: child's margin collapses through the parent.
        // The child contributes 0 local offset (its margin is applied at parent).
        return aggregate!(Sum, PrevSiblings, margin_box_size_query, Axis::Vertical);
    }

    // Has a previous sibling — apply margin collapsing.
    // Start with sum of prev siblings' margin boxes (includes prev.mb),
    // subtract prev.mb, then add the collapsed margin.
    //
    // Use effective margin-top (which accounts for parent-child collapse
    // with this element's first child, if applicable).
    //
    // collapsed(a, b) = max(max(a, b), 0) + min(min(a, b), 0)
    //
    // When the parent allows parent-child margin collapse with its first
    // child, the first child's margin-top is absorbed by the parent's
    // position. We must subtract it from the sum of previous siblings'
    // margin boxes so subsequent siblings don't double-count that space.
    let parent = styler.related(SingleRelationship::Parent);
    let parent_collapses = !prevents_top_margin_collapse(parent.as_ref());

    if parent_collapses {
        add!(
            sub!(
                aggregate!(Sum, PrevSiblings, margin_box_size_query, Axis::Vertical),
                related_val!(PrevSibling, css_prop!(MarginBottom)),
                aggregate!(
                    Max,
                    PrevSiblings,
                    first_child_margin_top_query,
                    Axis::Vertical
                ),
            ),
            max!(
                max!(
                    related_val!(PrevSibling, css_prop!(MarginBottom)),
                    related!(Self_, effective_margin_top_query, Axis::Vertical),
                ),
                constant!(Subpixel::ZERO),
            ),
            min!(
                min!(
                    related_val!(PrevSibling, css_prop!(MarginBottom)),
                    related!(Self_, effective_margin_top_query, Axis::Vertical),
                ),
                constant!(Subpixel::ZERO),
            ),
        )
    } else {
        add!(
            sub!(
                aggregate!(Sum, PrevSiblings, margin_box_size_query, Axis::Vertical),
                related_val!(PrevSibling, css_prop!(MarginBottom)),
            ),
            max!(
                max!(
                    related_val!(PrevSibling, css_prop!(MarginBottom)),
                    related!(Self_, effective_margin_top_query, Axis::Vertical),
                ),
                constant!(Subpixel::ZERO),
            ),
            min!(
                min!(
                    related_val!(PrevSibling, css_prop!(MarginBottom)),
                    related!(Self_, effective_margin_top_query, Axis::Vertical),
                ),
                constant!(Subpixel::ZERO),
            ),
        )
    }
}
