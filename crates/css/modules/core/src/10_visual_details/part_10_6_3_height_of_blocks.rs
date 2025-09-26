//! Spec: CSS 2.2 §10.6.3 The height of blocks
//! Root height and used height computations.

use css_orchestrator::style_model::ComputedStyle;
use css_text::default_line_height_px;
use js::NodeKey;

use crate::chapter8::part_8_3_1_collapsing_margins as cm83;
use crate::chapter9::part_9_4_1_block_formatting_context::establishes_block_formatting_context;
use crate::{
    ChildContentCtx, HeightExtras, HeightsAndMargins, HeightsCtx, HorizontalEdges, Layouter,
    RootHeightsCtx, TopEdges,
};

/// Compute content height and root border-box height.
#[inline]
pub fn compute_root_heights(layouter: &Layouter, ctx: RootHeightsCtx) -> (i32, i32) {
    let content_origin = ctx
        .root_y
        .saturating_add(ctx.metrics.border_top)
        .saturating_add(ctx.metrics.padding_top);
    let root_style = layouter
        .computed_styles
        .get(&ctx.root)
        .cloned()
        .unwrap_or_else(ComputedStyle::default);
    let padding_bottom = root_style.padding.bottom.max(0.0f32) as i32;
    let border_bottom = root_style.border_width.bottom.max(0.0f32) as i32;
    let bottom_edge_collapsible = padding_bottom == 0i32
        && border_bottom == 0i32
        && !establishes_block_formatting_context(&root_style);
    let content_height = ctx.content_bottom.map_or(0i32, |bottom_value| {
        bottom_value.saturating_sub(content_origin).max(0i32)
    });

    log::debug!(
        "[ROOT-HEIGHT] root={:?} origin_y={} content_bottom={:?} last_pos_mb={} bottom_edge_collapsible={} pb={} bb={} -> content_h={}",
        ctx.root,
        content_origin,
        ctx.content_bottom,
        ctx.root_last_pos_mb,
        bottom_edge_collapsible,
        padding_bottom,
        border_bottom,
        content_height
    );
    let root_height_border_box = content_height
        .saturating_add(ctx.metrics.padding_top)
        .saturating_add(padding_bottom)
        .saturating_add(ctx.metrics.border_top)
        .saturating_add(border_bottom)
        .max(0i32);
    (content_height, root_height_border_box)
}

/// Composite: compute heights and outgoing margins for a child.
/// Mirrors the former lib.rs logic, exposed here to keep lib.rs thin.
#[inline]
pub fn compute_heights_and_margins_public(
    layouter: &mut Layouter,
    hctx: HeightsCtx<'_>,
) -> HeightsAndMargins {
    let (content_h_inner, _last_out_mb) = layouter.compute_child_content_height(ChildContentCtx {
        key: hctx.child_key,
        used_border_box_width: hctx.used_bb_w,
        sides: hctx.sides,
        x: hctx.child_x,
        y: hctx.child_y,
        ancestor_applied_at_edge: hctx.ctx.ancestor_applied_at_edge_for_children,
    });
    let content_h = content_h_inner;
    let computed_h = compute_used_height(
        layouter,
        hctx.style,
        hctx.child_key,
        HeightExtras {
            padding_top: hctx.sides.padding_top,
            padding_bottom: hctx.sides.padding_bottom,
            border_top: hctx.sides.border_top,
            border_bottom: hctx.sides.border_bottom,
        },
        content_h,
    );
    let eff_bottom =
        cm83::effective_child_bottom_margin_public(layouter, hctx.child_key, &hctx.sides);
    let is_empty =
        layouter.is_effectively_empty_box(hctx.style, &hctx.sides, computed_h, hctx.child_key);
    let margin_bottom_out = if is_empty && hctx.ctx.is_first_placed {
        cm83::compute_first_placed_empty_margin_bottom(
            hctx.ctx.previous_bottom_margin,
            hctx.ctx.parent_self_top_margin,
            hctx.margin_top_eff,
            eff_bottom,
        )
    } else {
        cm83::compute_margin_bottom_out(hctx.margin_top_eff, eff_bottom, is_empty)
    };
    HeightsAndMargins {
        computed_h,
        eff_bottom,
        is_empty,
        margin_bottom_out,
    }
}

/// Compute used height for a block child, applying box extras when height is auto and
/// falling back to a single line height if there is inline text and overall height is 0.
#[inline]
pub fn compute_used_height(
    layouter: &Layouter,
    style: &ComputedStyle,
    child_key: NodeKey,
    extras: HeightExtras,
    child_content_height: i32,
) -> i32 {
    // Inline: used_border_box_height (moved from sizing.rs). Spec: CSS 2.2 §10.6.3 + Box Sizing L3
    #[inline]
    fn sum_vertical(style: &ComputedStyle) -> i32 {
        let pad = style.padding.top.max(0.0f32) + style.padding.bottom.max(0.0f32);
        let border = style.border_width.top.max(0.0f32) + style.border_width.bottom.max(0.0f32);
        (pad + border) as i32
    }
    #[inline]
    fn used_border_box_height(style: &ComputedStyle) -> i32 {
        use css_orchestrator::style_model::BoxSizing;
        let extras = sum_vertical(style);
        let specified_bb_opt: Option<i32> = match style.box_sizing {
            BoxSizing::ContentBox => style
                .height
                .map(|height_val| (height_val as i32).saturating_add(extras)),
            BoxSizing::BorderBox => style.height.map(|height_val| height_val as i32),
        };
        let min_bb_opt: Option<i32> = match style.box_sizing {
            BoxSizing::ContentBox => style
                .min_height
                .map(|height_val| (height_val as i32).saturating_add(extras)),
            BoxSizing::BorderBox => style.min_height.map(|height_val| height_val as i32),
        };
        let max_bb_opt: Option<i32> = match style.box_sizing {
            BoxSizing::ContentBox => style
                .max_height
                .map(|height_val| (height_val as i32).saturating_add(extras)),
            BoxSizing::BorderBox => style.max_height.map(|height_val| height_val as i32),
        };
        let mut out = specified_bb_opt.unwrap_or(0i32);
        if let Some(min_bb) = min_bb_opt {
            out = out.max(min_bb);
        }
        if let Some(max_bb) = max_bb_opt {
            out = out.min(max_bb);
        }
        out.max(0i32)
    }
    let mut computed_height = used_border_box_height(style);
    if style.height.is_none() {
        computed_height = child_content_height
            .saturating_add(extras.padding_top)
            .saturating_add(extras.padding_bottom)
            .saturating_add(extras.border_top)
            .saturating_add(extras.border_bottom);
        if computed_height == 0i32 && layouter.has_inline_text_descendant(child_key) {
            computed_height = default_line_height_px(style);
        }
    }
    computed_height
}

/// Build child container metrics and compute raw content height by laying out descendants.
/// Returns `(content_height, last_positive_bottom_margin)`.
#[inline]
pub fn compute_child_content_height(layouter: &mut Layouter, cctx: ChildContentCtx) -> (i32, i32) {
    let child_metrics = Layouter::build_child_metrics(
        cctx.used_border_box_width,
        HorizontalEdges {
            padding_left: cctx.sides.padding_left,
            padding_right: cctx.sides.padding_right,
            border_left: cctx.sides.border_left,
            border_right: cctx.sides.border_right,
        },
        TopEdges {
            padding_top: cctx.sides.padding_top,
            border_top: cctx.sides.border_top,
        },
        cctx.x,
        cctx.y,
    );
    let (_reflowed, content_height, last_pos_mb, _last_info) =
        layouter.layout_block_children(cctx.key, &child_metrics, cctx.ancestor_applied_at_edge);
    (content_height, last_pos_mb)
}
