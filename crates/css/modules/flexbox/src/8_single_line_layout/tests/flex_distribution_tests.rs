//! Tests for flex-grow and flex-shrink distribution.

use super::*;

#[test]
/// Ensures flex-grow respects `max_main` and redistributes remaining space.
///
/// # Panics
/// Panics if the produced sizes do not meet clamped/redistribution invariants.
fn grow_respects_max_and_redistributes() {
    let container = FlexContainerInputs {
        direction: FlexDirection::Row,
        writing_mode: WritingMode::HorizontalTb,
        container_main_size: 300.0,
        main_gap: 0.0,
    };
    let mut items = three_items_50(); // basis sum = 150
    // Remaining = 150; saturate first item at 80 max.
    if let Some(first) = items.get_mut(0) {
        first.flex_grow = 1.0;
        first.max_main = 80.0;
    }
    if let Some(second) = items.get_mut(1) {
        second.flex_grow = 1.0;
    }
    if let Some(third) = items.get_mut(2) {
        third.flex_grow = 1.0;
    }
    let out = layout_single_line(container, JustifyContent::Start, &items);
    assert_eq!(out.len(), 3);
    let sizes: Vec<f32> = out.iter().map(|placement| placement.main_size).collect();
    // First must clamp to 80
    let first_size = sizes.first().copied().unwrap_or(0.0);
    assert!((first_size - 80.0).abs() < 0.01);
    // Total equals container
    let total: f32 = sizes.iter().sum();
    assert!((total - 300.0).abs() < 0.01);
    // Others grew beyond base
    let second_size = sizes.get(1).copied().unwrap_or(0.0);
    let third_size = sizes.get(2).copied().unwrap_or(0.0);
    assert!(second_size >= 50.0 && third_size >= 50.0);
}

#[test]
/// Ensures flex-shrink respects `min_main` and freezes at min.
///
/// # Panics
/// Panics if the produced sizes violate min constraints or total does not equal container.
fn shrink_respects_min_and_freezes() {
    let container = FlexContainerInputs {
        direction: FlexDirection::Row,
        writing_mode: WritingMode::HorizontalTb,
        container_main_size: 100.0,
        main_gap: 0.0,
    };
    let mut items = three_items_50(); // sum=150, need shrink by 50
    for child in &mut items {
        child.flex_shrink = 1.0;
    }
    if let Some(first) = items.get_mut(0) {
        first.min_main = 40.0; // can only shrink by 10
    }
    let out = layout_single_line(container, JustifyContent::Start, &items);
    assert_eq!(out.len(), 3);
    let sizes: Vec<f32> = out.iter().map(|placement| placement.main_size).collect();
    // First must clamp to 40
    let first_size = sizes.first().copied().unwrap_or(0.0);
    assert!((first_size - 40.0).abs() < 0.01);
    // Total equals container
    let total: f32 = sizes.iter().sum();
    assert!((total - 100.0).abs() < 0.01);
    // Others shrank but stayed within [0,50]
    let second_size = sizes.get(1).copied().unwrap_or(0.0);
    let third_size = sizes.get(2).copied().unwrap_or(0.0);
    assert!((0.0..=50.0).contains(&second_size));
    assert!((0.0..=50.0).contains(&third_size));
}

#[test]
/// # Panics
/// Panics if sizes or offsets deviate from the expected results for a simple grow case.
fn grow_distribution_and_placement_row() {
    let items = vec![
        FlexChild {
            handle: ItemRef(1),
            flex_basis: 50.0,
            flex_grow: 1.0,
            flex_shrink: 1.0,
            min_main: 0.0,
            max_main: 1e9,
            margin_left: 0.0,
            margin_right: 0.0,
            margin_top: 0.0,
            margin_bottom: 0.0,
            margin_left_auto: false,
            margin_right_auto: false,
        },
        FlexChild {
            handle: ItemRef(2),
            flex_basis: 50.0,
            flex_grow: 1.0,
            flex_shrink: 1.0,
            min_main: 0.0,
            max_main: 1e9,
            margin_left: 0.0,
            margin_right: 0.0,
            margin_top: 0.0,
            margin_bottom: 0.0,
            margin_left_auto: false,
            margin_right_auto: false,
        },
    ];
    let container = FlexContainerInputs {
        direction: FlexDirection::Row,
        writing_mode: WritingMode::HorizontalTb,
        container_main_size: 200.0,
        main_gap: 0.0,
    };
    let out = layout_single_line(container, JustifyContent::Start, &items);
    assert_eq!(out.len(), 2, "expected two placements for two items");
    // Free space = 100, each grows by 50
    let expected_sizes = [100.0f32, 100.0f32];
    for (got, expect) in out
        .iter()
        .map(|placement| placement.main_size)
        .zip(expected_sizes)
    {
        assert!(
            (got - expect).abs() < 0.001,
            "unexpected item size: got {got} expect {expect}"
        );
    }
    let expected_offsets = [0.0f32, 100.0f32];
    for (got, expect) in out
        .iter()
        .map(|placement| placement.main_offset)
        .zip(expected_offsets)
    {
        assert!(
            (got - expect).abs() < 0.001,
            "unexpected offset: got {got} expect {expect}"
        );
    }
}

#[test]
/// # Panics
/// Panics if shrink distribution or reverse/center placement conditions are violated.
fn shrink_distribution_row_reverse_center() {
    let items = vec![
        FlexChild {
            handle: ItemRef(1),
            flex_basis: 120.0,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            min_main: 20.0,
            max_main: 1e9,
            margin_left: 0.0,
            margin_right: 0.0,
            margin_top: 0.0,
            margin_bottom: 0.0,
            margin_left_auto: false,
            margin_right_auto: false,
        },
        FlexChild {
            handle: ItemRef(2),
            flex_basis: 80.0,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            min_main: 20.0,
            max_main: 1e9,
            margin_left: 0.0,
            margin_right: 0.0,
            margin_top: 0.0,
            margin_bottom: 0.0,
            margin_left_auto: false,
            margin_right_auto: false,
        },
    ];
    let container = FlexContainerInputs {
        direction: FlexDirection::RowReverse,
        writing_mode: WritingMode::HorizontalTb,
        container_main_size: 160.0,
        main_gap: 0.0,
    };
    let out = layout_single_line(container, JustifyContent::Center, &items);
    assert_eq!(out.len(), 2, "expected two placements for two items");
    let total: f32 = out.iter().map(|placement| placement.main_size).sum();
    assert!(
        (total - 160.0).abs() < 0.001,
        "total size must equal container main size"
    );
    // Centered: minimal offset should be >= 0
    let first_offset = out.first().map_or(0.0, |placement| placement.main_offset);
    assert!(
        first_offset >= 0.0,
        "centered layout must not start before 0"
    );
    // Reverse places earlier logical item at a larger main coordinate (strictly descending offsets)
    let mut previous = f32::INFINITY;
    for offset in out.iter().map(|placement| placement.main_offset) {
        assert!(
            previous > offset,
            "offsets should strictly descend in row-reverse"
        );
        previous = offset;
    }
}
