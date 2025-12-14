//! Track resolution logic for computing track sizes.

use crate::GridAxis;
use crate::types::{GridTrack, GridTrackSize, TrackBreadth};

use super::helpers::calculate_auto_track_size;
use super::{NonFlexResult, ResolvedTrackSizes, TrackSizingContext};

/// Helper struct for track resolution state.
pub struct TrackResolutionState<'state> {
    pub resolved: &'state mut ResolvedTrackSizes,
    pub remaining_space: &'state mut f32,
    pub flex_tracks: &'state mut Vec<(usize, f32)>,
    pub auto_tracks: &'state mut Vec<usize>,
}

/// Resolve all non-flexible tracks and collect flex tracks for later distribution.
pub fn resolve_non_flex_tracks<NodeId>(
    resolved: &mut ResolvedTrackSizes,
    tracks: &[GridTrack],
    remaining_space: &mut f32,
    ctx: &TrackSizingContext<'_, NodeId>,
) -> NonFlexResult {
    let mut flex_tracks = Vec::new();
    let mut auto_tracks = Vec::new();
    let mut state = TrackResolutionState {
        resolved,
        remaining_space,
        flex_tracks: &mut flex_tracks,
        auto_tracks: &mut auto_tracks,
    };

    for (idx, track) in tracks.iter().enumerate() {
        match &track.size {
            GridTrackSize::Breadth(breadth) => {
                resolve_breadth_track(idx, breadth, &mut state, ctx);
            }
            GridTrackSize::MinMax(min_breadth, max_breadth) => {
                resolve_minmax_track(idx, min_breadth, max_breadth, &mut state, ctx);
            }
            GridTrackSize::FitContent(_) => {
                // Simplified: treat as auto
                let content_size =
                    calculate_auto_track_size(ctx.items, ctx.placements, idx, ctx.axis);
                state.resolved.base_sizes[idx] = content_size;
                state.resolved.growth_limits[idx] = content_size;
                *state.remaining_space -= content_size;
                state.auto_tracks.push(idx);
            }
        }
    }

    (flex_tracks, auto_tracks)
}

/// Resolve a breadth-based track (single value like `1fr`, `100px`, `auto`).
fn resolve_breadth_track<NodeId>(
    idx: usize,
    breadth: &TrackBreadth,
    state: &mut TrackResolutionState<'_>,
    ctx: &TrackSizingContext<'_, NodeId>,
) {
    match breadth {
        TrackBreadth::Length(len) => {
            // Fixed length
            state.resolved.base_sizes[idx] = *len;
            state.resolved.growth_limits[idx] = *len;
            *state.remaining_space -= *len;
        }
        TrackBreadth::Percentage(pct) => {
            // Percentage of available space
            let size = ctx.available_for_tracks * pct;
            state.resolved.base_sizes[idx] = size;
            state.resolved.growth_limits[idx] = size;
            *state.remaining_space -= size;
        }
        TrackBreadth::Flex(factor) => {
            // Store for later processing
            state.flex_tracks.push((idx, *factor));
        }
        TrackBreadth::Auto | TrackBreadth::MinContent | TrackBreadth::MaxContent => {
            // Auto and content-based tracks get content size (simplified)
            let content_size = calculate_auto_track_size(ctx.items, ctx.placements, idx, ctx.axis);

            if matches!(ctx.axis, GridAxis::Row) {
                tracing::info!(
                    "resolve_breadth_track: ROW track {} (breadth={:?}) -> content_size={:.1}px",
                    idx,
                    breadth,
                    content_size
                );
            }

            state.resolved.base_sizes[idx] = content_size;
            state.resolved.growth_limits[idx] = content_size;
            *state.remaining_space -= content_size;
            state.auto_tracks.push(idx);
        }
    }
}

/// Resolve a `minmax()` track.
fn resolve_minmax_track<NodeId>(
    idx: usize,
    min_breadth: &TrackBreadth,
    max_breadth: &TrackBreadth,
    state: &mut TrackResolutionState<'_>,
    ctx: &TrackSizingContext<'_, NodeId>,
) {
    // Simplified minmax: use min as base, max as limit
    let min_size = match min_breadth {
        TrackBreadth::Length(len) => *len,
        TrackBreadth::Percentage(pct) => ctx.available_for_tracks * pct,
        TrackBreadth::Auto => calculate_auto_track_size(ctx.items, ctx.placements, idx, ctx.axis),
        _ => 0.0,
    };

    // Check if max is flexible
    let is_flex_max = matches!(max_breadth, TrackBreadth::Flex(_));

    let max_size = match max_breadth {
        TrackBreadth::Length(len) => *len,
        TrackBreadth::Percentage(pct) => ctx.available_for_tracks * pct,
        TrackBreadth::Flex(factor) => {
            state.flex_tracks.push((idx, *factor));
            f32::INFINITY
        }
        _ => f32::INFINITY,
    };

    state.resolved.base_sizes[idx] = min_size;
    state.resolved.growth_limits[idx] = max_size;

    // Only subtract from remaining space if this is NOT a flex track
    // Flex tracks (including minmax with flex max) get sized in the flex distribution phase
    // and should have access to the full remaining space
    if !is_flex_max {
        *state.remaining_space -= min_size;
    }
}
