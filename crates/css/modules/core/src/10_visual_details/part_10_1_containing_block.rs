//! Spec: CSS 2.2 §10.1 Definition of containing block — core geometry helpers
//! Thin wrappers around inherent helpers to provide a spec-mirrored path.

use crate::Layouter;
use crate::types::ContainerMetrics;
use crate::{HorizontalEdges, TopEdges};

/// Spec: §10.1 — Build `ContainerMetrics` for a child from its used width and edge aggregates.
#[inline]
#[allow(
    dead_code,
    reason = "Spec-mapped public API; used by tests/future callers"
)]
pub fn build_child_metrics(
    used_border_box_width: i32,
    horizontal: HorizontalEdges,
    top: TopEdges,
    x_position: i32,
    y_position: i32,
) -> ContainerMetrics {
    Layouter::build_child_metrics(
        used_border_box_width,
        horizontal,
        top,
        x_position,
        y_position,
    )
}

/// Spec: §10.1 — Compute the parent's content origin from its margins, borders, and padding.
#[inline]
#[allow(
    dead_code,
    reason = "Spec-mapped public API; used by tests/future callers"
)]
pub const fn parent_content_origin(metrics: &ContainerMetrics) -> (i32, i32) {
    Layouter::parent_content_origin(metrics)
}
