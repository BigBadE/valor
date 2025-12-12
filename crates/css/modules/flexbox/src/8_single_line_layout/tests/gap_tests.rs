//! Tests for CSS gap behavior in flexbox.

use super::*;

#[test]
/// Ensures main-axis CSS gap influences between-spacing for items (simulating percentage-like gap via computed px).
///
/// # Panics
/// Panics if offsets are not separated by the specified gap in a simple Start layout.
fn main_gap_affects_between_offsets() {
    // Simulate a 10% main gap on a 200px container → 20px
    let items = three_items_50();
    let container = FlexContainerInputs {
        direction: FlexDirection::Row,
        writing_mode: WritingMode::HorizontalTb,
        container_main_size: 200.0,
        main_gap: 20.0, // as if resolved from percentage
    };
    let out = layout_single_line(container, JustifyContent::Start, &items);
    assert_eq!(out.len(), 3);
    // Expected offsets: 0, 50+20=70, 70+50+20=140
    let expected_offsets = [0.0f32, 70.0f32, 140.0f32];
    for (got, expect) in out
        .iter()
        .map(|placement| placement.main_offset)
        .zip(expected_offsets)
    {
        assert!((got - expect).abs() < 0.001);
    }
}
