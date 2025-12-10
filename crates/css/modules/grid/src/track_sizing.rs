//! Grid track sizing algorithm.
//!
//! Spec: §12 Grid Sizing
//! <https://www.w3.org/TR/css-grid-2/#algo-track-sizing>

use crate::placement::GridArea;
use crate::types::{GridItem, GridTrack, GridTrackSize, TrackBreadth, TrackListType, TrackRepeat};

/// Axis identifier (row or column).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridAxis {
    /// Row axis (inline in horizontal writing mode)
    Row,
    /// Column axis (block in horizontal writing mode)
    Column,
}

/// Tracks for a specific axis.
#[derive(Debug, Clone)]
pub struct GridAxisTracks {
    /// Track definitions
    pub tracks: Vec<GridTrack>,
    /// Gap between tracks
    pub gap: f32,
}

impl GridAxisTracks {
    /// Create a new axis tracks definition.
    pub fn new(tracks: Vec<GridTrack>, gap: f32) -> Self {
        Self { tracks, gap }
    }

    /// Get the number of tracks.
    pub fn count(&self) -> usize {
        self.tracks.len()
    }
}

/// Resolved track sizes after running the track sizing algorithm.
#[derive(Debug, Clone)]
pub struct ResolvedTrackSizes {
    /// Base sizes for each track
    pub base_sizes: Vec<f32>,
    /// Growth limits for each track
    pub growth_limits: Vec<f32>,
}

impl ResolvedTrackSizes {
    /// Create a new resolved track sizes with the given count.
    pub fn new(count: usize) -> Self {
        Self {
            base_sizes: vec![0.0; count],
            growth_limits: vec![f32::INFINITY; count],
        }
    }

    /// Get the final size for a track (minimum of base size and growth limit).
    pub fn final_size(&self, index: usize) -> f32 {
        if index < self.base_sizes.len() {
            self.base_sizes[index].min(self.growth_limits[index])
        } else {
            0.0
        }
    }
}

/// Expand a track list with `repeat()` notation into explicit tracks.
///
/// Spec: §7.2.3 Repeat notation
fn _expand_track_list(
    tracks: &[GridTrackSize],
    repeats: &[TrackRepeat],
    available_size: f32,
) -> Vec<GridTrack> {
    let mut expanded = Vec::new();

    // Add explicit tracks
    for track_size in tracks {
        expanded.push(GridTrack {
            size: track_size.clone(),
            track_type: TrackListType::Explicit,
        });
    }

    // Process repeat patterns
    for repeat_pattern in repeats {
        match repeat_pattern {
            TrackRepeat::Count(count, pattern) => {
                for _ in 0..*count {
                    for track_size in pattern {
                        expanded.push(GridTrack {
                            size: track_size.clone(),
                            track_type: TrackListType::Explicit,
                        });
                    }
                }
            }
            TrackRepeat::AutoFill(pattern) | TrackRepeat::AutoFit(pattern) => {
                // Calculate how many repetitions fit in available space
                let pattern_size: f32 = pattern
                    .iter()
                    .map(|track_size| match track_size.min_breadth() {
                        TrackBreadth::Length(len) => *len,
                        _ => 200.0, // Default minimum for auto-fill/fit
                    })
                    .sum();

                let repetitions = if pattern_size > 0.0 {
                    (available_size / pattern_size).floor().max(1.0) as usize
                } else {
                    1
                };

                for _ in 0..repetitions {
                    for track_size in pattern {
                        expanded.push(GridTrack {
                            size: track_size.clone(),
                            track_type: TrackListType::Explicit,
                        });
                    }
                }
            }
        }
    }

    expanded
}

/// Resolve track sizes according to the grid sizing algorithm.
///
/// Spec: §12 Grid Sizing Algorithm
/// <https://www.w3.org/TR/css-grid-2/#algo-track-sizing>
///
/// This is a simplified MVP implementation that handles:
/// - Fixed length tracks (px)
/// - Flexible tracks (fr units)
/// - Percentage tracks
/// - Auto tracks (basic implementation)
/// - `minmax()` with simple min/max values
///
/// Not yet implemented:
/// - min-content/max-content intrinsic sizing
/// - `fit-content()`
/// - Complex content-based sizing
/// - Baseline alignment
///
/// Context for track sizing operations.
struct TrackSizingContext<'ctx, NodeId> {
    items: &'ctx [GridItem<NodeId>],
    placements: &'ctx [GridArea],
    axis: GridAxis,
    available_for_tracks: f32,
}

/// # Errors
/// Returns an error if track sizing calculation fails.
pub fn resolve_track_sizes<NodeId>(
    axis_tracks: &GridAxisTracks,
    available_size: f32,
    items: &[GridItem<NodeId>],
    placements: &[GridArea],
    axis: GridAxis,
) -> Result<ResolvedTrackSizes, String> {
    let track_count = axis_tracks.count();
    let mut resolved = ResolvedTrackSizes::new(track_count);

    // Calculate total gap space
    let total_gap = if track_count > 1 {
        axis_tracks.gap * (track_count - 1) as f32
    } else {
        0.0
    };

    let available_for_tracks = available_size - total_gap;
    let ctx = TrackSizingContext {
        items,
        placements,
        axis,
        available_for_tracks,
    };

    // Phase 1: Resolve fixed and percentage tracks
    let mut remaining_space = available_for_tracks;
    let (flex_tracks, _auto_tracks) = resolve_non_flex_tracks(
        &mut resolved,
        &axis_tracks.tracks,
        &mut remaining_space,
        &ctx,
    );

    // Phase 2: Distribute remaining space to flexible tracks
    distribute_flex_space(&mut resolved, &flex_tracks, remaining_space);

    Ok(resolved)
}

/// Resolve all non-flexible tracks and collect flex tracks for later distribution.
fn resolve_non_flex_tracks<NodeId>(
    resolved: &mut ResolvedTrackSizes,
    tracks: &[GridTrack],
    remaining_space: &mut f32,
    ctx: &TrackSizingContext<'_, NodeId>,
) -> (Vec<(usize, f32)>, Vec<usize>) {
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

/// Helper struct for track resolution state.
struct TrackResolutionState<'state> {
    resolved: &'state mut ResolvedTrackSizes,
    remaining_space: &'state mut f32,
    flex_tracks: &'state mut Vec<(usize, f32)>,
    auto_tracks: &'state mut Vec<usize>,
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
    *state.remaining_space -= min_size;
}

/// Distribute remaining space among flexible tracks.
fn distribute_flex_space(
    resolved: &mut ResolvedTrackSizes,
    flex_tracks: &[(usize, f32)],
    remaining_space: f32,
) {
    if !flex_tracks.is_empty() && remaining_space > 0.0 {
        let total_flex: f32 = flex_tracks.iter().map(|(_, factor)| factor).sum();

        if total_flex > 0.0 {
            for (idx, factor) in flex_tracks {
                let flex_share = remaining_space * (factor / total_flex);
                resolved.base_sizes[*idx] = flex_share;
                resolved.growth_limits[*idx] = flex_share;
            }
        }
    }
}

/// Distribute remaining space equally among auto tracks.
fn distribute_auto_space(
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

/// Calculate the auto size for a track based on its content.
///
/// This is a simplified implementation that estimates content size.
/// In a full implementation, this would measure actual content.
fn calculate_auto_track_size<NodeId>(
    items: &[GridItem<NodeId>],
    placements: &[GridArea],
    track_idx: usize,
    axis: GridAxis,
) -> f32 {
    let track_line = track_idx + 1; // Convert to 1-indexed
    let mut max_size = 0.0f32;

    // Find items that occupy this track
    for (item, area) in items.iter().zip(placements.iter()) {
        let occupies_track = match axis {
            GridAxis::Row => area.row_start <= track_line && area.row_end > track_line,
            GridAxis::Column => area.col_start <= track_line && area.col_end > track_line,
        };

        if occupies_track {
            let item_size = match axis {
                GridAxis::Row => item.max_content_height,
                GridAxis::Column => item.max_content_width,
            };

            // For items spanning multiple tracks, divide size evenly (simplified)
            let span = match axis {
                GridAxis::Row => area.row_span(),
                GridAxis::Column => area.col_span(),
            };

            let size_per_track = if span > 0 {
                item_size / span as f32
            } else {
                item_size
            };

            max_size = max_size.max(size_per_track);
        }
    }

    max_size
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::GridItem;

    /// Test resolution of fixed-size tracks.
    ///
    /// # Panics
    /// Panics if track resolution fails or assertions fail.
    #[test]
    fn test_resolve_fixed_tracks() {
        const EPSILON: f32 = 1e-6;

        let tracks = vec![
            GridTrack {
                size: GridTrackSize::Breadth(TrackBreadth::Length(100.0)),
                track_type: TrackListType::Explicit,
            },
            GridTrack {
                size: GridTrackSize::Breadth(TrackBreadth::Length(200.0)),
                track_type: TrackListType::Explicit,
            },
        ];

        let axis_tracks = GridAxisTracks::new(tracks, 10.0);
        let items: Vec<GridItem<()>> = vec![];
        let placements = vec![];

        let result =
            resolve_track_sizes(&axis_tracks, 400.0, &items, &placements, GridAxis::Column)
                .ok()
                .unwrap_or_else(|| ResolvedTrackSizes::new(0));

        assert!((result.base_sizes[0] - 100.0).abs() < EPSILON);
        assert!((result.base_sizes[1] - 200.0).abs() < EPSILON);
    }

    /// Test resolution of flexible tracks with fr units.
    ///
    /// # Panics
    /// Panics if track resolution fails or assertions fail.
    #[test]
    fn test_resolve_flex_tracks() {
        const EPSILON: f32 = 1e-6;
        const FLEX_EPSILON: f32 = 0.1;

        let tracks = vec![
            GridTrack {
                size: GridTrackSize::Breadth(TrackBreadth::Length(100.0)),
                track_type: TrackListType::Explicit,
            },
            GridTrack {
                size: GridTrackSize::Breadth(TrackBreadth::Flex(1.0)),
                track_type: TrackListType::Explicit,
            },
            GridTrack {
                size: GridTrackSize::Breadth(TrackBreadth::Flex(2.0)),
                track_type: TrackListType::Explicit,
            },
        ];

        let axis_tracks = GridAxisTracks::new(tracks, 0.0);
        let items: Vec<GridItem<()>> = vec![];
        let placements = vec![];

        let result =
            resolve_track_sizes(&axis_tracks, 400.0, &items, &placements, GridAxis::Column)
                .ok()
                .unwrap_or_else(|| ResolvedTrackSizes::new(0));

        assert!((result.base_sizes[0] - 100.0).abs() < EPSILON);
        // Remaining 300px distributed as 1:2
        assert!((result.base_sizes[1] - 100.0).abs() < FLEX_EPSILON);
        assert!((result.base_sizes[2] - 200.0).abs() < FLEX_EPSILON);
    }
}
