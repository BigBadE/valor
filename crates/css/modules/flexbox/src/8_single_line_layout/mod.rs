use crate::{FlexDirection, ItemRef, WritingMode, resolve_axes};
use log::debug;

/// Optional baseline metrics per item: `(first_baseline, last_baseline)`.
type BaselineMetrics = Option<(f32, f32)>;
/// Slice alias for baseline metrics across items.
type BaselineSlice<'slice> = &'slice [BaselineMetrics];

/// Read-only inputs to pack lines along the cross axis.
#[derive(Copy, Clone)]
struct PackInputs<'inputs> {
    /// Per-item cross inputs `(cross_size, min_cross, max_cross)`
    cross_inputs: &'inputs [(f32, f32, f32)],
    /// Per-item baseline metrics if available
    baseline_inputs: BaselineSlice<'inputs>,
    /// Ranges of items per line
    line_ranges: &'inputs [LineRange],
    /// Main-axis placements per line
    per_line_main: &'inputs [PerLineMain],
    /// Resolved cross-size per line (after stretch)
    line_cross_vec: &'inputs [f32],
}

#[inline]
/// Compute the reference baseline value for a line from per-item baseline metrics and cross sizes.
fn compute_line_baseline_ref(
    align: AlignItems,
    baselines: BaselineSlice<'_>,
    cross_inputs: &[(f32, f32, f32)],
) -> f32 {
    let mut reference = 0.0f32;
    for (metrics_opt, &(item_cross, _min_c, _max_c)) in baselines.iter().zip(cross_inputs.iter()) {
        if let Some((first, last)) = *metrics_opt {
            let candidate = match align {
                AlignItems::Baseline => first,
                AlignItems::LastBaseline => last,
                _ => 0.0,
            };
            // Clamp within the item cross-size just in case metrics exceed bounds.
            let clamped = candidate.max(0.0).min(item_cross);
            if clamped > reference {
                reference = clamped;
            }
        }
    }
    reference
}

/// Context for adjusting cross offsets to align baselines within a line.
struct BaselineAdjustCtx<'baseline> {
    /// The effective `align-items` mode for baseline behavior.
    align: AlignItems,
    /// Per-item baseline metrics for the current line.
    baselines: BaselineSlice<'baseline>,
    /// Index of the item within the current line.
    index_in_line: usize,
    /// The target baseline reference for the line.
    line_ref: f32,
    /// The cross-size of the current line.
    line_cross_max: f32,
    /// The accumulated cross offset of previous lines.
    cross_accum_offset: f32,
}

#[inline]
/// Adjust the `cross_placement` so that the item's chosen baseline matches the line reference.
fn adjust_cross_for_baseline(ctx: &BaselineAdjustCtx<'_>, cross_placement: &mut CrossPlacement) {
    if matches!(ctx.align, AlignItems::Baseline | AlignItems::LastBaseline)
        && let Some((first, last)) = ctx
            .baselines
            .get(ctx.index_in_line)
            .and_then(|metrics| *metrics)
    {
        let item_baseline = match ctx.align {
            AlignItems::Baseline => first,
            AlignItems::LastBaseline => last,
            _ => 0.0,
        };
        let desired_in_line = (ctx.line_ref - item_baseline).max(0.0);
        let max_in_line = (ctx.line_cross_max - cross_placement.cross_size).max(0.0);
        cross_placement.cross_offset = ctx.cross_accum_offset + desired_in_line.min(max_in_line);
    }
}

#[inline]
/// Quantize a CSS pixel value to the layout unit (1/64 px) to match Chromium's subpixel model.
fn quantize_layout(value: f32) -> f32 {
    (value * 64.0).round() / 64.0
}

/// Bundle of cross and baseline inputs required by the combined layout APIs.
#[derive(Copy, Clone)]
pub struct CrossAndBaseline<'cb> {
    /// Per-item cross inputs `(cross_size, min_cross, max_cross)`
    pub cross_inputs: &'cb [(f32, f32, f32)],
    /// Per-item baseline metrics if available
    pub baseline_inputs: BaselineSlice<'cb>,
}

#[inline]
/// Compute hypothetical sizes, gaps, and apply flex grow/shrink.
fn plan_hypotheticals_and_flex(
    container: FlexContainerInputs,
    items: &[FlexChild],
) -> (Vec<f32>, f32) {
    let mut sizes: Vec<f32> = items
        .iter()
        .map(|child| clamp(child.flex_basis, child.min_main, child.max_main))
        .collect();
    let gaps_total = if items.len() > 1 {
        (items.len() as f32 - 1.0) * container.main_gap.max(0.0)
    } else {
        0.0
    };
    let sum: f32 = sizes.iter().copied().sum();
    let free_space = container.container_main_size - sum - gaps_total;
    debug!(
        target: "css::flexbox::single_line",
        "[FLEX-JUSTIFY] items={} sum_sizes={:.3} gaps_total={:.3} container_main={:.3} free_space={:.3}",
        items.len(),
        sum,
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

#[inline]
/// Quantize a CSS pixel value downward to the layout unit (1/64 px). Used for between-spacing to
/// match Chromium's fixed-point accumulation and avoid accumulating rounding overflow across slots.
fn quantize_layout_floor(value: f32) -> f32 {
    ((value * 64.0).floor()) / 64.0
}

#[inline]
/// Compute justify-content start offset and between-spacing (excluding CSS gap).
fn justify_params(
    justify: JustifyContent,
    container_main: f32,
    content_total: f32,
    item_count: usize,
) -> (f32, f32) {
    let remaining = (container_main - content_total).max(0.0);
    let (start, between) = match (justify, item_count) {
        (JustifyContent::End, _) => (remaining, 0.0),
        (JustifyContent::Center, _) => (remaining * 0.5, 0.0),
        (JustifyContent::SpaceBetween, count) if count > 1 => {
            (0.0, remaining / (count as f32 - 1.0))
        }
        (JustifyContent::SpaceAround, count) if count > 0 => {
            (remaining / (count as f32 * 2.0), remaining / (count as f32))
        }
        (JustifyContent::SpaceEvenly, count) if count > 0 => {
            let slots = count as f32 + 1.0;
            (remaining / slots, remaining / slots)
        }
        // Start and all other cases
        _ => (0.0, 0.0),
    };
    (quantize_layout(start), quantize_layout_floor(between))
}

#[inline]
/// Build main-axis placements from inner sizes and outer starts (margin-aware starts) with
/// the resolved effective left margins (includes auto margin absorption).
fn build_main_placements(
    items: &[FlexChild],
    inner_sizes: &[f32],
    outer_starts: &[f32],
    effective_left_margins: &[f32],
) -> Vec<FlexPlacement> {
    items
        .iter()
        .zip(inner_sizes.iter())
        .zip(outer_starts.iter())
        .zip(effective_left_margins.iter())
        .map(|(((child, size), outer_start), eff_left)| FlexPlacement {
            handle: child.handle,
            main_size: *size,
            main_offset: *outer_start + *eff_left,
        })
        .collect()
}

// removed: outer_sizes_and_sum (was unused after auto margin integration)

#[inline]
/// Ensure the first item's offset aligns to main-start for Start and `SpaceBetween`
/// when the main axis is not reversed. This guards against any accidental
/// pre-gap/start offset leaks. No effect for other justify modes or reverse axes.
fn clamp_first_offset_if_needed(
    justify_content: JustifyContent,
    main_reverse: bool,
    offsets: &mut [f32],
) {
    if !main_reverse
        && matches!(
            justify_content,
            JustifyContent::Start | JustifyContent::SpaceBetween
        )
        && let Some(first) = offsets.first_mut()
        && *first != 0.0
    {
        debug!(
            target: "css::flexbox::single_line",
            "[FLEX-JUSTIFY] clamping first offset from {:.3} to 0.000 for {:?}",
            *first,
            justify_content
        );
        *first = 0.0;
    }
}

/// Compute cross-axis placement for multiple items using `align-items`.
/// Each tuple is `(item_cross_size, min_cross, max_cross)`.
#[inline]
pub fn align_cross_for_items(
    align: AlignItems,
    container_cross_size: f32,
    items: &[(f32, f32, f32)],
) -> Vec<CrossPlacement> {
    items
        .iter()
        .map(|&(item_size, min_c, max_c)| {
            align_single_line_cross(align, container_cross_size, item_size, min_c, max_c)
        })
        .collect()
}

/// Compute single-line main-axis placements and cross-axis placements together.
///
/// Returns a vector matching input order where each element is `(FlexPlacement, CrossPlacement)`.
/// `cross_inputs` must be the same length as `items` and contain `(item_cross, min_cross, max_cross)`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct CrossContext {
    pub align_items: AlignItems,
    pub align_content: AlignContent,
    pub container_cross_size: f32,
    /// CSS cross-axis gap between adjacent flex lines (row-gap for row direction; column-gap for column direction).
    pub cross_gap: f32,
}

#[inline]
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
    let mut pairs: Vec<(FlexPlacement, CrossPlacement)> = main.into_iter().zip(cross).collect();
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

#[inline]
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
    // Break into lines using outer sizes (margin-aware)
    let line_ranges = break_into_lines(container.container_main_size, container.main_gap, items);

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
    };
    pack_lines_and_build(&cross_ctx, inputs)
}

/// Compute align-content start offset and between-spacing (excluding CSS gap) for lines.
///
/// Modes:
/// - Start/End/Center: pack lines against start/end or center them in remaining space.
/// - SpaceBetween/Around/Evenly: distribute remaining space between line boxes.
/// - Stretch: treated as Start in this MVP (line box stretching not implemented here).
fn align_content_params(
    align: AlignContent,
    container_cross: f32,
    content_total: f32,
    line_count: usize,
) -> (f32, f32) {
    let remaining = (container_cross - content_total).max(0.0);
    let (start, between) = match (align, line_count) {
        (AlignContent::End, _) => (remaining, 0.0),
        (AlignContent::Center, _) => (remaining * 0.5, 0.0),
        (AlignContent::SpaceBetween, count) if count > 1 => (0.0, remaining / (count as f32 - 1.0)),
        (AlignContent::SpaceAround, count) if count > 0 => {
            (remaining / (count as f32 * 2.0), remaining / (count as f32))
        }
        (AlignContent::SpaceEvenly, count) if count > 0 => {
            let slots = count as f32 + 1.0;
            (remaining / slots, remaining / slots)
        }
        // Start and Stretch (stretch handled by line-size expansion)
        _ => (0.0, 0.0),
    };
    (quantize_layout(start), quantize_layout(between))
}

/// Line start/end indices for items included in the line: `[start, end)`.
type LineRange = (usize, usize);

#[inline]
/// Break items into lines by accumulating hypothetical sizes and `main_gap` until exceeding
/// `container_main_size`. Returns a list of `[start, end)` ranges.
fn break_into_lines(
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

/// Per line, compute single-line main-axis placements and the maximum clamped cross-size.
/// Returns `(per_line_main, per_line_cross_max)` where `per_line_main[i]` pairs with
/// `per_line_cross_max[i]`.
#[inline]
fn per_line_main_and_cross(
    container: FlexContainerInputs,
    justify_content: JustifyContent,
    items: &[FlexChild],
    cross_inputs: &[(f32, f32, f32)],
    line_ranges: &[LineRange],
) -> (PerLineMainVec, Vec<f32>) {
    let mut per_line_main: PerLineMainVec = Vec::with_capacity(line_ranges.len());
    let mut per_line_cross_max: Vec<f32> = Vec::with_capacity(line_ranges.len());
    for (line_start, line_end) in line_ranges.iter().copied() {
        let (Some(line_items), Some(line_cross_inputs)) = (
            items.get(line_start..line_end),
            cross_inputs.get(line_start..line_end),
        ) else {
            return (Vec::new(), Vec::new());
        };
        let line_main = layout_single_line(container, justify_content, line_items);
        let mut line_cross_max = 0.0f32;
        for &(item_cross, min_c, max_c) in line_cross_inputs {
            let clamped = clamp(item_cross, min_c, max_c);
            if clamped > line_cross_max {
                line_cross_max = clamped;
            }
        }
        per_line_main.push(line_main);
        per_line_cross_max.push(line_cross_max);
    }
    (per_line_main, per_line_cross_max)
}
/// according to `align_content`, and aligning items within each line according to `align_items`.
#[inline]
fn stretch_line_crosses(cross_ctx: &CrossContext, per_line_cross_max: &[f32]) -> Vec<f32> {
    let mut line_cross_vec: Vec<f32> = per_line_cross_max.to_vec();
    let lines_total_cross: f32 = line_cross_vec.iter().copied().sum();
    let line_count = line_cross_vec.len();
    debug!(
        target: "css::flexbox::multi_line",
        "[ALIGN-CONTENT] mode={:?} container_cross={:.3} lines_total={:.3} line_count={}",
        cross_ctx.align_content,
        cross_ctx.container_cross_size,
        lines_total_cross,
        line_count
    );
    if matches!(cross_ctx.align_content, AlignContent::Stretch) && line_count > 0 {
        let gaps_total = if line_count > 1 {
            (line_count as f32 - 1.0) * cross_ctx.cross_gap.max(0.0)
        } else {
            0.0
        };
        let remaining = (cross_ctx.container_cross_size - lines_total_cross - gaps_total).max(0.0);
        let add_each = remaining / line_count as f32;
        debug!(
            target: "css::flexbox::multi_line",
            "[ALIGN-CONTENT] stretch: remaining={remaining:.3} add_each={add_each:.3}"
        );
        for value in &mut line_cross_vec {
            *value += add_each;
        }
    }
    line_cross_vec
}

/// Pack stretched line boxes along the cross axis according to `align-content`, adding cross-axis
/// gaps between adjacent lines, and build the final `(FlexPlacement, CrossPlacement)` pairs.
fn pack_lines_and_build(
    cross_ctx: &CrossContext,
    inputs: PackInputs<'_>,
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
            AlignItems::Baseline | AlignItems::LastBaseline
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

/// Per-line context for building `(FlexPlacement, CrossPlacement)` pairs.
struct LineBuildCtx<'ctx> {
    /// Cross-axis and alignment parameters.
    cross_ctx: &'ctx CrossContext,
    /// Main placements for items in the line.
    line_main: &'ctx PerLineMain,
    /// Cross inputs for items in the line.
    line_cross_inputs: &'ctx [(f32, f32, f32)],
    /// Baseline metrics for items in the line.
    line_baselines: BaselineSlice<'ctx>,
    /// Max cross-size for the line.
    line_cross_max: f32,
    /// Reference baseline value for the line.
    line_ref: f32,
    /// Accumulated cross offset before this line.
    cross_accum_offset: f32,
}

#[inline]
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

/// Inputs for a flex item needed for single-line main-axis sizing.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct FlexChild {
    pub handle: ItemRef,
    /// Flex base size (used as hypothetical main size before flexing), in CSS px.
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
    /// First baseline alignment [Approximation]: treated as `FlexStart` until baseline metrics are available.
    Baseline,
    /// Last baseline alignment [Approximation]: treated as `FlexEnd` until baseline metrics are available.
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

/// Convenience alias for per-line main placements vector.
type PerLineMain = Vec<FlexPlacement>;
/// Convenience alias for the list of all per-line main placement vectors.
type PerLineMainVec = Vec<PerLineMain>;

/// Compute single-line main-axis sizes and offsets for items.
///
/// Behavior:
/// - Computes hypothetical main sizes from `flex_basis` clamped by min/max.
/// - Distributes free space using grow when positive, shrink when negative.
/// - Produces main offsets honoring direction (normal vs reverse).
/// - Places items according to `justify_content` along the main axis (start/center/end).
#[inline]
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
    let (start_offset, between_spacing) = justify_params(
        effective_justify,
        container.container_main_size,
        total + gaps_total,
        items.len(),
    );
    debug!(
        target: "css::flexbox::single_line",
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

/// Resolve auto margins (§9.4) by distributing remaining space equally across all auto margin slots,
/// then compute outer sizes and sum. Returns `(outer_sizes, effective_left_margins, auto_slots, sum_outer)`.
type OuterCalc = (Vec<f32>, Vec<f32>, usize, f32);

/// Distribute remaining main-axis free space across any auto margins, then produce effective
/// left margins and outer sizes for each item. Returns `(outer_sizes, effective_left_margins, auto_slots, sum_outer)`.
#[inline]
fn resolve_auto_margins_and_outer(
    items: &[FlexChild],
    inner_sizes: &[f32],
    container_main_size: f32,
    gaps_total: f32,
) -> OuterCalc {
    // Count auto slots and sum non-auto margins
    let mut non_auto_margins_sum = 0.0f32;
    let auto_slots = items.iter().fold(0usize, |acc, child| {
        let left_slot = usize::from(child.margin_left_auto);
        let right_slot = usize::from(child.margin_right_auto);
        non_auto_margins_sum += if child.margin_left_auto {
            0.0
        } else {
            child.margin_left.max(0.0)
        };
        non_auto_margins_sum += if child.margin_right_auto {
            0.0
        } else {
            child.margin_right.max(0.0)
        };
        acc.saturating_add(left_slot).saturating_add(right_slot)
    });
    let sum_inner_after_flex: f32 = inner_sizes.iter().copied().sum();
    let remaining_for_margins_raw =
        container_main_size - sum_inner_after_flex - non_auto_margins_sum - gaps_total;
    let remaining_for_margins_pos = remaining_for_margins_raw.max(0.0);
    let auto_each = if auto_slots > 0 {
        quantize_layout_floor(remaining_for_margins_pos / auto_slots as f32)
    } else {
        0.0
    };
    let mut outer_sizes: Vec<f32> = Vec::with_capacity(items.len());
    let mut effective_left_margins: Vec<f32> = Vec::with_capacity(items.len());
    for (child, inner) in items.iter().zip(inner_sizes.iter().copied()) {
        let eff_left = child.margin_left.max(0.0)
            + if child.margin_left_auto {
                auto_each
            } else {
                0.0
            };
        let eff_right = child.margin_right.max(0.0)
            + if child.margin_right_auto {
                auto_each
            } else {
                0.0
            };
        effective_left_margins.push(eff_left);
        outer_sizes.push(inner + eff_left + eff_right);
    }
    let sum_outer: f32 = outer_sizes.iter().copied().sum();
    (outer_sizes, effective_left_margins, auto_slots, sum_outer)
}

// duplicate helper definitions removed; see top-of-file helpers for implementations

#[derive(Copy, Clone, Debug)]
/// Parameters for planning main-axis offset accumulation.
struct MainOffsetPlan {
    /// Whether the main axis runs in reverse order.
    reverse: bool,
    /// The container's definite main size in px.
    container_main_size: f32,
    /// Pre-placement offset from main-start.
    start_offset: f32,
    /// Extra spacing between items from justify-content (excludes CSS gap).
    between_spacing: f32,
    /// CSS main-axis gap between adjacent items in px.
    main_gap: f32,
}

#[inline]
/// Compute per-item main-axis offsets from a sizing plan and item sizes.
fn accumulate_main_offsets(plan: &MainOffsetPlan, sizes: &[f32]) -> Vec<f32> {
    if plan.reverse {
        // Accumulate from the end in logical order so that earlier logical items
        // appear at larger main-axis coordinates.
        let mut cursor = quantize_layout(plan.container_main_size - plan.start_offset);
        let mut offsets_accum = Vec::with_capacity(sizes.len());
        let mut iter = sizes.iter().peekable();
        while let Some(size_ref) = iter.next() {
            let size_val = *size_ref;
            cursor = quantize_layout(cursor - size_val);
            offsets_accum.push(cursor);
            if iter.peek().is_some() {
                cursor = quantize_layout(cursor - (plan.main_gap + plan.between_spacing));
            }
        }
        offsets_accum
    } else {
        let mut cursor = quantize_layout(plan.start_offset);
        let mut offsets_accum = Vec::with_capacity(sizes.len());
        let mut iter = sizes.iter().peekable();
        while let Some(size_ref) = iter.next() {
            offsets_accum.push(cursor);
            cursor = quantize_layout(cursor + *size_ref);
            if iter.peek().is_some() {
                cursor = quantize_layout(cursor + (plan.main_gap + plan.between_spacing));
            }
        }
        offsets_accum
    }
}

#[inline]
/// Clamp a value between min and max inclusive.
const fn clamp(value: f32, min_v: f32, max_v: f32) -> f32 {
    value.max(min_v).min(max_v)
}

/// Distribute positive free space to items using flex-grow factors.
fn distribute_grow(free_space: f32, items: &[FlexChild], sizes: &mut [f32]) {
    debug_assert!(free_space >= 0.0, "grow called with negative free space");
    let mut remaining = free_space;
    let mut saturated = vec![false; items.len()];
    // Iterate to handle saturation at max constraints
    for _ in 0..items.len() {
        let mut sum_grow = 0.0f32;
        for (child, is_saturated) in items.iter().zip(saturated.iter()) {
            if !*is_saturated {
                sum_grow += child.flex_grow.max(0.0);
            }
        }
        if sum_grow <= 0.0 || remaining <= 0.0 {
            break;
        }
        let unit = remaining / sum_grow;
        let mut any_saturated = false;
        let mut applied_total = 0.0f32;
        for ((size_ref, child), sat_ref) in sizes.iter_mut().zip(items).zip(saturated.iter_mut()) {
            if *sat_ref {
                continue;
            }
            let delta = child.flex_grow.max(0.0) * unit;
            let grown = *size_ref + delta;
            let clamped = clamp(grown, child.min_main, child.max_main);
            let applied = clamped - *size_ref;
            *size_ref = clamped;
            applied_total += applied;
            if (clamped - child.max_main).abs() < f32::EPSILON {
                *sat_ref = true;
                any_saturated = true;
            }
        }
        remaining -= applied_total;
        if !any_saturated {
            break;
        }
    }
}

/// Distribute negative free space to items using weighted flex-shrink factors.
fn distribute_shrink(free_space: f32, items: &[FlexChild], sizes: &mut [f32]) {
    debug_assert!(free_space <= 0.0, "shrink called with positive free space");
    // Weighted shrink based on current size and shrink factor, with min saturation
    let mut remaining = -free_space; // positive amount to remove
    let mut frozen = vec![false; items.len()];
    for _ in 0..items.len() {
        let mut sum_weight = 0.0f32;
        for ((size_ref, child), is_frozen) in sizes.iter().zip(items).zip(frozen.iter()) {
            if *is_frozen {
                continue;
            }
            let basis = (*size_ref).max(0.0);
            sum_weight += basis * child.flex_shrink.max(0.0);
        }
        if sum_weight <= 0.0 || remaining <= 0.0 {
            break;
        }
        let mut any_froze = false;
        let mut applied_total = 0.0f32;
        for ((size_ref, child), frozen_ref) in sizes.iter_mut().zip(items).zip(frozen.iter_mut()) {
            if *frozen_ref {
                continue;
            }
            let basis = (*size_ref).max(0.0);
            let weight = basis * child.flex_shrink.max(0.0);
            let delta = remaining * (weight / sum_weight);
            let shrunk = (*size_ref - delta).max(0.0);
            let clamped = clamp(shrunk, child.min_main, child.max_main);
            let applied = *size_ref - clamped;
            *size_ref = clamped;
            applied_total += applied;
            if (clamped - child.min_main).abs() < f32::EPSILON {
                *frozen_ref = true;
                any_froze = true;
            }
        }
        remaining -= applied_total;
        if !any_froze {
            break;
        }
    }
}

/// Cross-axis placement result when aligning a single line.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct CrossPlacement {
    /// The resolved cross-size after alignment and clamping.
    pub cross_size: f32,
    /// The cross-axis offset from cross-start.
    pub cross_offset: f32,
}

/// Compute cross-axis placement and size given `align-items` for single-line containers.
///
/// Behavior:
/// - Stretch: cross-size becomes container cross-size clamped by min/max; offset 0.
/// - Center: cross-size remains the item cross-size (clamped) and offset centers it in the container.
#[inline]
pub fn align_single_line_cross(
    align: AlignItems,
    container_cross_size: f32,
    item_cross_size: f32,
    min_cross: f32,
    max_cross: f32,
) -> CrossPlacement {
    let clamped_item = clamp(item_cross_size, min_cross, max_cross);
    match align {
        AlignItems::Stretch => {
            // Stretch applies when the item cross-size is auto/unspecified.
            // Heuristic: treat non-positive sizes as auto for MVP.
            if item_cross_size <= 0.0 {
                CrossPlacement {
                    cross_size: clamp(container_cross_size, min_cross, max_cross),
                    cross_offset: 0.0,
                }
            } else {
                CrossPlacement {
                    cross_size: clamped_item,
                    cross_offset: 0.0,
                }
            }
        }
        AlignItems::Center => {
            let size = clamped_item;
            let offset = ((container_cross_size - size) * 0.5).max(0.0);
            CrossPlacement {
                cross_size: size,
                cross_offset: offset,
            }
        }
        AlignItems::FlexStart | AlignItems::Baseline => CrossPlacement {
            cross_size: clamped_item,
            cross_offset: 0.0,
        },
        AlignItems::FlexEnd | AlignItems::LastBaseline => {
            let size = clamped_item;
            let offset = (container_cross_size - size).max(0.0);
            CrossPlacement {
                cross_size: size,
                cross_offset: offset,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[inline]
    fn item_zero_margins(handle: u64, basis: f32) -> FlexChild {
        FlexChild {
            handle: ItemRef(handle),
            flex_basis: basis,
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
        }
    }

    #[inline]
    fn three_items_50() -> Vec<FlexChild> {
        vec![
            item_zero_margins(1, 50.0),
            item_zero_margins(2, 50.0),
            item_zero_margins(3, 50.0),
        ]
    }

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
        let min_offset = out
            .iter()
            .map(|placement| placement.main_offset)
            .fold(f32::INFINITY, f32::min);
        assert!(min_offset >= 0.0, "centered layout must not start before 0");
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

    #[test]
    /// # Panics
    /// Panics if center alignment does not center the item within the container cross-size.
    fn align_items_center_cross_axis() {
        let placement = align_single_line_cross(AlignItems::Center, 200.0, 100.0, 0.0, 1e9);
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
        // When item cross-size is auto/unspecified (here modeled as 0), Stretch expands to container size.
        let placement = align_single_line_cross(AlignItems::Stretch, 120.0, 0.0, 0.0, 1e9);
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
        let items: Vec<(f32, f32, f32)> = vec![
            (10.0, 0.0, 1000.0),
            (20.0, 0.0, 1000.0),
            (30.0, 0.0, 1000.0),
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
            },
        ];
        let cross_inputs = vec![(20.0, 0.0, 100.0), (20.0, 0.0, 100.0)];
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
}
