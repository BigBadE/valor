//! Flex grow and shrink distribution algorithms.
//!
//! Implements the CSS Flexbox specification's flexible sizing algorithm
//! for distributing free space among flex items.

use super::super::FlexChild;

/// Clamp a value between min and max inclusive.
pub const fn clamp(value: f32, min_v: f32, max_v: f32) -> f32 {
    // Guard against invalid constraints where min > max
    if min_v > max_v {
        // Use the average as a reasonable fallback
        return f32::midpoint(min_v, max_v);
    }
    value.max(min_v).min(max_v)
}

/// Distribute positive free space to items using flex-grow factors.
pub fn distribute_grow(free_space: f32, items: &[FlexChild], sizes: &mut [f32]) {
    debug_assert!(free_space >= 0.0, "grow called with negative free space");
    let mut remaining = free_space;
    let mut saturated = vec![false; items.len()];
    // Iterate to handle saturation at max constraints
    for _ in 0..items.len() {
        let mut sum_grow = 0.0f32;
        for (child, is_saturated) in items.iter().zip(saturated.iter()) {
            if !*is_saturated {
                sum_grow += child.flex_grow.max(0.0);
            }
        }
        if sum_grow <= 0.0 || remaining <= 0.0 {
            break;
        }
        let unit = remaining / sum_grow;
        let mut any_saturated = false;
        let mut applied_total = 0.0f32;
        for ((size_ref, child), sat_ref) in sizes.iter_mut().zip(items).zip(saturated.iter_mut()) {
            if *sat_ref {
                continue;
            }
            let delta = child.flex_grow.max(0.0) * unit;
            let grown = *size_ref + delta;
            let clamped = clamp(grown, child.min_main, child.max_main);
            let applied = clamped - *size_ref;
            *size_ref = clamped;
            applied_total += applied;
            if (clamped - child.max_main).abs() < f32::EPSILON {
                *sat_ref = true;
                any_saturated = true;
            }
        }
        remaining -= applied_total;
        if !any_saturated {
            break;
        }
    }
}

/// Distribute negative free space to items using weighted flex-shrink factors.
pub fn distribute_shrink(free_space: f32, items: &[FlexChild], sizes: &mut [f32]) {
    debug_assert!(free_space <= 0.0, "shrink called with positive free space");
    // Weighted shrink based on current size and shrink factor, with min saturation
    let mut remaining = -free_space; // positive amount to remove
    let mut frozen = vec![false; items.len()];
    for _ in 0..items.len() {
        let mut sum_weight = 0.0f32;
        for ((size_ref, child), is_frozen) in sizes.iter().zip(items).zip(frozen.iter()) {
            if *is_frozen {
                continue;
            }
            let basis = (*size_ref).max(0.0);
            sum_weight += basis * child.flex_shrink.max(0.0);
        }
        if sum_weight <= 0.0 || remaining <= 0.0 {
            break;
        }
        let mut any_froze = false;
        let mut applied_total = 0.0f32;
        for ((size_ref, child), frozen_ref) in sizes.iter_mut().zip(items).zip(frozen.iter_mut()) {
            if *frozen_ref {
                continue;
            }
            let basis = (*size_ref).max(0.0);
            let weight = basis * child.flex_shrink.max(0.0);
            let delta = remaining * (weight / sum_weight);
            let shrunk = (*size_ref - delta).max(0.0);
            let clamped = clamp(shrunk, child.min_main, child.max_main);
            let applied = *size_ref - clamped;
            *size_ref = clamped;
            applied_total += applied;
            if (clamped - child.min_main).abs() < f32::EPSILON {
                *frozen_ref = true;
                any_froze = true;
            }
        }
        remaining -= applied_total;
        if !any_froze {
            break;
        }
    }
}
