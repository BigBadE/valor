//! Tests for baseline alignment in flexbox.

use super::*;

#[test]
/// Ensures first baseline alignment positions items so their first baselines line up (single line).
///
/// # Panics
/// Panics if any baseline metric is missing or baseline positions do not align.
fn baseline_alignment_single_line_first() {
    let container = FlexContainerInputs {
        direction: FlexDirection::Row,
        writing_mode: WritingMode::HorizontalTb,
        container_main_size: 300.0,
        main_gap: 0.0,
    };
    let items = vec![
        item_zero_margins(1, 50.0),
        item_zero_margins(2, 50.0),
        item_zero_margins(3, 50.0),
    ];
    let cross_inputs = vec![
        (CrossSize::Explicit(30.0), 0.0, 1000.0),
        (CrossSize::Explicit(40.0), 0.0, 1000.0),
        (CrossSize::Explicit(20.0), 0.0, 1000.0),
    ];
    let baseline_inputs = vec![Some((10.0, 25.0)), Some((15.0, 35.0)), Some((5.0, 10.0))];
    let cab = CrossAndBaseline {
        cross_inputs: &cross_inputs,
        baseline_inputs: &baseline_inputs,
    };
    let ctx = CrossContext {
        align_items: AlignItems::Baseline,
        align_content: AlignContent::Start,
        container_cross_size: 50.0,
        cross_gap: 0.0,
    };
    let pairs = layout_single_line_with_cross(container, JustifyContent::Start, ctx, &items, cab);
    let line_ref = compute_line_baseline_ref(AlignItems::Baseline, &baseline_inputs, &cross_inputs);
    for (cross, baseline_opt) in pairs.iter().map(|pair| &pair.1).zip(baseline_inputs.iter()) {
        assert!(
            baseline_opt.is_some(),
            "missing baseline metrics in test input"
        );
        if let Some((first, _last)) = *baseline_opt {
            let baseline_pos = cross.cross_offset + first;
            assert!((baseline_pos - line_ref).abs() <= 1.0 / 64.0);
        }
    }
}

#[test]
/// Ensures last baseline alignment positions items so their last baselines line up (single line).
///
/// # Panics
/// Panics if any baseline metric is missing or baseline positions do not align.
fn baseline_alignment_single_line_last() {
    let container = FlexContainerInputs {
        direction: FlexDirection::Row,
        writing_mode: WritingMode::HorizontalTb,
        container_main_size: 300.0,
        main_gap: 0.0,
    };
    let items = vec![item_zero_margins(1, 60.0), item_zero_margins(2, 60.0)];
    let cross_inputs = vec![
        (CrossSize::Explicit(32.0), 0.0, 1000.0),
        (CrossSize::Explicit(28.0), 0.0, 1000.0),
    ];
    let baseline_inputs = vec![Some((8.0, 28.0)), Some((6.0, 22.0))];
    let cab = CrossAndBaseline {
        cross_inputs: &cross_inputs,
        baseline_inputs: &baseline_inputs,
    };
    let ctx = CrossContext {
        align_items: AlignItems::LastBaseline,
        align_content: AlignContent::Start,
        container_cross_size: 40.0,
        cross_gap: 0.0,
    };
    let pairs = layout_single_line_with_cross(container, JustifyContent::Start, ctx, &items, cab);
    let line_ref =
        compute_line_baseline_ref(AlignItems::LastBaseline, &baseline_inputs, &cross_inputs);
    for (cross, baseline_opt) in pairs.iter().map(|pair| &pair.1).zip(baseline_inputs.iter()) {
        assert!(
            baseline_opt.is_some(),
            "missing baseline metrics in test input"
        );
        if let Some((_first, last)) = *baseline_opt {
            let baseline_pos = cross.cross_offset + last;
            assert!((baseline_pos - line_ref).abs() <= 1.0 / 64.0);
        }
    }
}

#[test]
/// Ensures baseline alignment works per-line in multi-line wrapping with per-line references.
///
/// # Panics
/// Panics if any baseline metric is missing or baseline positions do not align per line.
fn baseline_alignment_multi_line() {
    let container = FlexContainerInputs {
        direction: FlexDirection::Row,
        writing_mode: WritingMode::HorizontalTb,
        container_main_size: 170.0, // two items per line: 80 + 10 + 80
        main_gap: 10.0,
    };
    let items = vec![
        item_zero_margins(1, 80.0),
        item_zero_margins(2, 80.0),
        item_zero_margins(3, 80.0),
        item_zero_margins(4, 80.0),
    ];
    let cross_inputs = vec![
        (CrossSize::Explicit(24.0), 0.0, 1000.0),
        (CrossSize::Explicit(30.0), 0.0, 1000.0),
        (CrossSize::Explicit(18.0), 0.0, 1000.0),
        (CrossSize::Explicit(26.0), 0.0, 1000.0),
    ];
    let baseline_inputs = vec![
        Some((6.0, 20.0)),
        Some((10.0, 26.0)),
        Some((5.0, 15.0)),
        Some((8.0, 22.0)),
    ];
    let cab = CrossAndBaseline {
        cross_inputs: &cross_inputs,
        baseline_inputs: &baseline_inputs,
    };
    let ctx = CrossContext {
        align_items: AlignItems::Baseline,
        align_content: AlignContent::Start,
        container_cross_size: 60.0,
        cross_gap: 4.0,
    };
    let pairs = layout_multi_line_with_cross(container, JustifyContent::Start, ctx, &items, cab);
    // Validate in chunks of 2 items per line without slicing/indexing.
    let mut bi_iter = baseline_inputs.iter();
    let mut ci_iter = cross_inputs.iter();
    for pair_chunk in pairs.chunks_exact(2) {
        let b_chunk: Vec<_> = bi_iter.by_ref().take(2).collect();
        let c_chunk: Vec<_> = ci_iter.by_ref().take(2).collect();
        // Convert borrowed chunks to owned slices matching function signatures.
        let b_owned: Vec<_> = b_chunk.iter().map(|opt| **opt).collect();
        let c_owned: Vec<_> = c_chunk.iter().map(|tpl| **tpl).collect();
        let ref_val = compute_line_baseline_ref(AlignItems::Baseline, &b_owned, &c_owned);
        // Determine the line's accumulated cross offset (top of the line) as the minimum cross_offset in the chunk.
        let line_start_offset = pair_chunk
            .iter()
            .map(|pair| pair.1.cross_offset)
            .fold(f32::INFINITY, f32::min);
        for (pair, baseline) in pair_chunk.iter().zip(b_owned.iter()) {
            let cross = &pair.1;
            assert!(baseline.is_some(), "missing baseline metrics in test input");
            if let Some((first, _last)) = *baseline {
                let baseline_pos = cross.cross_offset + first;
                let target = line_start_offset + ref_val;
                assert!((baseline_pos - target).abs() <= 1.0 / 64.0);
            }
        }
    }
}
