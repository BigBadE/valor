//! Tests for multi-line flexbox wrapping behavior.

use super::*;

#[test]
/// # Panics
/// Panics if multi-line wrapping does not break into two lines correctly or cross stacking is wrong.
fn multi_line_wrap_basic_two_lines() {
    // Three items of 50 each, gap 10, container 120 → line 1 has two items (50+10+50=110), line 2 has one item.
    let items = three_items_50();
    let cross_inputs = vec![
        (20.0, 0.0, 1000.0),
        (20.0, 0.0, 1000.0),
        (20.0, 0.0, 1000.0),
    ];
    let container = FlexContainerInputs {
        direction: FlexDirection::Row,
        writing_mode: WritingMode::HorizontalTb,
        container_main_size: 120.0,
        main_gap: 10.0,
    };
    let cross_ctx = CrossContext {
        align_items: AlignItems::Center,
        align_content: AlignContent::Start,
        container_cross_size: 100.0,
        cross_gap: 0.0,
    };

    let out = layout_multi_line_with_cross(
        container,
        JustifyContent::Start,
        cross_ctx,
        &items,
        CrossAndBaseline {
            cross_inputs: &cross_inputs,
            baseline_inputs: &[None, None, None],
        },
    );
    assert_eq!(out.len(), 3, "expected three placements");

    // Verify main offsets and cross stacking by index without indexing operations.
    let expected_pairs = [(0.0, 0.0), (60.0, 0.0), (0.0, 20.0)];
    for ((got_main, got_cross), (exp_main, exp_cross)) in out
        .iter()
        .map(|pair| (pair.0.main_offset, pair.1.cross_offset))
        .zip(expected_pairs)
    {
        assert!((got_main - exp_main).abs() < 0.001);
        assert!((got_cross - exp_cross).abs() < 0.001);
    }
}

#[test]
/// Ensures margin-left:auto on the first item of the first line absorbs remaining space on that line.
///
/// # Panics
/// Panics if the first item's offset does not equal the per-line remaining space.
fn multi_line_auto_margin_left_first_line_absorbs_space() {
    // Four items of 50 each, gap 10, container 120 → lines: [0,2) and [2,4)
    let items_line = vec![
        item_zero_margins(1, 50.0),
        item_zero_margins(2, 50.0),
        item_zero_margins(3, 50.0),
        item_zero_margins(4, 50.0),
    ];
    let mut items_with_auto = items_line;
    if let Some(first) = items_with_auto.get_mut(0) {
        first.margin_left_auto = true;
    }
    let cross_inputs = vec![(20.0, 0.0, 1000.0); 4];
    let container = FlexContainerInputs {
        direction: FlexDirection::Row,
        writing_mode: WritingMode::HorizontalTb,
        container_main_size: 120.0,
        main_gap: 10.0,
    };
    let cross_ctx = CrossContext {
        align_items: AlignItems::Center,
        align_content: AlignContent::Start,
        container_cross_size: 100.0,
        cross_gap: 0.0,
    };
    let out = layout_multi_line_with_cross(
        container,
        JustifyContent::Start,
        cross_ctx,
        &items_with_auto,
        CrossAndBaseline {
            cross_inputs: &cross_inputs,
            baseline_inputs: &[None, None, None, None],
        },
    );
    // Per first line: inner sum 100, one gap 10 → remaining = 120 - 110 = 10 → first item offset should be 10.
    let first_offset = out.first().map_or(0.0, |pair| pair.0.main_offset);
    assert!((first_offset - 10.0).abs() < 0.001);
}

#[test]
/// Ensures a single-item line with both auto margins centers the item within that line.
///
/// # Panics
/// Panics if the offset is not half of the per-line remaining space.
fn multi_line_single_item_both_auto_centers() {
    // Construct lines: first line uses two items to force a second line with one item.
    // Container 120, gap 10; items: [50, 50] then [50]
    let mut items = vec![
        item_zero_margins(1, 50.0),
        item_zero_margins(2, 50.0),
        item_zero_margins(3, 50.0),
    ];
    if let Some(last) = items.get_mut(2) {
        last.margin_left_auto = true;
        last.margin_right_auto = true;
    }
    let cross_inputs = vec![(20.0, 0.0, 1000.0); 3];
    let container = FlexContainerInputs {
        direction: FlexDirection::Row,
        writing_mode: WritingMode::HorizontalTb,
        container_main_size: 120.0,
        main_gap: 10.0,
    };
    let cross_ctx = CrossContext {
        align_items: AlignItems::Center,
        align_content: AlignContent::Start,
        container_cross_size: 100.0,
        cross_gap: 0.0,
    };
    let out = layout_multi_line_with_cross(
        container,
        JustifyContent::Start,
        cross_ctx,
        &items,
        CrossAndBaseline {
            cross_inputs: &cross_inputs,
            baseline_inputs: &[None, None, None],
        },
    );
    // First line consumes two placements, third is in second line.
    // Remaining on line 2: container 120 - inner 50 - gaps 0 = 70 → both auto -> left margin = 35 → offset 35
    let third_offset = out.get(2).map_or(0.0, |pair| pair.0.main_offset);
    assert!((third_offset - 35.0).abs() < 0.001);
}
