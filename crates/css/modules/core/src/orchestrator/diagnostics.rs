//! Diagnostics and logging helpers for layout

use crate::{ChildLayoutCtx, LayoutRect, Layouter, VertLog};
use js::NodeKey;
use log::debug;
use std::collections::HashMap;

#[inline]
/// Log a vertical placement entry.
pub fn log_vert(entry: VertLog) {
    debug!(
        "[VERT child idx={}] pm_prev_bottom={} child(mt_raw={}, mt_eff={}, mb(eff={}), empty={}) collapsed_top={} parent_origin_y={} -> y={} cursor_in={} lt_applied={}",
        entry.index,
        entry.prev_mb,
        entry.margin_top_raw,
        entry.margin_top_eff,
        entry.eff_bottom,
        entry.is_empty,
        entry.collapsed_top,
        entry.parent_origin_y,
        entry.y_position,
        entry.y_cursor_in,
        entry.leading_top_applied
    );
}

#[inline]
/// Log the initial context for the first placed child under a parent.
pub fn log_first_child_context(root: NodeKey, ctx: &ChildLayoutCtx) {
    debug!(
        "[VERT-CONTEXT first root={root:?}] pad_top={} border_top={} parent_self_top={} prev_bottom={} y_cursor={} lt_applied={} parent_edge_collapsible={}",
        ctx.metrics.padding_top,
        ctx.metrics.border_top,
        ctx.parent_self_top_margin,
        ctx.previous_bottom_margin,
        ctx.y_cursor,
        ctx.leading_top_applied,
        ctx.parent_edge_collapsible
    );
}

#[inline]
/// Insert or update the rectangle for a child in the rect map.
pub fn insert_child_rect(
    rects: &mut HashMap<NodeKey, LayoutRect>,
    child_key: NodeKey,
    rect: LayoutRect,
) {
    rects.insert(child_key, rect);
}

#[inline]
/// Build the last-placed info tuple `(key, rect_bottom, mb_out)` for a child.
pub fn last_info_for_child(
    layouter: &Layouter,
    child_key: NodeKey,
    mb_out: i32,
) -> Option<(NodeKey, i32, i32)> {
    let rect = layouter.rects.get(&child_key).copied()?;
    let rect_bottom = (rect.y + rect.height).round() as i32;
    Some((child_key, rect_bottom, mb_out))
}
