//! Tests for cross-axis alignment in flexbox.

use super::*;

#[test]
/// # Panics
/// Panics if center alignment does not center the item within the container cross-size.
fn align_items_center_cross_axis() {
    let placement = align_single_line_cross(
        AlignItems::Center,
        200.0,
        CrossSize::Explicit(100.0),
        0.0,
        1e9,
    );
    assert!(
        (placement.cross_size - 100.0).abs() < 0.001,
        "size remains item size"
    );
    assert!(
        (placement.cross_offset - 50.0).abs() < 0.001,
        "offset should center item"
    );
}

#[test]
/// # Panics
/// Panics if stretch alignment does not expand to container cross-size respecting constraints.
fn align_items_stretch_cross_axis() {
    // When item cross-size is auto/unspecified, Stretch expands to container size.
    let placement = align_single_line_cross(
        AlignItems::Stretch,
        120.0,
        CrossSize::Stretch(0.0),
        0.0,
        1e9,
    );
    assert!(
        (placement.cross_size - 120.0).abs() < 0.001,
        "stretched to container size"
    );
    assert!(
        (placement.cross_offset - 0.0).abs() < 0.001,
        "stretched offset should be 0"
    );
}

#[test]
/// # Panics
/// Panics if bulk cross-axis alignment does not mirror per-item alignment.
fn align_cross_for_items_bulk_matches_scalar() {
    let items: Vec<(CrossSize, f32, f32)> = vec![
        (CrossSize::Explicit(10.0), 0.0, 1000.0),
        (CrossSize::Explicit(20.0), 0.0, 1000.0),
        (CrossSize::Explicit(30.0), 0.0, 1000.0),
    ];
    let bulk = align_cross_for_items(AlignItems::Center, 100.0, &items);
    let scalar: Vec<CrossPlacement> = items
        .iter()
        .map(|&(size, min_c, max_c)| {
            align_single_line_cross(AlignItems::Center, 100.0, size, min_c, max_c)
        })
        .collect();
    assert_eq!(bulk.len(), scalar.len(), "bulk and scalar lengths differ");
    for (bulk_cp, scalar_cp) in bulk.iter().zip(scalar.iter()) {
        assert!((bulk_cp.cross_size - scalar_cp.cross_size).abs() < 0.0001);
        assert!((bulk_cp.cross_offset - scalar_cp.cross_offset).abs() < 0.0001);
    }
}

#[test]
/// # Panics
/// Panics if combined API does not pair main and cross placements correctly.
fn layout_single_line_with_cross_pairs_outputs() {
    let items = vec![
        FlexChild {
            handle: ItemRef(1),
            flex_basis: 50.0,
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
            main_padding_border: 0.0,
        },
        FlexChild {
            handle: ItemRef(2),
            flex_basis: 50.0,
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
            main_padding_border: 0.0,
        },
    ];
    let cross_inputs = vec![
        (CrossSize::Explicit(20.0), 0.0, 100.0),
        (CrossSize::Explicit(20.0), 0.0, 100.0),
    ];
    let container = FlexContainerInputs {
        direction: FlexDirection::Row,
        writing_mode: WritingMode::HorizontalTb,
        container_main_size: 200.0,
        main_gap: 0.0,
    };
    let cross_ctx = CrossContext {
        align_items: AlignItems::Center,
        align_content: AlignContent::Start,
        container_cross_size: 100.0,
        cross_gap: 0.0,
    };
    let out = layout_single_line_with_cross(
        container,
        JustifyContent::Center,
        cross_ctx,
        &items,
        CrossAndBaseline {
            cross_inputs: &cross_inputs,
            baseline_inputs: &[None, None],
        },
    );
    assert_eq!(out.len(), 2);
    for (idx, pair) in out.iter().enumerate() {
        let main_cp = &pair.0;
        assert_eq!(
            main_cp.handle.0,
            (idx as u64) + 1u64,
            "handles must align with input order"
        );
    }
    for pair in &out {
        let cross_cp = &pair.1;
        assert!((cross_cp.cross_size - 20.0).abs() < 0.001);
        assert!((cross_cp.cross_offset - 40.0).abs() < 0.001);
    }
}
