//! Line packing and cross-placement for multi-line flex containers.

use log::debug;

use super::super::cross_axis::alignment::align_single_line_cross;
use super::super::cross_axis::{
    BaselineAdjustCtx, adjust_cross_for_baseline, compute_line_baseline_ref,
};
use super::super::distribution::clamp;
use super::super::layout_single_line;
use super::super::{
    CrossContext, CrossPlacement, FlexContainerInputs, FlexPlacement, JustifyContent,
};
use super::align_content::{PackInputs, align_content_params};
use super::line_breaking::LineRange;

/// Type alias for per-line main placements.
type PerLineMain = Vec<FlexPlacement>;

/// Per line, compute single-line main-axis placements and the maximum clamped cross-size.
/// Returns `(per_line_main, per_line_cross_max)` where `per_line_main[i]` pairs with
/// `per_line_cross_max[i]`.
pub fn per_line_main_and_cross(
    container: FlexContainerInputs,
    justify_content: JustifyContent,
    items: &[super::super::FlexChild],
    cross_inputs: &[(f32, f32, f32)],
    line_ranges: &[LineRange],
) -> (Vec<PerLineMain>, Vec<f32>) {
    let mut per_line_main: Vec<PerLineMain> = Vec::with_capacity(line_ranges.len());
    let mut per_line_cross_max: Vec<f32> = Vec::with_capacity(line_ranges.len());
    for (line_idx, (line_start, line_end)) in line_ranges.iter().copied().enumerate() {
        let (Some(line_items), Some(line_cross_inputs)) = (
            items.get(line_start..line_end),
            cross_inputs.get(line_start..line_end),
        ) else {
            return (Vec::new(), Vec::new());
        };
        debug!(
            "[MULTI-LINE] Line {}: items=[{}..{}) count={}",
            line_idx,
            line_start,
            line_end,
            line_items.len()
        );
        let line_main = layout_single_line(container, justify_content, line_items);
        debug!(
            "[MULTI-LINE] Line {} main results: {:?}",
            line_idx,
            line_main
                .iter()
                .map(|placement| (placement.main_offset, placement.main_size))
                .collect::<Vec<_>>()
        );
        let mut line_cross_max = 0.0f32;
        for (idx, &(item_cross, min_c, max_c)) in line_cross_inputs.iter().enumerate() {
            let clamped = clamp(item_cross, min_c, max_c);
            // Include cross-axis margins per CSS Flexbox spec
            // For row direction, this is margin-top + margin-bottom
            let global_item_idx = line_start + idx;
            let item_margin_cross =
                items[global_item_idx].margin_top + items[global_item_idx].margin_bottom;
            let total_cross = clamped + item_margin_cross;
            if total_cross > line_cross_max {
                line_cross_max = total_cross;
            }
        }
        debug!("[MULTI-LINE] Line {line_idx} cross_max={line_cross_max:.1}");
        per_line_main.push(line_main);
        per_line_cross_max.push(line_cross_max);
    }
    (per_line_main, per_line_cross_max)
}

/// Per-line context for building `(FlexPlacement, CrossPlacement)` pairs.
struct LineBuildCtx<'ctx> {
    /// Cross-axis and alignment parameters.
    cross_ctx: &'ctx CrossContext,
    /// Main placements for items in the line.
    line_main: &'ctx PerLineMain,
    /// Cross inputs for items in the line.
    line_cross_inputs: &'ctx [(f32, f32, f32)],
    /// Baseline metrics for items in the line.
    line_baselines: &'ctx [super::super::cross_axis::BaselineMetrics],
    /// Max cross-size for the line.
    line_cross_max: f32,
    /// Reference baseline value for the line.
    line_ref: f32,
    /// Accumulated cross offset before this line.
    cross_accum_offset: f32,
}

/// Build `(FlexPlacement, CrossPlacement)` pairs for a single line.
fn build_line_pairs(ctx: &LineBuildCtx<'_>) -> Vec<(FlexPlacement, CrossPlacement)> {
    ctx.line_main
        .iter()
        .zip(ctx.line_cross_inputs.iter())
        .enumerate()
        .map(
            |(index_in_line, (main_place, &(item_cross, min_c, max_c)))| {
                let within_line = align_single_line_cross(
                    ctx.cross_ctx.align_items,
                    ctx.line_cross_max,
                    item_cross,
                    min_c,
                    max_c,
                );
                let mut lifted = CrossPlacement {
                    cross_size: within_line.cross_size,
                    cross_offset: ctx.cross_accum_offset + within_line.cross_offset,
                };
                let bctx = BaselineAdjustCtx {
                    align: ctx.cross_ctx.align_items,
                    baselines: ctx.line_baselines,
                    index_in_line,
                    line_ref: ctx.line_ref,
                    line_cross_max: ctx.line_cross_max,
                    cross_accum_offset: ctx.cross_accum_offset,
                };
                adjust_cross_for_baseline(&bctx, &mut lifted);
                (*main_place, lifted)
            },
        )
        .collect()
}

/// Pack stretched line boxes along the cross axis according to `align-content`, adding cross-axis
/// gaps between adjacent lines, and build the final `(FlexPlacement, CrossPlacement)` pairs.
pub fn pack_lines_and_build(
    cross_ctx: &CrossContext,
    inputs: &PackInputs<'_>,
) -> Vec<(FlexPlacement, CrossPlacement)> {
    let line_count = inputs.line_cross_vec.len();
    let sum_lines = inputs.line_cross_vec.iter().copied().sum::<f32>();
    let gaps_total = if line_count > 1 {
        (line_count as f32 - 1.0) * cross_ctx.cross_gap.max(0.0)
    } else {
        0.0
    };
    let (start_offset, between_spacing) = align_content_params(
        cross_ctx.align_content,
        cross_ctx.container_cross_size,
        sum_lines + gaps_total,
        line_count,
    );
    let mut cross_accum_offset = start_offset;
    let capacity: usize = inputs.per_line_main.iter().map(Vec::len).sum();
    let mut results: Vec<(FlexPlacement, CrossPlacement)> = Vec::with_capacity(capacity);
    let last_index = inputs.line_ranges.len().saturating_sub(1);
    for (line_index, (line_start, line_end)) in inputs.line_ranges.iter().copied().enumerate() {
        let Some(line_cross_inputs) = inputs.cross_inputs.get(line_start..line_end) else {
            return Vec::new();
        };
        let line_baselines = inputs
            .baseline_inputs
            .get(line_start..line_end)
            .unwrap_or(&[]);
        let line_main = inputs
            .per_line_main
            .get(line_index)
            .cloned()
            .unwrap_or_default();
        let line_cross_max = inputs
            .line_cross_vec
            .get(line_index)
            .copied()
            .unwrap_or(0.0);
        let line_ref = if matches!(
            cross_ctx.align_items,
            super::super::AlignItems::Baseline | super::super::AlignItems::LastBaseline
        ) {
            compute_line_baseline_ref(cross_ctx.align_items, line_baselines, line_cross_inputs)
        } else {
            0.0
        };
        let line_pairs = build_line_pairs(&LineBuildCtx {
            cross_ctx,
            line_main: &line_main,
            line_cross_inputs,
            line_baselines,
            line_cross_max,
            line_ref,
            cross_accum_offset,
        });
        results.extend(line_pairs);
        cross_accum_offset += line_cross_max + between_spacing;
        if line_index < last_index {
            cross_accum_offset += cross_ctx.cross_gap.max(0.0);
        }
    }
    results
}
