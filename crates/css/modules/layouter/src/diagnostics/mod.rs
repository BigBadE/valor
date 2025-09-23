//! Diagnostics helpers for the layouter.

use log::debug;

use crate::VertLog;

#[inline]
/// Log a vertical layout step with margin collapsing inputs and results.
pub fn log_vert_impl(entry: VertLog) {
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
        entry.leading_top_applied,
    );
}
