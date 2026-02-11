//! Tests for auto margin behavior in flexbox.

use super::*;

#[test]
/// # Panics
/// Panics if auto margin absorption does not push the item to the end correctly.
fn auto_margin_single_end_absorbs_free_space() {
    let container = FlexContainerInputs {
        direction: FlexDirection::Row,
        writing_mode: WritingMode::HorizontalTb,
        container_main_size: 300.0,
        main_gap: 0.0,
    };
    let mut items = vec![item_zero_margins(1, 100.0)];
    // margin-left:auto should push the item to the end by absorbing remaining space at start.
    if let Some(first_item) = items.first_mut() {
        first_item.margin_left_auto = true;
    }
    let out = layout_single_line(container, JustifyContent::Center, &items);
    assert_eq!(out.len(), 1);
    // Remaining space = 200 -> offset at start equals 200.
    for placement in &out {
        assert!((placement.main_offset - 200.0).abs() < 0.001);
        assert!((placement.main_size - 100.0).abs() < 0.001);
    }
}

#[test]
/// # Panics
/// Panics if auto margins on both sides do not center the item.
fn auto_margins_both_center_item() {
    let container = FlexContainerInputs {
        direction: FlexDirection::Row,
        writing_mode: WritingMode::HorizontalTb,
        container_main_size: 300.0,
        main_gap: 0.0,
    };
    let mut items = vec![item_zero_margins(1, 100.0)];
    if let Some(first_item) = items.first_mut() {
        first_item.margin_left_auto = true;
        first_item.margin_right_auto = true;
    }
    let out = layout_single_line(container, JustifyContent::Start, &items);
    assert_eq!(out.len(), 1);
    // Remaining space = 200 -> split equally -> offset 100.
    for placement in &out {
        assert!((placement.main_offset - 100.0).abs() < 0.001);
    }
}

#[test]
/// # Panics
/// Panics if multiple auto margins do not share remaining space equally.
fn auto_margins_multiple_items_share_space() {
    let container = FlexContainerInputs {
        direction: FlexDirection::Row,
        writing_mode: WritingMode::HorizontalTb,
        container_main_size: 400.0,
        main_gap: 0.0,
    };
    let mut items = vec![
        item_zero_margins(1, 50.0),
        item_zero_margins(2, 50.0),
        item_zero_margins(3, 50.0),
    ];
    // Two auto slots: first has left auto, third has right auto.
    if let Some(first_item) = items.first_mut() {
        first_item.margin_left_auto = true;
    }
    if let Some(third_item) = items.get_mut(2) {
        third_item.margin_right_auto = true;
    }
    let out = layout_single_line(container, JustifyContent::SpaceBetween, &items);
    assert_eq!(out.len(), 3);
    // Total inner = 150, remaining = 250, two slots -> 125 each.
    // Item 1 offset = 125 (auto left), Item 2 offset = 125 + 50 = 175,
    // plus no extra spacing beyond margins in this test.
    for (index, placement) in out.iter().enumerate() {
        if index == 0 {
            assert!((placement.main_offset - 125.0).abs() < 0.001);
        }
        if index == 1 {
            assert!((placement.main_offset - 175.0).abs() < 0.001);
        }
    }
}
