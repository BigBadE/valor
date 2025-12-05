//! Spec: CSS 2.2 §9.5 Floats — clearance and float avoidance bands
//! Implement float-related helpers for the layouter.

use crate::chapter9::part_9_4_1_block_formatting_context::establishes_block_formatting_context;
use crate::chapter10::part_10_1_containing_block::parent_content_origin;
use crate::{Layouter, PlaceLoopCtx};
use css_box::compute_box_sides;
use css_orchestrator::style_model::{Clear, ComputedStyle, Float};
use js::NodeKey;
use log::debug;

/// Spec: §9.5 — Compute the clearance floor for a child.
pub fn compute_clearance_floor_for_child(
    layouter: &Layouter,
    child_key: NodeKey,
    floor_left: i32,
    floor_right: i32,
) -> i32 {
    let style = layouter
        .computed_styles
        .get(&child_key)
        .cloned()
        .unwrap_or_else(ComputedStyle::default);
    if establishes_block_formatting_context(&style) {
        debug!("[CLEAR-FLOOR] child={child_key:?} is BFC => floor=0 (mask external)");
        return 0i32;
    }
    let floor = match style.clear {
        Clear::Left => floor_left,
        Clear::Right => floor_right,
        Clear::Both => floor_left.max(floor_right),
        Clear::None => 0i32,
    };
    debug!(
        "[CLEAR-FLOOR] child={child_key:?} clear={:?} floors(L={}, R={}) -> floor={}",
        style.clear, floor_left, floor_right, floor
    );
    floor
}

/// Spec: §9.5 — Update side-specific float clearance floors after laying out a float.
pub fn update_clearance_floors_for_float(
    layouter: &Layouter,
    child_key: NodeKey,
    current_left: i32,
    current_right: i32,
) -> (i32, i32) {
    let mut left = current_left;
    let mut right = current_right;
    let Some(style) = layouter.computed_styles.get(&child_key) else {
        return (left, right);
    };
    if matches!(style.float, Float::None) {
        return (left, right);
    }
    let Some(rect) = layouter.rects.get(&child_key) else {
        return (left, right);
    };
    let mb_pos = compute_box_sides(style).margin_bottom.max(0i32);
    let bottom_edge = ((rect.y + rect.height).round() as i32).saturating_add(mb_pos);
    debug!(
        "[FLOOR-UPDATE] child={child_key:?} float={:?} rect_bottom={} mb_pos={} -> bottom_edge={} (prev L={}, R={})",
        style.float,
        (rect.y + rect.height),
        mb_pos,
        bottom_edge,
        left,
        right
    );
    match style.float {
        Float::Left => {
            if bottom_edge > left {
                left = bottom_edge;
            }
        }
        Float::Right => {
            if bottom_edge > right {
                right = bottom_edge;
            }
        }
        Float::None => {}
    }
    (left, right)
}

/// Cap left/right float-avoidance bands to prevent over-constraining.
///
/// Ensures neither band exceeds the parent content width and their sum does not exceed it.
/// This prevents over-constraining available inline space when multiple floats overlap the
/// query `y`.
fn cap_bands(parent_content_width: i32, left_band: i32, right_band: i32) -> (i32, i32) {
    let mut left_capped = left_band.clamp(0i32, parent_content_width);
    let mut right_capped = right_band.clamp(0i32, parent_content_width);
    let sum = left_capped.saturating_add(right_capped);
    if sum > parent_content_width {
        let excess = sum.saturating_sub(parent_content_width);
        if left_capped >= right_capped {
            left_capped = left_capped.saturating_sub(excess);
        } else {
            right_capped = right_capped.saturating_sub(excess);
        }
    }
    (left_capped, right_capped)
}

/// Spec: §9.5 — Compute horizontal float-avoidance bands at a given y.
pub fn compute_float_bands_for_y(
    layouter: &Layouter,
    loop_ctx: &PlaceLoopCtx<'_>,
    up_to_index: usize,
    y_in_parent: i32,
) -> (i32, i32) {
    let mut left_band: i32 = 0;
    let mut right_band: i32 = 0;
    let (parent_x, _parent_y) = parent_content_origin(&loop_ctx.metrics);
    let parent_content_right = parent_x.saturating_add(loop_ctx.metrics.container_width);
    let parent_content_width = parent_content_right.saturating_sub(parent_x).max(0i32);
    for (idx, key) in loop_ctx.block_children.iter().copied().enumerate() {
        if idx >= up_to_index {
            break;
        }
        let Some(style) = layouter.computed_styles.get(&key) else {
            continue;
        };
        if matches!(style.float, Float::None) {
            continue;
        }
        let Some(rect) = layouter.rects.get(&key) else {
            continue;
        };
        let sides = compute_box_sides(style);
        let top = (rect.y.round() as i32).saturating_sub(sides.margin_top);
        let bottom = ((rect.y + rect.height).round() as i32).saturating_add(sides.margin_bottom);
        let overlaps = top <= y_in_parent && y_in_parent < bottom;
        let style_float = style.float;
        debug!(
            "[BANDS] considering prior float key={key:?} float={style_float:?} span=[{top}, {bottom}) query_y={y_in_parent} -> overlaps={overlaps}"
        );
        if !overlaps {
            continue;
        }
        // Compute occupied span using the margin box of the float.
        let left_edge = (rect.x.round() as i32).saturating_sub(sides.margin_left);
        let right_edge = ((rect.x + rect.width).round() as i32).saturating_add(sides.margin_right);
        match style.float {
            Float::Left => {
                // Left band is distance from parent content left to the float's right margin-edge.
                let occupied = right_edge.saturating_sub(parent_x);
                debug!(
                    "[BANDS L] key={key:?} right_edge={right_edge} parent_x={parent_x} occupied={occupied} (prev L={left_band})"
                );
                if occupied > left_band {
                    left_band = occupied;
                }
            }
            Float::Right => {
                // Right band is distance from float's left margin-edge to parent content right.
                let occupied = parent_content_right.saturating_sub(left_edge);
                debug!(
                    "[BANDS R] key={key:?} left_edge={left_edge} parent_right={parent_content_right} occupied={occupied} (prev R={right_band})"
                );
                if occupied > right_band {
                    right_band = occupied;
                }
            }
            Float::None => {}
        }
    }
    // Safety caps and final debug.
    let (left_capped, right_capped) = cap_bands(parent_content_width, left_band, right_band);
    debug!(
        "[BANDS OUT] y={y_in_parent} parent_w={parent_content_width} -> L={left_capped} R={right_capped}"
    );
    (left_capped, right_capped)
}
