//! Box-sizing aware sizing utilities for the layouter.

//! Spec references:
//! - CSS Box Sizing Module Level 3 — `box-sizing`.
//! - CSS 2.2 §8.1 Box model.

use style_engine::{BoxSizing, ComputedStyle};

/// Sum horizontal padding and border widths in pixels (clamped to >= 0).
#[inline]
fn sum_horizontal(style: &ComputedStyle) -> i32 {
    let pad = style.padding.left.max(0.0f32) + style.padding.right.max(0.0f32);
    let border = style.border_width.left.max(0.0f32) + style.border_width.right.max(0.0f32);
    (pad + border) as i32
}

/// Sum vertical padding and border widths in pixels (clamped to >= 0).
#[inline]
fn sum_vertical(style: &ComputedStyle) -> i32 {
    let pad = style.padding.top.max(0.0f32) + style.padding.bottom.max(0.0f32);
    let border = style.border_width.top.max(0.0f32) + style.border_width.bottom.max(0.0f32);
    (pad + border) as i32
}

/// Compute the used border-box width respecting `box-sizing`.
///
/// The `fill_available_border_box_width` is the container's available border-box width
/// after subtracting margins and acts as an upper clamp. Min/max are applied in the
/// same box space as the specified size per Box Sizing L3.
#[inline]
pub fn used_border_box_width(style: &ComputedStyle, fill_available_border_box_width: i32) -> i32 {
    let extras = sum_horizontal(style);

    // Convert specified and min/max into border-box space based on box-sizing.
    let specified_bb_opt: Option<i32> = match style.box_sizing {
        BoxSizing::ContentBox => style
            .width
            .map(|width_val| (width_val as i32).saturating_add(extras)),
        BoxSizing::BorderBox => style.width.map(|width_val| width_val as i32),
    };
    let min_bb_opt: Option<i32> = match style.box_sizing {
        BoxSizing::ContentBox => style
            .min_width
            .map(|width_val| (width_val as i32).saturating_add(extras)),
        BoxSizing::BorderBox => style.min_width.map(|width_val| width_val as i32),
    };
    let max_bb_opt: Option<i32> = match style.box_sizing {
        BoxSizing::ContentBox => style
            .max_width
            .map(|width_val| (width_val as i32).saturating_add(extras)),
        BoxSizing::BorderBox => style.max_width.map(|width_val| width_val as i32),
    };

    // Start from specified or fill-available when auto.
    let mut out = specified_bb_opt.unwrap_or(fill_available_border_box_width);

    // Apply min/max clamps in border-box space.
    if let Some(min_bb) = min_bb_opt {
        out = out.max(min_bb);
    }
    if let Some(max_bb) = max_bb_opt {
        out = out.min(max_bb);
    }

    // Clamp to container availability as a final guard.
    out.min(fill_available_border_box_width).max(0i32)
}

/// Compute the used border-box height respecting `box-sizing`.
///
/// Applies min/max in the same box space as the specified size per Box Sizing L3.
#[inline]
pub fn used_border_box_height(style: &ComputedStyle) -> i32 {
    let extras = sum_vertical(style);

    // Convert specified and min/max into border-box space based on box-sizing.
    let specified_bb_opt: Option<i32> = match style.box_sizing {
        BoxSizing::ContentBox => style
            .height
            .map(|height_val| (height_val as i32).saturating_add(extras)),
        BoxSizing::BorderBox => style.height.map(|height_val| height_val as i32),
    };
    let min_bb_opt: Option<i32> = match style.box_sizing {
        BoxSizing::ContentBox => style
            .min_height
            .map(|height_val| (height_val as i32).saturating_add(extras)),
        BoxSizing::BorderBox => style.min_height.map(|height_val| height_val as i32),
    };
    let max_bb_opt: Option<i32> = match style.box_sizing {
        BoxSizing::ContentBox => style
            .max_height
            .map(|height_val| (height_val as i32).saturating_add(extras)),
        BoxSizing::BorderBox => style.max_height.map(|height_val| height_val as i32),
    };

    // Auto height: start at 0 in MVP.
    let mut out = specified_bb_opt.unwrap_or(0i32);

    // Apply min/max clamps in border-box space.
    if let Some(min_bb) = min_bb_opt {
        out = out.max(min_bb);
    }
    if let Some(max_bb) = max_bb_opt {
        out = out.min(max_bb);
    }

    out.max(0i32)
}
