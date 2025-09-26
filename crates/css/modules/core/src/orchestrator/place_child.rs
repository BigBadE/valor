// Per-child placement composite
// Wraps §10.3.3 (horizontal/position), §10.6.3 (heights/margins), rect insertion and logging.

use crate::chapter10::part_10_1_containing_block as cb10;
use crate::{
    ChildLayoutCtx, CollapsedPos, HeightsAndMargins, HeightsCtx, LayoutRect, Layouter, VertCommit,
};
use css_box::compute_box_sides;
use css_orchestrator::style_model::ComputedStyle;
use js::NodeKey;
use log::debug;

#[inline]
/// Place a single block-level child and return `(computed_height, child_y, margin_bottom_out)`.
pub fn place_child_public(
    layouter: &mut Layouter,
    child_key: NodeKey,
    ctx: ChildLayoutCtx,
) -> (i32, i32, i32) {
    let has_style = layouter.computed_styles.contains_key(&child_key);
    debug!("[LAYOUT][DIAG] child={child_key:?} has_computed_style={has_style}");
    let style = layouter
        .computed_styles
        .get(&child_key)
        .cloned()
        .unwrap_or_else(ComputedStyle::default);
    let sides = compute_box_sides(&style);
    let CollapsedPos {
        margin_top_eff,
        collapsed_top,
        used_bb_w,
        child_x,
        child_y,
        x_adjust,
        y_adjust,
    } = layouter.compute_collapsed_and_position(child_key, &ctx, &style, &sides);

    let HeightsAndMargins {
        computed_h,
        eff_bottom,
        is_empty,
        margin_bottom_out,
    } = layouter.compute_heights_and_margins(HeightsCtx {
        child_key,
        style: &style,
        sides,
        child_x,
        child_y,
        used_bb_w,
        ctx: &ctx,
        margin_top_eff,
    });

    debug!(
        "[VERT child place idx={}] first={} ancestor_applied_at_edge_for_children={} mt_raw={} mt_eff={} collapsed_top={} is_empty={} parent_origin_y={} y_cursor_in={} -> y={} mb_out={} lt_applied={}",
        ctx.index,
        ctx.is_first_placed,
        ctx.ancestor_applied_at_edge_for_children,
        sides.margin_top,
        margin_top_eff,
        collapsed_top,
        is_empty,
        cb10::parent_content_origin(&ctx.metrics).1,
        ctx.y_cursor,
        child_y,
        margin_bottom_out,
        ctx.leading_top_applied
    );

    layouter.commit_vert(VertCommit {
        index: ctx.index,
        prev_mb: ctx.previous_bottom_margin,
        margin_top_raw: sides.margin_top,
        margin_top_eff,
        eff_bottom,
        is_empty,
        collapsed_top,
        parent_origin_y: cb10::parent_content_origin(&ctx.metrics).1,
        y_position: child_y,
        y_cursor_in: ctx.y_cursor,
        leading_top_applied: if ctx.index == 0 {
            ctx.leading_top_applied
        } else {
            0
        },
        child_key,
        rect: LayoutRect {
            x: child_x.saturating_add(x_adjust),
            y: child_y.saturating_add(y_adjust),
            width: used_bb_w,
            height: computed_h,
        },
    });

    (computed_h, child_y, margin_bottom_out)
}
