//! Auto margin resolution for flex items.
//!
//! Implements CSS Flexbox auto margin distribution along the main axis.

use super::super::FlexChild;

/// Quantize a CSS pixel value downward to the layout unit (1/64 px). Used for between-spacing to
/// match Chromium's fixed-point accumulation and avoid accumulating rounding overflow across slots.
pub fn quantize_layout_floor(value: f32) -> f32 {
    ((value * 64.0).floor()) / 64.0
}

/// Result type for auto margin calculations.
pub type OuterCalc = (Vec<f32>, Vec<f32>, usize, f32);

/// Distribute remaining main-axis free space across any auto margins.
///
/// Produces effective left margins and outer sizes for each item. Returns
/// `(outer_sizes, effective_left_margins, auto_slots, sum_outer)`.
pub fn resolve_auto_margins_and_outer(
    items: &[FlexChild],
    inner_sizes: &[f32],
    container_main_size: f32,
    gaps_total: f32,
) -> OuterCalc {
    // Count auto slots and sum non-auto margins
    let mut non_auto_margins_sum = 0.0f32;
    let auto_slots = items.iter().fold(0usize, |acc, child| {
        let left_slot = usize::from(child.margin_left_auto);
        let right_slot = usize::from(child.margin_right_auto);
        non_auto_margins_sum += if child.margin_left_auto {
            0.0
        } else {
            child.margin_left.max(0.0)
        };
        non_auto_margins_sum += if child.margin_right_auto {
            0.0
        } else {
            child.margin_right.max(0.0)
        };
        acc.saturating_add(left_slot).saturating_add(right_slot)
    });
    let sum_inner_after_flex: f32 = inner_sizes.iter().copied().sum();
    let remaining_for_margins_raw =
        container_main_size - sum_inner_after_flex - non_auto_margins_sum - gaps_total;
    let remaining_for_margins_pos = remaining_for_margins_raw.max(0.0);
    let auto_each = if auto_slots > 0 {
        quantize_layout_floor(remaining_for_margins_pos / auto_slots as f32)
    } else {
        0.0
    };
    let mut outer_sizes: Vec<f32> = Vec::with_capacity(items.len());
    let mut effective_left_margins: Vec<f32> = Vec::with_capacity(items.len());
    for (child, inner) in items.iter().zip(inner_sizes.iter().copied()) {
        let eff_left = child.margin_left.max(0.0)
            + if child.margin_left_auto {
                auto_each
            } else {
                0.0
            };
        let eff_right = child.margin_right.max(0.0)
            + if child.margin_right_auto {
                auto_each
            } else {
                0.0
            };
        effective_left_margins.push(eff_left);
        outer_sizes.push(inner + eff_left + eff_right);
    }
    let sum_outer: f32 = outer_sizes.iter().copied().sum();
    (outer_sizes, effective_left_margins, auto_slots, sum_outer)
}
