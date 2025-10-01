//! Spec: CSS 2.2 §10.1 Definition of containing block — core geometry helpers
//! Thin wrappers around inherent helpers to provide a spec-mirrored path.

use crate::ContainerMetrics;
use crate::{HorizontalEdges, TopEdges};

/// Spec: §10.1 — Build `ContainerMetrics` for a child from its used width and edge aggregates.
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
    // Spec: CSS 2.2 §10.1 — containing block and content box origin math.
    // Compute the child's available content width by subtracting padding+border from used border-box width.
    let container_width = used_border_box_width
        .saturating_sub(horizontal.padding_left)
        .saturating_sub(horizontal.padding_right)
        .saturating_sub(horizontal.border_left)
        .saturating_sub(horizontal.border_right)
        .max(0i32);

    ContainerMetrics {
        container_width,
        total_border_box_width: used_border_box_width,
        padding_left: horizontal.padding_left,
        padding_top: top.padding_top,
        border_left: horizontal.border_left,
        border_top: top.border_top,
        margin_left: x_position,
        margin_top: y_position,
    }
}

/// Spec: §10.1 — Compute the parent's content origin from its margins, borders, and padding.
#[allow(
    dead_code,
    reason = "Spec-mapped public API; used by tests/future callers"
)]
pub const fn parent_content_origin(metrics: &ContainerMetrics) -> (i32, i32) {
    let x = metrics
        .margin_left
        .saturating_add(metrics.border_left)
        .saturating_add(metrics.padding_left);
    let y = metrics
        .margin_top
        .saturating_add(metrics.border_top)
        .saturating_add(metrics.padding_top);
    (x, y)
}
