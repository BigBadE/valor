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
    is_row_direction: bool,
) -> Vec<LineRange> {
    use log::debug;

    let mut line_ranges: Vec<(usize, usize)> = Vec::new();
    let mut start = 0usize;
    let mut cursor = 0.0f32;

    debug!(
        "[LINE-BREAK] container_main_size={} main_gap={} is_row={}",
        container_main_size, main_gap, is_row_direction
    );

    let mut last_non_zero_idx: Option<usize> = None;

    for (idx, child) in items.iter().copied().enumerate() {
        // For row direction, use left/right margins; for column direction, use top/bottom
        let (margin_start, margin_end) = if is_row_direction {
            (child.margin_left, child.margin_right)
        } else {
            (child.margin_top, child.margin_bottom)
        };
        let size = clamp(child.flex_basis, child.min_main, child.max_main)
            + margin_start.max(0.0)
            + margin_end.max(0.0);

        // Only add gap if there was a previous non-zero item in this line
        let gap = if let Some(last_idx) = last_non_zero_idx {
            if last_idx >= start && size > 0.0 {
                main_gap.max(0.0)
            } else {
                0.0
            }
        } else {
            0.0
        };

        let next = cursor + gap + size;

        debug!(
            "[LINE-BREAK] item[{}]: basis={:.1} margins=({:.1},{:.1}) size={:.1} gap={:.1} cursor={:.1} next={:.1}",
            idx, child.flex_basis, margin_start, margin_end, size, gap, cursor, next
        );

        let is_first_in_line = idx == start;

        if next <= container_main_size || is_first_in_line {
            cursor = next;
            if size > 0.0 {
                last_non_zero_idx = Some(idx);
            }
            debug!(
                "[LINE-BREAK] item[{}]: fits in line, cursor now {:.1}",
                idx, cursor
            );
        } else if idx > start {
            line_ranges.push((start, idx));
            debug!(
                "[LINE-BREAK] item[{}]: BREAK LINE, new line starts at {}",
                idx, idx
            );
            start = idx;
            cursor = size;
            if size > 0.0 {
                last_non_zero_idx = Some(idx);
            }
        }
    }
    if start < items.len() {
        line_ranges.push((start, items.len()));
    }

    debug!("[LINE-BREAK] Final line_ranges: {:?}", line_ranges);
    line_ranges
}
