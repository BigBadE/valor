#[inline]
/// Compute justify-content start offset and between-spacing (excluding CSS gap).
fn justify_params(
    justify: JustifyContent,
    container_main: f32,
    content_total: f32,
    item_count: usize,
) -> (f32, f32) {
    let remaining = (container_main - content_total).max(0.0);
    match (justify, item_count) {
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
    }
}

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
    pub container_cross_size: f32,
}

#[inline]
pub fn layout_single_line_with_cross(
    container: FlexContainerInputs,
    justify_content: JustifyContent,
    cross_ctx: CrossContext,
    items: &[FlexChild],
    cross_inputs: &[(f32, f32, f32)],
) -> Vec<(FlexPlacement, CrossPlacement)> {
    debug_assert_eq!(
        items.len(),
        cross_inputs.len(),
        "items and cross_inputs length mismatch"
    );
    let main = layout_single_line(container, justify_content, items);
    let cross = align_cross_for_items(
        cross_ctx.align_items,
        cross_ctx.container_cross_size,
        cross_inputs,
    );
    main.into_iter().zip(cross).collect()
}

// Single-line flex layout (no-wrap) — main-axis sizing & placement
// Spec: <https://www.w3.org/TR/css-flexbox-1/#layout-algorithm>
// This module implements a minimal subset of the main-axis algorithm for single-line flex containers.

use crate::{FlexDirection, ItemRef, WritingMode, resolve_axes};
use log::{debug, error};

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
}

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

    // 1) Hypothetical sizes (clamped flex-basis)
    let mut hypothetical_sizes: Vec<f32> = items
        .iter()
        .map(|child| clamp(child.flex_basis, child.min_main, child.max_main))
        .collect();

    // 2) Free space (account for gaps)
    let sum_hypotheticals: f32 = hypothetical_sizes.iter().copied().sum();
    let gaps_total = if items.len() > 1 {
        (items.len() as f32 - 1.0) * container.main_gap.max(0.0)
    } else {
        0.0
    };
    let free_space = container.container_main_size - sum_hypotheticals - gaps_total;
    debug!(
        target: "css::flexbox::single_line",
        "[FLEX-JUSTIFY] items={} sum_sizes={:.3} gaps_total={:.3} container_main={:.3} free_space={:.3}",
        items.len(),
        sum_hypotheticals,
        gaps_total,
        container.container_main_size,
        free_space
    );

    // 3) Flexing
    if free_space > 0.0 {
        distribute_grow(free_space, items, &mut hypothetical_sizes);
    } else if free_space < 0.0 {
        distribute_shrink(free_space, items, &mut hypothetical_sizes);
    }

    // 4) Main offsets before justification (packed at start of flow direction)
    let total: f32 = hypothetical_sizes.iter().copied().sum();
    let (start_offset, between_spacing) = justify_params(
        justify_content,
        container.container_main_size,
        total + gaps_total,
        items.len(),
    );
    debug!(
        target: "css::flexbox::single_line",
        "[FLEX-JUSTIFY] justify={:?} start_offset={:.3} between_spacing={:.3} total_including_gaps={:.3}",
        justify_content,
        start_offset,
        between_spacing,
        total + gaps_total
    );

    // 5) Direction (reverse flips order and offset accumulation)
    let plan = MainOffsetPlan {
        reverse: axes.main_reverse,
        container_main_size: container.container_main_size,
        start_offset,
        between_spacing,
        main_gap: container.main_gap.max(0.0),
    };
    let mut offsets: Vec<f32> = accumulate_main_offsets(&plan, &hypothetical_sizes);
    clamp_first_offset_if_needed(justify_content, axes.main_reverse, &mut offsets);
    if let Some(first) = offsets.first() {
        error!(
            target: "css::flexbox::single_line",
            "[FLEX-JUSTIFY] first_offset={:.3} start_offset={:.3} between_spacing={:.3}",
            *first,
            start_offset,
            between_spacing
        );
    }

    // 6) Build placements preserving input order.
    // Touch cross-axis helper so clippy sees it used pre-integration.
    let _align_dummy = align_single_line_cross(AlignItems::Center, 0.0, 0.0, 0.0, 0.0);
    items
        .iter()
        .zip(hypothetical_sizes.iter())
        .zip(offsets.iter())
        .map(|((child, size), offset)| FlexPlacement {
            handle: child.handle,
            main_size: *size,
            main_offset: *offset,
        })
        .collect()
}

/// Parameters for accumulating main-axis offsets.
#[derive(Copy, Clone, Debug)]
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
        let mut cursor = plan.container_main_size - plan.start_offset;
        let mut offsets_accum = Vec::with_capacity(sizes.len());
        let mut iter = sizes.iter().peekable();
        while let Some(size_ref) = iter.next() {
            let size_val = *size_ref;
            cursor -= size_val;
            offsets_accum.push(cursor);
            if iter.peek().is_some() {
                cursor -= plan.main_gap + plan.between_spacing;
            }
        }
        offsets_accum
    } else {
        let mut cursor = plan.start_offset;
        let mut offsets_accum = Vec::with_capacity(sizes.len());
        let mut iter = sizes.iter().peekable();
        while let Some(size_ref) = iter.next() {
            offsets_accum.push(cursor);
            cursor += *size_ref;
            if iter.peek().is_some() {
                cursor += plan.main_gap + plan.between_spacing;
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
        AlignItems::FlexStart => CrossPlacement {
            cross_size: clamped_item,
            cross_offset: 0.0,
        },
        AlignItems::FlexEnd => {
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
            },
            FlexChild {
                handle: ItemRef(2),
                flex_basis: 50.0,
                flex_grow: 1.0,
                flex_shrink: 1.0,
                min_main: 0.0,
                max_main: 1e9,
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
            },
            FlexChild {
                handle: ItemRef(2),
                flex_basis: 80.0,
                flex_grow: 0.0,
                flex_shrink: 1.0,
                min_main: 20.0,
                max_main: 1e9,
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
            },
            FlexChild {
                handle: ItemRef(2),
                flex_basis: 50.0,
                flex_grow: 0.0,
                flex_shrink: 0.0,
                min_main: 0.0,
                max_main: 1e9,
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
            container_cross_size: 100.0,
        };
        let out = layout_single_line_with_cross(
            container,
            JustifyContent::Center,
            cross_ctx,
            &items,
            &cross_inputs,
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
}
