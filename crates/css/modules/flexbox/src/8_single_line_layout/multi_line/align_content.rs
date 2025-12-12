//! Align-content packing logic for multi-line flex containers.

use log::debug;

use super::super::distribution::main_axis::quantize_layout;
use super::super::{AlignContent, CrossContext};
use super::line_breaking::LineRange;

/// Compute align-content start offset and between-spacing (excluding CSS gap) for lines.
///
/// Modes:
/// - Start/End/Center: pack lines against start/end or center them in remaining space.
/// - SpaceBetween/Around/Evenly: distribute remaining space between line boxes.
/// - Stretch: treated as Start in this MVP (line box stretching not implemented here).
pub fn align_content_params(
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
    // Use floor quantization for between-spacing to avoid accumulating rounding errors
    (
        quantize_layout(start),
        super::super::distribution::quantize_layout_floor(between),
    )
}

/// Stretch line boxes to fill container cross-size according to `align-content`.
pub fn stretch_line_crosses(cross_ctx: &CrossContext, per_line_cross_max: &[f32]) -> Vec<f32> {
    let mut line_cross_vec: Vec<f32> = per_line_cross_max.to_vec();
    let lines_total_cross: f32 = line_cross_vec.iter().copied().sum();
    let line_count = line_cross_vec.len();
    debug!(
        "[ALIGN-CONTENT] mode={:?} container_cross={:.3} lines_total={:.3} line_count={}",
        cross_ctx.align_content, cross_ctx.container_cross_size, lines_total_cross, line_count
    );
    if matches!(cross_ctx.align_content, AlignContent::Stretch) && line_count > 0 {
        let gaps_total = if line_count > 1 {
            (line_count as f32 - 1.0) * cross_ctx.cross_gap.max(0.0)
        } else {
            0.0
        };
        let remaining = (cross_ctx.container_cross_size - lines_total_cross - gaps_total).max(0.0);
        let add_each = remaining / line_count as f32;
        debug!("[ALIGN-CONTENT] stretch: remaining={remaining:.3} add_each={add_each:.3}");
        for value in &mut line_cross_vec {
            *value += add_each;
        }
    }
    line_cross_vec
}

/// Inputs for line packing.
pub struct PackInputs<'inputs> {
    /// Per-item cross inputs `(cross_size, min_cross, max_cross)`
    pub cross_inputs: &'inputs [(f32, f32, f32)],
    /// Per-item baseline metrics if available
    pub baseline_inputs: &'inputs [super::super::cross_axis::BaselineMetrics],
    /// Ranges of items per line
    pub line_ranges: &'inputs [LineRange],
    /// Main-axis placements per line
    pub per_line_main: &'inputs [Vec<super::super::FlexPlacement>],
    /// Resolved cross-size per line (after stretch)
    pub line_cross_vec: &'inputs [f32],
}
