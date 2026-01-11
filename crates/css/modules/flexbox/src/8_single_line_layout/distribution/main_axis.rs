//! Main-axis justification and positioning logic.

use log::debug;

use super::super::{FlexChild, FlexPlacement, JustifyContent};

/// Quantize a CSS pixel value to the layout unit (1/64 px) to match Chromium's subpixel model.
pub fn quantize_layout(value: f32) -> f32 {
    (value * 64.0).round() / 64.0
}

/// Compute justify-content start offset and between-spacing (excluding CSS gap).
pub fn justify_params(
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
    (
        quantize_layout(start),
        super::auto_margins::quantize_layout_floor(between),
    )
}

/// Build main-axis placements from inner sizes and outer starts (margin-aware starts) with
/// the resolved effective left margins (includes auto margin absorption).
///
/// Per CSS Flexbox spec 9.7, returns content-box sizes (same as flex-basis).
/// The caller is responsible for converting to border-box if needed for layout.
pub fn build_main_placements(
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
        .map(|(((child, inner_size), outer_start), eff_left)| {
            FlexPlacement {
                handle: child.handle,
                // Per spec 9.7, used main size is content-box (same as flex base size)
                main_size: *inner_size,
                main_offset: *outer_start + *eff_left,
            }
        })
        .collect()
}

/// Ensure the first item's offset aligns to main-start.
///
/// For Start and `SpaceBetween` when the main axis is not reversed, this guards against
/// any accidental pre-gap/start offset leaks. No effect for other justify modes or reverse axes.
pub fn clamp_first_offset_if_needed(
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
            "[FLEX-JUSTIFY] clamping first offset from {:.3} to 0.000 for {:?}",
            *first, justify_content
        );
        *first = 0.0;
    }
}

#[derive(Copy, Clone, Debug)]
/// Parameters for planning main-axis offset accumulation.
pub struct MainOffsetPlan {
    /// Whether the main axis runs in reverse order.
    pub reverse: bool,
    /// The container's definite main size in px.
    pub container_main_size: f32,
    /// Pre-placement offset from main-start.
    pub start_offset: f32,
    /// Extra spacing between items from justify-content (excludes CSS gap).
    pub between_spacing: f32,
    /// CSS main-axis gap between adjacent items in px.
    pub main_gap: f32,
}

/// Compute per-item main-axis offsets from a sizing plan and item sizes.
/// Quantizes final positions to avoid accumulating rounding errors.
pub fn accumulate_main_offsets(plan: &MainOffsetPlan, sizes: &[f32]) -> Vec<f32> {
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
                cursor = quantize_layout(cursor + plan.main_gap + plan.between_spacing);
            }
        }
        offsets_accum
    }
}
