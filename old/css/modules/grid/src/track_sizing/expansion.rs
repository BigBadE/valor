//! Track expansion logic for auto-repeat and implicit tracks.

use crate::placement::GridArea;
use crate::types::{GridTrack, GridTrackSize, TrackBreadth, TrackListType, TrackRepeat};

use super::{GridAxis, TrackSizingParams};

/// Expand auto-repeat tracks based on available space.
///
/// Spec: §7.2.3.2 Repeat-to-fill: auto-fill and auto-fit repetitions
pub fn expand_auto_repeat_tracks<NodeId>(params: &TrackSizingParams<'_, NodeId>) -> Vec<GridTrack> {
    let mut expanded = params.axis_tracks.tracks.clone();

    let Some(auto_repeat) = &params.axis_tracks.auto_repeat else {
        return expanded;
    };

    let (TrackRepeat::AutoFit(repeat_tracks) | TrackRepeat::AutoFill(repeat_tracks)) = auto_repeat
    else {
        return expanded; // Count variant
    };

    // Calculate the minimum size of one repetition
    let min_repeat_size: f32 = repeat_tracks
        .iter()
        .map(|track| match track.min_breadth() {
            TrackBreadth::Length(len) => *len,
            TrackBreadth::Percentage(pct) => pct * params.available_size,
            _ => 0.0, // Auto, min-content, max-content default to 0 for min
        })
        .sum();

    // Calculate how many repetitions fit
    // Must account for gaps: available_size = n * min_size + (n-1) * gap
    // Solving for n: n = (available_size + gap) / (min_size + gap)
    let gap = params.axis_tracks.gap;
    let repetitions = if min_repeat_size > 0.0 {
        let effective_track_size = min_repeat_size + gap;
        let adjusted_space = params.available_size + gap;
        (adjusted_space / effective_track_size).floor() as usize
    } else {
        1 // At least one repetition
    }
    .max(1); // Ensure at least one repetition

    // Expand the repeat pattern
    for _ in 0..repetitions {
        for track_size in repeat_tracks {
            expanded.push(GridTrack {
                size: track_size.clone(),
                track_type: TrackListType::Explicit,
            });
        }
    }

    expanded
}

/// Add implicit tracks for items placed beyond the explicit grid.
pub fn add_implicit_tracks_for_placements(
    mut tracks: Vec<GridTrack>,
    placements: &[GridArea],
    axis: GridAxis,
) -> Vec<GridTrack> {
    if placements.is_empty() {
        return tracks;
    }

    // Find the maximum line index used
    let max_line = placements
        .iter()
        .map(|area| match axis {
            GridAxis::Row => area.row_end,
            GridAxis::Column => area.col_end,
        })
        .max()
        .unwrap_or(1);

    // Tracks are between lines, so we need (max_line - 1) tracks
    let needed_tracks = max_line.saturating_sub(1);

    // Add implicit Auto tracks if needed
    while tracks.len() < needed_tracks {
        tracks.push(GridTrack {
            size: GridTrackSize::Breadth(TrackBreadth::Auto),
            track_type: TrackListType::Implicit,
        });
    }

    tracks
}

/// Check if a track has any items placed in or spanning across it.
fn track_has_items(track_idx: usize, placements: &[GridArea], axis: GridAxis) -> bool {
    let track_line = track_idx + 1; // Convert to 1-indexed line
    placements.iter().any(|area| match axis {
        GridAxis::Row => area.row_start <= track_line && area.row_end > track_line,
        GridAxis::Column => area.col_start <= track_line && area.col_end > track_line,
    })
}

/// Collapse empty auto-fit tracks to create a more compact grid.
///
/// Spec: §7.2.3.2 - The auto-fit keyword collapses empty repeated tracks.
/// An empty track is one with no in-flow grid items placed into or spanning across it.
pub fn collapse_auto_fit_tracks(
    tracks: Vec<GridTrack>,
    placements: &[GridArea],
    axis: GridAxis,
    was_auto_fit: bool,
) -> Vec<GridTrack> {
    if !was_auto_fit {
        return tracks;
    }

    // Filter out tracks that are:
    // 1. From auto-fit repetition (explicit tracks from auto-fit)
    // 2. Empty (no items placed in or spanning across them)
    let collapsed: Vec<GridTrack> = tracks
        .into_iter()
        .enumerate()
        .filter(|(idx, track)| {
            // Keep implicit tracks and tracks with items
            track.track_type == TrackListType::Implicit || track_has_items(*idx, placements, axis)
        })
        .map(|(_idx, track)| track)
        .collect();

    // Ensure at least one track remains
    if collapsed.is_empty() {
        vec![GridTrack {
            size: GridTrackSize::Breadth(TrackBreadth::Auto),
            track_type: TrackListType::Explicit,
        }]
    } else {
        collapsed
    }
}
