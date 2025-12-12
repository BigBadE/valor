//! Line breaking logic for multi-line flex layouts.

use super::super::FlexChild;
use super::super::distribution::clamp;

/// Line start/end indices for items included in the line: `[start, end)`.
pub type LineRange = (usize, usize);

/// Break items into lines by accumulating hypothetical sizes and `main_gap` until exceeding
/// `container_main_size`. Returns a list of `[start, end)` ranges.
pub fn break_into_lines(
    container_main_size: f32,
    main_gap: f32,
    items: &[FlexChild],
) -> Vec<LineRange> {
    let mut line_ranges: Vec<(usize, usize)> = Vec::new();
    let mut start = 0usize;
    let mut cursor = 0.0f32;
    for (idx, child) in items.iter().copied().enumerate() {
        let size = clamp(child.flex_basis, child.min_main, child.max_main)
            + child.margin_left.max(0.0)
            + child.margin_right.max(0.0);
        let is_first_in_line = idx == start;
        let gap = if is_first_in_line {
            0.0
        } else {
            main_gap.max(0.0)
        };
        let next = cursor + gap + size;
        if next <= container_main_size || is_first_in_line {
            cursor = next;
        } else if idx > start {
            line_ranges.push((start, idx));
            start = idx;
            cursor = size;
        }
    }
    if start < items.len() {
        line_ranges.push((start, items.len()));
    }
    line_ranges
}
