//! Spec: CSS 2.2 §9.4.3 Relative positioning — offsets application

use css_orchestrator::style_model::{ComputedStyle, Position};

/// Apply relative offsets from top/left/right/bottom when `position: relative`.
/// Returns `(x_adjust, y_adjust)` in pixels.
#[inline]
pub const fn apply_relative_offsets(style: &ComputedStyle) -> (i32, i32) {
    if !matches!(style.position, Position::Relative) {
        return (0i32, 0i32);
    }
    let mut x_adjust = 0i32;
    let mut y_adjust = 0i32;
    if let Some(left_off) = style.left {
        x_adjust = x_adjust.saturating_add(left_off as i32);
    }
    if let Some(right_off) = style.right {
        x_adjust = x_adjust.saturating_sub(right_off as i32);
    }
    if let Some(top_off) = style.top {
        y_adjust = y_adjust.saturating_add(top_off as i32);
    }
    if let Some(bottom_off) = style.bottom {
        y_adjust = y_adjust.saturating_sub(bottom_off as i32);
    }
    (x_adjust, y_adjust)
}
