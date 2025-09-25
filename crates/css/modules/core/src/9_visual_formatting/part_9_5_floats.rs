//! Spec: CSS 2.2 §9.5 Floats — clearance and float avoidance bands
//! Re-export float-related helpers.

use crate::Layouter;
use crate::PlaceLoopCtx;
use js::NodeKey;

/// Spec: §9.5 — Compute the clearance floor for a child.
#[allow(
    dead_code,
    reason = "Public API kept for direct spec-mapped calls; used by tests/future orchestrator paths"
)]
#[inline]
pub fn compute_clearance_floor_for_child(
    layouter: &Layouter,
    child_key: NodeKey,
    floor_left: i32,
    floor_right: i32,
) -> i32 {
    layouter.compute_clearance_floor_for_child(child_key, floor_left, floor_right)
}

/// Spec: §9.5 — Update side-specific float clearance floors after laying out a float.
#[allow(
    dead_code,
    reason = "Public API kept for direct spec-mapped calls; used by tests/future orchestrator paths"
)]
#[inline]
pub fn update_clearance_floors_for_float(
    layouter: &Layouter,
    child_key: NodeKey,
    current_left: i32,
    current_right: i32,
) -> (i32, i32) {
    layouter.update_clearance_floors_for_float(child_key, current_left, current_right)
}

/// Spec: §9.5 — Compute horizontal float-avoidance bands at a given y.
#[allow(
    dead_code,
    reason = "Public API kept for direct spec-mapped calls; used by tests/future orchestrator paths"
)]
#[inline]
pub fn compute_float_bands_for_y(
    layouter: &Layouter,
    loop_ctx: &PlaceLoopCtx<'_>,
    up_to_index: usize,
    y_in_parent: i32,
) -> (i32, i32) {
    layouter.compute_float_bands_for_y(loop_ctx, up_to_index, y_in_parent)
}
