//! Orchestrator: entry points and root aggregation for layout.

use core::sync::atomic::{Ordering, compiler_fence};
use css_box::compute_box_sides;
use style_engine::ComputedStyle;

use super::{INITIAL_CONTAINING_BLOCK_WIDTH, LayoutRect};
use crate::dimensions;
use crate::visual_formatting;
use crate::{ContainerMetrics, LayoutNodeKind, Layouter, RootHeightsCtx};
use js::NodeKey;

#[inline]
/// Computes a naive block layout and returns the number of nodes affected.
pub fn compute_layout_impl(layouter: &mut Layouter) -> usize {
    layouter.perf_layout_time_last_ms = 0;
    layouter.perf_updates_applied = 0;
    layouter.perf_nodes_reflowed_last = 0;
    layouter.perf_dirty_subtrees_last = 0;
    layouter.perf_layout_time_last_ms = 0;
    compiler_fence(Ordering::SeqCst);
    layout_root_impl(layouter)
}

#[inline]
/// Compute container metrics for `root` given an initial containing block width.
pub fn compute_container_metrics_impl(
    layouter: &Layouter,
    root: NodeKey,
    icb_width: i32,
) -> ContainerMetrics {
    let root_style = layouter
        .computed_styles
        .get(&root)
        .cloned()
        .unwrap_or_else(ComputedStyle::default);

    let sides = compute_box_sides(&root_style);
    let padding_left = sides.padding_left;
    let padding_right = sides.padding_right;
    let padding_top = sides.padding_top;
    let border_left = sides.border_left;
    let border_right = sides.border_right;
    let border_top = sides.border_top;
    let margin_left = 0i32;
    let margin_top = 0i32;
    let horizontal_non_content = padding_left
        .saturating_add(padding_right)
        .saturating_add(border_left)
        .saturating_add(border_right);
    let container_width = icb_width.saturating_sub(horizontal_non_content).max(0i32);

    ContainerMetrics {
        container_width,
        padding_left,
        padding_top,
        border_left,
        border_top,
        margin_left,
        margin_top,
    }
}

/// Lays out the root node and its children.
pub fn layout_root_impl(layouter: &mut Layouter) -> usize {
    let icb_width: i32 = INITIAL_CONTAINING_BLOCK_WIDTH;

    let Some(root) = layouter.choose_layout_root() else {
        layouter.rects.clear();
        compiler_fence(Ordering::SeqCst);
        return 0;
    };

    let metrics = compute_container_metrics_impl(layouter, root, icb_width);

    layouter.rects.insert(
        root,
        LayoutRect {
            x: 0,
            y: 0,
            width: metrics.container_width,
            height: 0,
        },
    );

    let (reflowed_count, _content_height_from_cursor, root_last_pos_mb) =
        layouter.layout_block_children(root, &metrics, false);

    let root_y =
        visual_formatting::root::compute_root_y_after_top_collapse(layouter, root, &metrics);

    let (_content_top, content_bottom) = aggregate_content_extents_impl(layouter, root);
    let root_y_aligned = root_y;
    let (content_height, root_height_border_box) = dimensions::compute_root_heights_impl(
        layouter,
        RootHeightsCtx {
            root,
            metrics,
            root_y: root_y_aligned,
            root_last_pos_mb,
            content_bottom,
        },
    );
    update_root_rect_impl(
        layouter,
        root,
        &metrics,
        root_y_aligned,
        root_height_border_box,
    );

    layouter.perf_nodes_reflowed_last = reflowed_count as u64;
    push_dirty_rect_if_changed_impl(
        layouter,
        metrics.container_width,
        content_height,
        reflowed_count,
    );
    compiler_fence(Ordering::SeqCst);
    reflowed_count
}

/// Aggregate the minimum top and maximum bottom (including positive bottom margin) across block children.
pub fn aggregate_content_extents_impl(
    layouter: &Layouter,
    root: NodeKey,
) -> (Option<i32>, Option<i32>) {
    let mut content_top: Option<i32> = None;
    let mut content_bottom: Option<i32> = None;
    if let Some(children) = layouter.children.get(&root) {
        for child_key in children {
            if matches!(
                layouter.nodes.get(child_key),
                Some(&LayoutNodeKind::Block { .. })
            ) && let Some(rect) = layouter.rects.get(child_key)
            {
                content_top =
                    Some(content_top.map_or(rect.y, |current_top| current_top.min(rect.y)));
                let bottom_margin = layouter
                    .computed_styles
                    .get(child_key)
                    .map_or(0i32, |style| style.margin.bottom as i32)
                    .max(0i32);
                let bottom = rect
                    .y
                    .saturating_add(rect.height)
                    .saturating_add(bottom_margin);
                content_bottom = Some(
                    content_bottom.map_or(bottom, |current_bottom| current_bottom.max(bottom)),
                );
            }
        }
    }
    (content_top, content_bottom)
}

/// Update the root rectangle with final y and height.
pub fn update_root_rect_impl(
    layouter: &mut Layouter,
    root: NodeKey,
    metrics: &ContainerMetrics,
    root_y: i32,
    root_height_border_box: i32,
) {
    if let Some(root_rect) = layouter.rects.get_mut(&root) {
        root_rect.x = metrics.margin_left;
        root_rect.y = root_y;
        root_rect.height = root_height_border_box;
    }
}

/// Push a dirty rectangle when reflow changed any nodes.
pub fn push_dirty_rect_if_changed_impl(
    layouter: &mut Layouter,
    width: i32,
    content_height: i32,
    reflowed_count: usize,
) {
    if reflowed_count > 0 {
        layouter.dirty_rects.push(LayoutRect {
            x: 0,
            y: 0,
            width,
            height: content_height.max(0i32),
        });
    }
}
