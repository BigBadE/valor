//! Dimensions helpers: root height, used height and child content height computations.

use crate::visual_formatting::vertical::establishes_bfc;
use css_text::default_line_height_px;
use js::NodeKey;
use style_engine::ComputedStyle;

use crate::{ChildContentCtx, HeightExtras, HorizontalEdges, Layouter, RootHeightsCtx, TopEdges};

/// Compute content height and root border-box height.
#[inline]
pub fn compute_root_heights_impl(layouter: &Layouter, ctx: RootHeightsCtx) -> (i32, i32) {
    let content_origin = ctx
        .root_y
        .saturating_add(ctx.metrics.border_top)
        .saturating_add(ctx.metrics.padding_top);
    // Look up the root's computed style to determine bottom-edge collapsibility (we need
    // padding-bottom and border-bottom which are not in ContainerMetrics).
    let root_style = layouter
        .computed_styles
        .get(&ctx.root)
        .cloned()
        .unwrap_or_else(ComputedStyle::default);
    let padding_bottom = root_style.padding.bottom.max(0.0f32) as i32;
    let border_bottom = root_style.border_width.bottom.max(0.0f32) as i32;
    let bottom_edge_collapsible =
        padding_bottom == 0i32 && border_bottom == 0i32 && !establishes_bfc(&root_style);
    // CSS 2.2 §10.6.3: The height of a block is the distance between the top content edge
    // and the bottom margin edge of the bottommost in-flow block box, plus its own padding/border.
    // Here, `content_bottom` must already represent the last child's bottom margin edge.
    let content_bottom_with_parent_mb = ctx.content_bottom;
    let content_height = content_bottom_with_parent_mb.map_or(0i32, |bottom_value| {
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

/// Compute used height for a block child, applying box extras when height is auto and
/// falling back to a single line height if there is inline text and overall height is 0.
#[inline]
pub fn compute_used_height_impl(
    layouter: &Layouter,
    style: &ComputedStyle,
    child_key: NodeKey,
    extras: HeightExtras,
    child_content_height: i32,
) -> i32 {
    use crate::sizing::used_border_box_height;
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
pub fn compute_child_content_height_impl(
    layouter: &mut Layouter,
    cctx: ChildContentCtx,
) -> (i32, i32) {
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
