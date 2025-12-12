//! Tests for single-line flexbox layout.

use super::*;

mod auto_margin_tests;
mod baseline_tests;
mod cross_alignment_tests;
mod flex_distribution_tests;
mod gap_tests;
mod multi_line_tests;

/// Helper to create a `FlexChild` with zero margins and the given basis.
#[inline]
pub fn item_zero_margins(handle: u64, basis: f32) -> FlexChild {
    FlexChild {
        handle: ItemRef(handle),
        flex_basis: basis,
        flex_grow: 0.0,
        flex_shrink: 0.0,
        min_main: 0.0,
        max_main: 1e9,
        margin_left: 0.0,
        margin_right: 0.0,
        margin_top: 0.0,
        margin_bottom: 0.0,
        margin_left_auto: false,
        margin_right_auto: false,
    }
}

/// Helper to create three items with basis 50.
#[inline]
pub fn three_items_50() -> Vec<FlexChild> {
    vec![
        item_zero_margins(1, 50.0),
        item_zero_margins(2, 50.0),
        item_zero_margins(3, 50.0),
    ]
}
