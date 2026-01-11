//! Cross-axis alignment and sizing for flex items.

use super::super::{AlignItems, CrossPlacement};
use super::baseline::BaselineMetrics;

/// Cross-size specification for flex items, distinguishing between
/// explicit sizes and items that should stretch to fill the container.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum CrossSize {
    /// Item has explicit cross-size (or intrinsic size with no stretch)
    Explicit(f32),
    /// Item should stretch to container; value is the measured intrinsic size
    Stretch(f32),
}

impl CrossSize {
    /// Get the intrinsic cross size (the size before alignment/stretching)
    pub fn intrinsic_size(self) -> f32 {
        match self {
            Self::Explicit(size) | Self::Stretch(size) => size,
        }
    }

    /// Check if this item should stretch
    pub fn should_stretch(self) -> bool {
        matches!(self, Self::Stretch(_))
    }
}

/// Compute cross-axis placement and size given `align-items` for single-line containers.
///
/// Behavior:
/// - Stretch: cross-size becomes container cross-size clamped by min/max; offset 0.
/// - Center: cross-size remains the item cross-size (clamped) and offset centers it in the container.
pub fn align_single_line_cross(
    align: AlignItems,
    container_cross_size: f32,
    item_cross_size: CrossSize,
    min_cross: f32,
    max_cross: f32,
) -> CrossPlacement {
    let intrinsic = item_cross_size.intrinsic_size();
    let clamped_item = super::super::distribution::clamp(intrinsic, min_cross, max_cross);

    match align {
        AlignItems::Stretch => {
            // Stretch applies when the item doesn't have an explicit cross-size
            if item_cross_size.should_stretch() {
                CrossPlacement {
                    cross_size: super::super::distribution::clamp(
                        container_cross_size,
                        min_cross,
                        max_cross,
                    ),
                    cross_offset: 0.0,
                }
            } else {
                // Item has explicit cross-size - use it, no stretching
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

/// Compute cross-axis placement for multiple items using `align-items`.
/// Each tuple is `(item_cross_size, min_cross, max_cross)`.
pub fn align_cross_for_items(
    align: AlignItems,
    container_cross_size: f32,
    items: &[(CrossSize, f32, f32)],
) -> Vec<CrossPlacement> {
    items
        .iter()
        .map(|&(item_size, min_c, max_c)| {
            align_single_line_cross(align, container_cross_size, item_size, min_c, max_c)
        })
        .collect()
}

/// Context for adjusting cross offsets to align baselines within a line.
pub struct BaselineAdjustCtx<'baseline> {
    /// The effective `align-items` mode for baseline behavior.
    pub align: AlignItems,
    /// Per-item baseline metrics for the current line.
    pub baselines: &'baseline [BaselineMetrics],
    /// Index of the item within the current line.
    pub index_in_line: usize,
    /// The target baseline reference for the line.
    pub line_ref: f32,
    /// The cross-size of the current line.
    pub line_cross_max: f32,
    /// The accumulated cross offset of previous lines.
    pub cross_accum_offset: f32,
}

/// Adjust the `cross_placement` so that the item's chosen baseline matches the line reference.
pub fn adjust_cross_for_baseline(
    ctx: &BaselineAdjustCtx<'_>,
    cross_placement: &mut CrossPlacement,
) {
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
