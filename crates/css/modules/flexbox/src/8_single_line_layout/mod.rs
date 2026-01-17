//! Single-line flex layout algorithm implementation.
//!
//! This module implements the CSS Flexbox Level 1 specification's single-line layout
//! algorithm, including flex grow/shrink, justify-content, and align-items.

use crate::{FlexDirection, ItemRef, WritingMode, resolve_axes};
use log::debug;

mod cross_axis;
mod distribution;
mod multi_line;

pub use cross_axis::BaselineMetrics;
pub use cross_axis::alignment::{CrossSize, align_cross_for_items, align_single_line_cross};
use cross_axis::compute_line_baseline_ref;
use distribution::{
    MainOffsetPlan, accumulate_main_offsets, build_main_placements, clamp,
    clamp_first_offset_if_needed, distribute_grow, distribute_shrink, justify_params,
    resolve_auto_margins_and_outer,
};
use multi_line::{
    PackInputs, break_into_lines, pack_lines_and_build, per_line_main_and_cross,
    stretch_line_crosses,
};

/// Bundle of cross and baseline inputs required by the combined layout APIs.
#[derive(Copy, Clone)]
pub struct CrossAndBaseline<'cb> {
    /// Per-item cross inputs `(cross_size, min_cross, max_cross)`
    pub cross_inputs: &'cb [(CrossSize, f32, f32)],
    /// Per-item baseline metrics if available
    pub baseline_inputs: &'cb [BaselineMetrics],
}

/// Inputs for a flex item needed for single-line main-axis sizing.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct FlexChild {
    pub handle: ItemRef,
    /// Flex base size (used as hypothetical main size before flexing), in CSS px.
    /// This is the **content-box** (inner) size, used for shrink weight calculation.
    pub flex_basis: f32,
    /// Flex grow factor (>= 0).
    pub flex_grow: f32,
    /// Flex shrink factor (>= 0).
    pub flex_shrink: f32,
    /// Min main size constraint.
    pub min_main: f32,
    /// Max main size constraint.
    pub max_main: f32,
    /// Margins (used for main-axis outer sizing and positioning). All in CSS px.
    pub margin_left: f32,
    pub margin_right: f32,
    pub margin_top: f32,
    pub margin_bottom: f32,
    /// Whether the corresponding main-axis margins are `auto`.
    /// For row direction in horizontal writing modes, these map to left/right.
    pub margin_left_auto: bool,
    pub margin_right_auto: bool,
    /// Main-axis padding + border (content-box to border-box adjustment).
    /// For row flex, this is horizontal padding + border.
    /// For column flex, this is vertical padding + border.
    pub main_padding_border: f32,
}

/// Resulting per-item main-axis size and offset.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct FlexPlacement {
    pub handle: ItemRef,
    pub main_size: f32,
    pub main_offset: f32,
}

/// Container inputs needed for single-line layout.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct FlexContainerInputs {
    pub direction: FlexDirection,
    pub writing_mode: WritingMode,
    /// Definite main-size of the content box in px.
    pub container_main_size: f32,
    /// Main-axis gap between adjacent items (CSS gap), in px.
    pub main_gap: f32,
}

/// Minimal justify-content values we support initially.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum JustifyContent {
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// Minimal align-items values we support initially for cross-axis behavior (stub).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum AlignItems {
    Stretch,
    Center,
    FlexStart,
    FlexEnd,
    /// First baseline alignment.
    Baseline,
    /// Last baseline alignment.
    LastBaseline,
}

/// Minimal align-content values for cross-axis multi-line packing.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum AlignContent {
    Start,
    Center,
    End,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    Stretch,
}

/// Cross-axis placement result when aligning a single line.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct CrossPlacement {
    /// The resolved cross-size after alignment and clamping.
    pub cross_size: f32,
    /// The cross-axis offset from cross-start.
    pub cross_offset: f32,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct CrossContext {
    pub align_items: AlignItems,
    pub align_content: AlignContent,
    pub container_cross_size: f32,
    /// CSS cross-axis gap between adjacent flex lines (row-gap for row direction; column-gap for column direction).
    pub cross_gap: f32,
}

/// Compute hypothetical sizes, gaps, and apply flex grow/shrink.
fn plan_hypotheticals_and_flex(
    container: FlexContainerInputs,
    items: &[FlexChild],
) -> (Vec<f32>, f32) {
    // Inner (content-box) sizes from flex_basis, used for shrink weight calculation
    let mut sizes: Vec<f32> = items
        .iter()
        .map(|child| clamp(child.flex_basis, child.min_main, child.max_main))
        .collect();
    // Gaps should only be between non-zero-sized items (excludes collapsed whitespace)
    let non_zero_count = sizes.iter().filter(|&&s| s > 0.0).count();
    let gaps_total = if non_zero_count > 1 {
        (non_zero_count as f32 - 1.0) * container.main_gap.max(0.0)
    } else {
        0.0
    };

    // Calculate free space using OUTER (border-box) sizes
    // Per CSS Flexbox spec 9.7: "the sum of the outer hypothetical main sizes"
    // Outer size = inner size + padding + border
    let sum_outer: f32 = sizes
        .iter()
        .zip(items.iter())
        .map(|(inner_size, child)| inner_size + child.main_padding_border)
        .sum();
    let free_space = container.container_main_size - sum_outer - gaps_total;

    debug!(
        "[FLEX-JUSTIFY] items={} sum_inner={:.3} sum_outer={:.3} gaps_total={:.3} container_main={:.3} free_space={:.3}",
        items.len(),
        sizes.iter().copied().sum::<f32>(),
        sum_outer,
        gaps_total,
        container.container_main_size,
        free_space
    );

    if free_space > 0.0 {
        distribute_grow(free_space, items, &mut sizes);
    } else if free_space < 0.0 {
        distribute_shrink(free_space, items, &mut sizes);
    }
    (sizes, gaps_total)
}

/// Compute single-line main-axis sizes and offsets for items.
///
/// Behavior:
/// - Computes hypothetical main sizes from `flex_basis` clamped by min/max.
/// - Distributes free space using grow when positive, shrink when negative.
/// - Produces main offsets honoring direction (normal vs reverse).
/// - Places items according to `justify_content` along the main axis (start/center/end).
pub fn layout_single_line(
    container: FlexContainerInputs,
    justify_content: JustifyContent,
    items: &[FlexChild],
) -> Vec<FlexPlacement> {
    let axes = resolve_axes(container.direction, container.writing_mode);
    // 1–3) Hypotheticals, gaps, free space, and flex distribution
    let (hypothetical_sizes, gaps_total) = plan_hypotheticals_and_flex(container, items);

    // 3.5) Resolve auto margins and outer sizes (extracted helper)
    let (outer_sizes, effective_left_margins, auto_slots, sum_outer) =
        resolve_auto_margins_and_outer(
            items,
            &hypothetical_sizes,
            container.container_main_size,
            gaps_total,
        );
    // 4) Main offsets before justification (packed at start of flow direction)
    let total: f32 = sum_outer;
    let effective_justify = if auto_slots > 0 {
        JustifyContent::Start
    } else {
        justify_content
    };

    // For justify-content spacing calculations, only count items with non-zero size
    // (whitespace text nodes can have zero size but still be flex items)
    let non_zero_item_count = hypothetical_sizes
        .iter()
        .filter(|&&size| size > 0.0)
        .count();

    let (start_offset, between_spacing) = justify_params(
        effective_justify,
        container.container_main_size,
        total + gaps_total,
        non_zero_item_count,
    );

    debug!(
        "[FLEX-JUSTIFY] justify={:?} start_offset={:.3} between_spacing={:.3} total_including_gaps={:.3} sum_outer={:.3}",
        effective_justify,
        start_offset,
        between_spacing,
        total + gaps_total,
        total
    );

    // 5) Direction (reverse flips order and offset accumulation)
    let plan = MainOffsetPlan {
        reverse: axes.main_reverse,
        container_main_size: container.container_main_size,
        start_offset,
        between_spacing,
        main_gap: container.main_gap.max(0.0),
    };
    // Accumulate starting positions of each item's outer box.
    let mut outer_offsets: Vec<f32> = accumulate_main_offsets(&plan, &outer_sizes);
    clamp_first_offset_if_needed(effective_justify, axes.main_reverse, &mut outer_offsets);
    // 6) Build placements preserving input order.
    build_main_placements(
        items,
        &hypothetical_sizes,
        &outer_offsets,
        &effective_left_margins,
    )
}

/// Compute single-line main-axis placements and cross-axis placements together.
///
/// Returns a vector matching input order where each element is `(FlexPlacement, CrossPlacement)`.
/// `cross_inputs` must be the same length as `items` and contain `(item_cross, min_cross, max_cross)`.
pub fn layout_single_line_with_cross(
    container: FlexContainerInputs,
    justify_content: JustifyContent,
    cross_ctx: CrossContext,
    items: &[FlexChild],
    cab: CrossAndBaseline<'_>,
) -> Vec<(FlexPlacement, CrossPlacement)> {
    debug_assert_eq!(
        items.len(),
        cab.cross_inputs.len(),
        "items and cross_inputs length mismatch"
    );
    debug_assert_eq!(
        items.len(),
        cab.baseline_inputs.len(),
        "items and baseline_inputs length mismatch"
    );
    let main = layout_single_line(container, justify_content, items);
    let cross = align_cross_for_items(
        cross_ctx.align_items,
        cross_ctx.container_cross_size,
        cab.cross_inputs,
    );
    let mut pairs: Vec<(FlexPlacement, CrossPlacement)> = main
        .into_iter()
        .zip(cross)
        .enumerate()
        .map(|(idx, (main_place, mut cross_place))| {
            // Add margin-top to position the item within the container
            // For row direction: margin-top pushes item down from container start
            // For column direction: this would be margin-left (handled by main-axis)
            if let Some(item) = items.get(idx) {
                cross_place.cross_offset += item.margin_top;
            }
            (main_place, cross_place)
        })
        .collect();
    // Adjust cross offsets for baseline alignment if needed (single-line)
    if matches!(
        cross_ctx.align_items,
        AlignItems::Baseline | AlignItems::LastBaseline
    ) {
        let line_cross_max = cross_ctx.container_cross_size;
        let line_ref =
            compute_line_baseline_ref(cross_ctx.align_items, cab.baseline_inputs, cab.cross_inputs);
        for (pair, baseline_opt) in pairs.iter_mut().zip(cab.baseline_inputs.iter()) {
            if let Some((first, last)) = *baseline_opt {
                let item_baseline = match cross_ctx.align_items {
                    AlignItems::Baseline => first,
                    AlignItems::LastBaseline => last,
                    _ => 0.0,
                };
                let desired = (line_ref - item_baseline).max(0.0);
                let max_offset = (line_cross_max - pair.1.cross_size).max(0.0);
                pair.1.cross_offset = desired.min(max_offset);
            }
        }
    }
    pairs
}

/// Multi-line flex layout (wrap) — per-line main layout + cross-axis line packing.
///
/// Breaks items into lines by container main-size and CSS gap, then runs the single-line main
/// algorithm per line. Cross-axis per line uses the maximum clamped cross-size of items in that
/// line and packs lines according to `align-content` within `container_cross_size`.
pub fn layout_multi_line_with_cross(
    container: FlexContainerInputs,
    justify_content: JustifyContent,
    cross_ctx: CrossContext,
    items: &[FlexChild],
    cab: CrossAndBaseline<'_>,
) -> Vec<(FlexPlacement, CrossPlacement)> {
    debug_assert_eq!(
        items.len(),
        cab.cross_inputs.len(),
        "items and cross_inputs length mismatch",
    );
    debug_assert_eq!(
        items.len(),
        cab.baseline_inputs.len(),
        "items and baseline_inputs length mismatch"
    );
    debug!(
        "[MULTI-LINE] Starting: items={} container_main={:.1} container_cross={:.1}",
        items.len(),
        container.container_main_size,
        cross_ctx.container_cross_size
    );
    // Break into lines using outer sizes (margin-aware)
    let line_ranges = break_into_lines(container.container_main_size, container.main_gap, items);
    debug!(
        "[MULTI-LINE] Line breaking: {} lines -> {:?}",
        line_ranges.len(),
        line_ranges
    );

    // Per-line main and cross extents
    let (per_line_main, per_line_cross_max) = per_line_main_and_cross(
        container,
        justify_content,
        items,
        cab.cross_inputs,
        &line_ranges,
    );

    // Build final results using align-content packing
    let stretched = stretch_line_crosses(&cross_ctx, &per_line_cross_max);
    let inputs = PackInputs {
        cross_inputs: cab.cross_inputs,
        baseline_inputs: cab.baseline_inputs,
        line_ranges: &line_ranges,
        per_line_main: &per_line_main,
        line_cross_vec: &stretched,
        items,
    };
    pack_lines_and_build(&cross_ctx, &inputs)
}

#[cfg(test)]
mod tests;
