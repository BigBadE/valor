//! Baseline alignment support for flex items.

use super::super::AlignItems;
use super::alignment::CrossSize;

/// Optional baseline metrics per item: `(first_baseline, last_baseline)`.
pub type BaselineMetrics = Option<(f32, f32)>;

/// Compute the reference baseline value for a line from per-item baseline metrics and cross sizes.
pub fn compute_line_baseline_ref(
    align: AlignItems,
    baselines: &[BaselineMetrics],
    cross_inputs: &[(CrossSize, f32, f32)],
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
            let intrinsic = item_cross.intrinsic_size();
            let clamped = candidate.max(0.0).min(intrinsic);
            if clamped > reference {
                reference = clamped;
            }
        }
    }
    reference
}
