//! Block layout formulas.
//!
//! Block elements fill their parent's content box horizontally and stack
//! vertically. Positions account for margins, and sizes account for
//! padding and border.

use lightningcss::properties::PropertyId;
use lightningcss::values::length::LengthPercentageOrAuto;
use rewrite_core::{Axis, Formula, NodeId, PropertyResolver, Subpixel};

use super::size::{content_size_query, margin_box_size_query, size_query};

// ============================================================================
// Layout participation helpers
// ============================================================================

/// Check if a node participates in layout (is a visible element, not text or display:none).
fn participates_in_layout(id: NodeId, ctx: &dyn PropertyResolver) -> bool {
    // Must be a DOM element (not text, comment, or document node).
    if !ctx.is_element(id) {
        return false;
    }
    // display:none elements don't participate in layout.
    !matches!(
        ctx.get_css_property(id, &PropertyId::Display),
        Some(lightningcss::properties::Property::Display(
            lightningcss::properties::display::Display::Keyword(
                lightningcss::properties::display::DisplayKeyword::None,
            ),
        ))
    )
}

// ============================================================================
// Margin auto detection helpers
// ============================================================================

fn is_margin_auto(node: NodeId, ctx: &dyn PropertyResolver, prop_id: &PropertyId<'static>) -> bool {
    match ctx.get_css_property(node, prop_id) {
        Some(lightningcss::properties::Property::MarginLeft(LengthPercentageOrAuto::Auto))
        | Some(lightningcss::properties::Property::MarginRight(LengthPercentageOrAuto::Auto))
        | Some(lightningcss::properties::Property::MarginTop(LengthPercentageOrAuto::Auto))
        | Some(lightningcss::properties::Property::MarginBottom(LengthPercentageOrAuto::Auto)) => {
            true
        }
        _ => false,
    }
}

fn has_explicit_width(node: NodeId, ctx: &dyn PropertyResolver) -> bool {
    ctx.get_css_property(node, &PropertyId::Width).is_some()
}

// ============================================================================
// Size formulas
// ============================================================================

/// Compute block size formula for the given axis.
pub fn block_size(node: NodeId, ctx: &dyn PropertyResolver, axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => block_width(node, ctx),
        Axis::Vertical => block_height(node, ctx),
    }
}

fn block_width(node: NodeId, ctx: &dyn PropertyResolver) -> &'static Formula {
    if super::is_block_in_inline(node, ctx) {
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
fn inline_main_size_query(node: NodeId, ctx: &dyn PropertyResolver) -> Option<&'static Formula> {
    if ctx.is_intrinsic(node) {
        return Some(inline_width!());
    }
    if matches!(
        super::DisplayType::of_element(node, ctx),
        Some(super::DisplayType::Inline)
    ) {
        return Some(inline_width!());
    }
    // Block-level child: return None to force line break.
    None
}

/// Per-child height query for block containers.
fn block_child_height_query(
    node: NodeId,
    ctx: &dyn PropertyResolver,
) -> Option<&'static Formula> {
    if ctx.is_intrinsic(node) {
        return Some(inline_height!());
    }
    if matches!(
        super::DisplayType::of_element(node, ctx),
        Some(super::DisplayType::Inline)
    ) {
        return Some(inline_height!());
    }
    collapsed_margin_box_height(node, ctx)
}

/// Height contribution of a block child, accounting for sibling margin collapse.
fn collapsed_margin_box_height(
    node: NodeId,
    ctx: &dyn PropertyResolver,
) -> Option<&'static Formula> {
    let prev = ctx
        .prev_siblings(node)
        .into_iter()
        .find(|&id| participates_in_layout(id, ctx))
        .unwrap_or(node);
    if prev == node {
        // First child: full margin-box.
        return margin_box_size_query(node, ctx, Axis::Vertical);
    }

    Some(add!(
        related!(Self_, size_query, Axis::Vertical),
        css_prop!(MarginBottom),
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
                    fn wrap(
                        node: NodeId,
                        ctx: &dyn rewrite_core::PropertyResolver,
                    ) -> Option<&'static Formula> {
                        super::size::size_query(node, ctx, Axis::Horizontal)
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

fn block_height(node: NodeId, ctx: &dyn PropertyResolver) -> &'static Formula {
    let collapse_top = has_collapsing_first_child(node, ctx);
    let collapse_bottom = has_collapsing_last_child(node, ctx);

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
pub fn block_offset(node: NodeId, ctx: &dyn PropertyResolver, axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => block_offset_x(node, ctx),
        Axis::Vertical => block_offset_y(node, ctx),
    }
}

fn is_element_node(node: NodeId, ctx: &dyn PropertyResolver) -> bool {
    ctx.is_element(node)
}

fn has_bfc_overflow(node: NodeId, ctx: &dyn PropertyResolver) -> bool {
    if let Some(overflow) = ctx.get_css_property(node, &PropertyId::OverflowY) {
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

fn establishes_formatting_context(node: NodeId, ctx: &dyn PropertyResolver) -> bool {
    matches!(
        super::DisplayType::of_element(node, ctx),
        Some(super::DisplayType::Flex(_, _) | super::DisplayType::Grid)
    )
}

fn prevents_top_margin_collapse(node: NodeId, ctx: &dyn PropertyResolver) -> bool {
    let has_padding = ctx
        .get_property(node, &PropertyId::PaddingTop)
        .is_some_and(|v| v != Subpixel::ZERO);
    let has_border = ctx
        .get_property(node, &PropertyId::BorderTopWidth)
        .is_some_and(|v| v != Subpixel::ZERO);
    has_padding || has_border || has_bfc_overflow(node, ctx) || establishes_formatting_context(node, ctx)
}

fn has_collapsing_first_child(node: NodeId, ctx: &dyn PropertyResolver) -> bool {
    if prevents_top_margin_collapse(node, ctx) {
        return false;
    }
    let children = ctx.children(node);
    children.iter().any(|&child| is_element_node(child, ctx))
}

fn prevents_bottom_margin_collapse(node: NodeId, ctx: &dyn PropertyResolver) -> bool {
    if ctx.get_css_property(node, &PropertyId::Height).is_some() {
        return true;
    }
    let has_padding = ctx
        .get_property(node, &PropertyId::PaddingBottom)
        .is_some_and(|v| v != Subpixel::ZERO);
    let has_border = ctx
        .get_property(node, &PropertyId::BorderBottomWidth)
        .is_some_and(|v| v != Subpixel::ZERO);
    has_padding || has_border || has_bfc_overflow(node, ctx)
}

fn has_collapsing_last_child(node: NodeId, ctx: &dyn PropertyResolver) -> bool {
    if prevents_bottom_margin_collapse(node, ctx) {
        return false;
    }
    let children = ctx.children(node);
    children.iter().any(|&child| is_element_node(child, ctx))
}

fn last_child_margin_bottom_query(
    node: NodeId,
    ctx: &dyn PropertyResolver,
    _axis: Axis,
) -> Option<&'static Formula> {
    if !is_element_node(node, ctx) {
        return None;
    }
    let next = ctx.next_siblings(node);
    let has_next_element = next.iter().any(|&sib| is_element_node(sib, ctx));
    if has_next_element {
        return None;
    }
    Some(css_prop!(MarginBottom))
}

fn effective_margin_top_query(
    node: NodeId,
    ctx: &dyn PropertyResolver,
    _axis: Axis,
) -> Option<&'static Formula> {
    if has_collapsing_first_child(node, ctx) {
        Some(max!(
            css_prop!(MarginTop),
            aggregate!(Max, Children, first_child_margin_top_query, Axis::Vertical),
        ))
    } else {
        Some(css_prop!(MarginTop))
    }
}

fn first_child_margin_top_query(
    node: NodeId,
    ctx: &dyn PropertyResolver,
    _axis: Axis,
) -> Option<&'static Formula> {
    if !is_element_node(node, ctx) {
        return None;
    }
    let prev = ctx
        .prev_siblings(node)
        .into_iter()
        .find(|&id| participates_in_layout(id, ctx))
        .unwrap_or(node);
    if prev != node {
        return None; // Not the first child
    }
    effective_margin_top_query(node, ctx, Axis::Vertical)
}

fn block_offset_x(node: NodeId, ctx: &dyn PropertyResolver) -> &'static Formula {
    let ml_auto = is_margin_auto(node, ctx, &PropertyId::MarginLeft);
    let mr_auto = is_margin_auto(node, ctx, &PropertyId::MarginRight);
    let has_width = has_explicit_width(node, ctx);

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

fn block_offset_y(node: NodeId, ctx: &dyn PropertyResolver) -> &'static Formula {
    let prev = ctx
        .prev_siblings(node)
        .into_iter()
        .find(|&id| participates_in_layout(id, ctx))
        .unwrap_or(node);
    if prev == node {
        let parent = ctx.parent(node).unwrap_or(NodeId(0));
        if prevents_top_margin_collapse(parent, ctx) {
            return add!(
                related!(Self_, effective_margin_top_query, Axis::Vertical),
                aggregate!(Sum, PrevSiblings, margin_box_size_query, Axis::Vertical),
            );
        }
        return aggregate!(Sum, PrevSiblings, margin_box_size_query, Axis::Vertical);
    }

    let parent = ctx.parent(node).unwrap_or(NodeId(0));
    let parent_collapses = !prevents_top_margin_collapse(parent, ctx);

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
