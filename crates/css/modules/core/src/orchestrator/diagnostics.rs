//! Diagnostics helpers for logging and test instrumentation.

use crate::{ChildLayoutCtx, LayoutRect, Layouter, VertLog};
use js::NodeKey;
use log::debug;
use std::collections::HashMap;

/// Build the last-placed info tuple for diagnostics and content-bottom calculation.
pub fn last_info_for_child(
    layouter: &Layouter,
    child_key: NodeKey,
    mb_out: i32,
) -> Option<(NodeKey, i32, i32)> {
    let rect = layouter.rects.get(&child_key)?;
    let rect_bottom = (rect.y + rect.height).round() as i32;
    Some((child_key, rect_bottom, mb_out))
}

/// Log initial context for the first placed child under a parent (diagnostics only).
pub fn log_first_child_context(root: NodeKey, ctx: &ChildLayoutCtx) {
    debug!(
        "[FIRST-CHILD ctx root={root:?}] index={} is_first={} y_cursor={} prev_mb={} parent_self_top={} leading_applied={} ancestor_applied={} parent_edge_collapsible={} clearance_floor={}",
        ctx.index,
        ctx.is_first_placed,
        ctx.y_cursor,
        ctx.previous_bottom_margin,
        ctx.parent_self_top_margin,
        ctx.leading_top_applied,
        ctx.ancestor_applied_at_edge_for_children,
        ctx.parent_edge_collapsible,
        ctx.clearance_floor_y
    );
}

/// Emit a vertical layout log for debugging margin collapsing and positioning.
pub fn log_vert(log: VertLog) {
    debug!(
        "[VERT] idx={} prev_mb={} mt_raw={} mt_eff={} eff_bottom={} is_empty={} collapsed_top={} parent_origin_y={} y_position={} y_cursor_in={} leading_top_applied={}",
        log.index,
        log.prev_mb,
        log.margin_top_raw,
        log.margin_top_eff,
        log.eff_bottom,
        log.is_empty,
        log.collapsed_top,
        log.parent_origin_y,
        log.y_position,
        log.y_cursor_in,
        log.leading_top_applied
    );
}

/// Insert a child's rect into the layouter's rects map.
pub fn insert_child_rect(rects: &mut HashMap<NodeKey, LayoutRect>, key: NodeKey, rect: LayoutRect) {
    rects.insert(key, rect);
}
