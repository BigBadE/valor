//! Space distribution logic for flexible and auto tracks.

use super::ResolvedTrackSizes;

/// Distribute remaining space among flexible tracks.
pub fn distribute_flex_space(
    resolved: &mut ResolvedTrackSizes,
    flex_tracks: &[(usize, f32)],
    remaining_space: f32,
) {
    if !flex_tracks.is_empty() && remaining_space > 0.0 {
        let total_flex: f32 = flex_tracks.iter().map(|(_, factor)| factor).sum();

        tracing::debug!(
            "distribute_flex_space: remaining_space={}, flex_tracks_count={}, total_flex={}",
            remaining_space,
            flex_tracks.len(),
            total_flex
        );

        if total_flex > 0.0 {
            for (idx, factor) in flex_tracks {
                let flex_share = remaining_space * (factor / total_flex);
                // Respect the minimum size (base_size may have been set by minmax min)
                // Take the maximum of the flex share and the existing base size
                let min_size = resolved.base_sizes[*idx];
                let final_size = flex_share.max(min_size);
                resolved.base_sizes[*idx] = final_size;
                resolved.growth_limits[*idx] = final_size;
                tracing::debug!(
                    "  Track {}: factor={}, flex_share={}, min={}, final={}",
                    idx,
                    factor,
                    flex_share,
                    min_size,
                    final_size
                );
            }
        }
    } else {
        tracing::debug!(
            "distribute_flex_space: skipped (flex_tracks.len()={}, remaining_space={})",
            flex_tracks.len(),
            remaining_space
        );
    }
}

/// Distribute remaining space equally among auto tracks.
pub fn distribute_auto_space(
    resolved: &mut ResolvedTrackSizes,
    auto_tracks: &[usize],
    remaining_space: f32,
) {
    if !auto_tracks.is_empty() && remaining_space > 0.0 {
        let space_per_track = remaining_space / auto_tracks.len() as f32;

        for &idx in auto_tracks {
            resolved.base_sizes[idx] += space_per_track;
            resolved.growth_limits[idx] += space_per_track;
        }
    }
}
