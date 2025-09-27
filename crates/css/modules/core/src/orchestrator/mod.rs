//! Orchestrator: entry points and root aggregation for layout.

use css_box::compute_box_sides;
use css_orchestrator::style_model::ComputedStyle;

use crate::SCROLLBAR_GUTTER_PX;
use crate::chapter8::part_8_3_1_collapsing_margins::compute_root_y_after_top_collapse;
use crate::chapter9::part_9_4_1_block_formatting_context::establishes_block_formatting_context;
use crate::chapter10::part_10_6_3_height_of_blocks::compute_root_heights;
use crate::{ContainerMetrics, Layouter, RootHeightsCtx};
use crate::{INITIAL_CONTAINING_BLOCK_WIDTH, LayoutRect};
use js::NodeKey;
// Layouter is defined in the parent module (`lib.rs`) to allow this child module
// to access private fields per Rust's visibility rules.

/// Diagnostics helpers for logging and test instrumentation.
pub mod diagnostics;
/// Per-child placement pipeline (position, heights, rect commit, logging).
pub mod place_child;
/// Tree traversal and document utilities used by the orchestrator.
pub mod tree;

#[inline]
/// Computes a naive block layout and returns the number of nodes affected.
pub fn compute_layout_impl(layouter: &mut Layouter) -> usize {
    layouter.perf_layout_time_last_ms = 0;
    layouter.perf_updates_applied = 0;
    layouter.perf_nodes_reflowed_last = 0;
    layouter.perf_dirty_subtrees_last = 0;
    layouter.perf_layout_time_last_ms = 0;
    layout_root_impl(layouter)
}

#[inline]
/// Emit diagnostics for the last placed in-flow block used to compute `content_bottom`.
fn log_last_placed_child_diag(layouter: &Layouter, root: NodeKey, content_bottom: Option<i32>) {
    let ordered_blocks = layouter.collect_block_children(root);
    if let Some(last_key) = ordered_blocks
        .iter()
        .rev()
        .copied()
        .find(|child_key| layouter.rects.contains_key(child_key))
    {
        let rect = layouter.rects.get(&last_key).copied().unwrap_or_default();
        let raw_mb = layouter
            .computed_styles
            .get(&last_key)
            .map_or(0i32, |style| style.margin.bottom as i32);
        let id_opt = layouter
            .attrs
            .get(&last_key)
            .and_then(|map| map.get("id").cloned())
            .unwrap_or_default();
        let bottom_edge = rect.y.saturating_add(rect.height).saturating_add(raw_mb);
        log::debug!(
            "[ROOT-LAST DIAG] last_key={last_key:?} id=#{} rect=({}, {}, {}, {}) mb_raw={} bottom_edge={} content_bottom={:?}",
            id_opt,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            raw_mb,
            bottom_edge,
            content_bottom
        );
    }
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
    // Normalize the initial containing block (ICB) edges to the viewport: for the chosen layout root
    // (html/body), treat the top/left edges as having no padding/border when computing the container
    // origin. This matches browser behavior where the canvas/viewport origin is at (0,0) and avoids
    // shifting all descendants by any authored root-side borders.
    // Spec notes:
    //   - CSS 2.2 Visual formatting model root and initial containing block: the canvas has no border.
    //   - getBoundingClientRect() for element boxes is measured relative to the viewport origin.
    //     We align our ICB to (0,0) for parity with Chromium in tests.
    let padding_left: i32 = 0; // sides.padding_left;
    let padding_right = sides.padding_right;
    let padding_top: i32 = 0; // sides.padding_top;
    let border_left: i32 = 0; // sides.border_left;
    let border_right = sides.border_right;
    let border_top: i32 = 0; // sides.border_top;
    let margin_left = 0i32;
    let margin_top = 0i32;
    // Apply a fixed scrollbar gutter for the viewport to approximate Chromium's reserved space.
    // This brings our initial containing block into alignment with Chrome's layout width on Windows.
    let scrollbar_gutter = SCROLLBAR_GUTTER_PX;
    let total_border_box_width = icb_width.saturating_sub(scrollbar_gutter).max(0i32);
    let horizontal_non_content = padding_left
        .saturating_add(padding_right)
        .saturating_add(border_left)
        .saturating_add(border_right);
    let container_width = total_border_box_width
        .saturating_sub(horizontal_non_content)
        .max(0i32);

    ContainerMetrics {
        container_width,
        total_border_box_width,
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
        return 0;
    };

    let metrics = compute_container_metrics_impl(layouter, root, icb_width);

    layouter.rects.insert(
        root,
        LayoutRect {
            x: 0,
            y: 0,
            // Root rect is the viewport/root element border-box width.
            width: metrics.total_border_box_width,
            height: 0,
        },
    );

    let (reflowed_count, _content_height_from_cursor, root_last_pos_mb, last_placed_info) =
        layouter.layout_block_children(root, &metrics, false);

    let root_y = compute_root_y_after_top_collapse(layouter, root, &metrics);

    // Prefer effective outgoing bottom margin from the placement loop if available.
    let content_bottom = if let Some((last_key, rect_bottom, mb_out)) = last_placed_info {
        let root_style = layouter
            .computed_styles
            .get(&root)
            .cloned()
            .unwrap_or_else(ComputedStyle::default);
        let padding_bottom = root_style.padding.bottom.max(0.0f32) as i32;
        let border_bottom = root_style.border_width.bottom.max(0.0f32) as i32;
        let bottom_edge_collapsible = padding_bottom == 0i32
            && border_bottom == 0i32
            && !establishes_block_formatting_context(&root_style);
        let bottom_edge = if bottom_edge_collapsible {
            rect_bottom
        } else {
            rect_bottom.saturating_add(mb_out)
        };
        log::debug!(
            "[ROOT-LAST] (from loop) key={last_key:?} rect_bottom={rect_bottom} mb_out={mb_out} -> bottom_edge={bottom_edge}"
        );
        Some(bottom_edge)
    } else {
        compute_last_block_bottom_edge_impl(layouter, root)
    };
    log_last_placed_child_diag(layouter, root, content_bottom);
    let root_y_aligned = root_y;
    let (content_height, root_height_border_box) = compute_root_heights(
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
    reflowed_count
}

/// Compute the bottom margin edge of the bottommost in-flow block (placed) child per CSS 2.2 §10.6.3.
/// Returns the absolute bottom position including the last child's signed margin-bottom.
pub fn compute_last_block_bottom_edge_impl(layouter: &Layouter, root: NodeKey) -> Option<i32> {
    let ordered_blocks = layouter.collect_block_children(root);
    // Find the last block in placement order that has a rect.
    let last_key_opt = ordered_blocks
        .iter()
        .rev()
        .copied()
        .find(|child_key| layouter.rects.contains_key(child_key));
    let last_key = last_key_opt?;
    let rect = layouter.rects.get(&last_key).copied().unwrap_or_default();
    let raw_mb = layouter
        .computed_styles
        .get(&last_key)
        .map_or(0i32, |style| style.margin.bottom as i32);
    let bottom_edge = rect.y.saturating_add(rect.height).saturating_add(raw_mb);
    log::debug!(
        "[ROOT-LAST] key={last_key:?} rect=({}, {}, {}, {}) mb_raw={} -> bottom_edge={}",
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        raw_mb,
        bottom_edge
    );
    Some(bottom_edge)
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
