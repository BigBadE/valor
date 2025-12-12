//! Grid track sizing algorithm.
//!
//! Spec: §12 Grid Sizing
//! <https://www.w3.org/TR/css-grid-2/#algo-track-sizing>

mod distribution;
mod expansion;
mod helpers;
mod resolution;

use crate::placement::GridArea;
use crate::types::{GridItem, GridTrack, TrackRepeat};

pub use distribution::{distribute_auto_space, distribute_flex_space};
pub use expansion::{
    add_implicit_tracks_for_placements, collapse_auto_fit_tracks, expand_auto_repeat_tracks,
};
pub use resolution::resolve_non_flex_tracks;

/// Result type for non-flex track resolution: (`flex_tracks`, `auto_tracks`)
pub type NonFlexResult = (Vec<(usize, f32)>, Vec<usize>);

/// Parameters for track sizing.
#[derive(Debug)]
pub struct TrackSizingParams<'params, NodeId> {
    /// Axis tracks definition
    pub axis_tracks: &'params GridAxisTracks,
    /// Available size for this axis
    pub available_size: f32,
    /// Grid items to place
    pub items: &'params [GridItem<NodeId>],
    /// Placement results
    pub placements: &'params [GridArea],
    /// Axis being sized
    pub axis: GridAxis,
    /// Whether to distribute remaining space to auto tracks
    pub distribute_auto: bool,
}

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
    /// Optional auto-repeat pattern (auto-fit/auto-fill)
    pub auto_repeat: Option<TrackRepeat>,
}

impl GridAxisTracks {
    /// Create a new axis tracks definition.
    pub fn new(tracks: Vec<GridTrack>, gap: f32) -> Self {
        Self {
            tracks,
            gap,
            auto_repeat: None,
        }
    }

    /// Create a new axis tracks definition with auto-repeat pattern.
    pub fn with_auto_repeat(tracks: Vec<GridTrack>, gap: f32, auto_repeat: TrackRepeat) -> Self {
        Self {
            tracks,
            gap,
            auto_repeat: Some(auto_repeat),
        }
    }

    /// Get the number of explicit tracks (before auto-repeat expansion).
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

/// Context for track sizing operations.
pub struct TrackSizingContext<'ctx, NodeId> {
    pub items: &'ctx [GridItem<NodeId>],
    pub placements: &'ctx [GridArea],
    pub axis: GridAxis,
    pub available_for_tracks: f32,
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
/// # Errors
/// Returns an error if track sizing calculation fails.
pub fn resolve_track_sizes<NodeId>(
    params: &TrackSizingParams<'_, NodeId>,
) -> Result<ResolvedTrackSizes, String> {
    tracing::debug!(
        "resolve_track_sizes: axis={:?}, available_size={}, items_count={}",
        params.axis,
        params.available_size,
        params.items.len()
    );

    // Check if we're using auto-fit (needed for collapsing empty tracks later)
    let is_auto_fit = matches!(
        params.axis_tracks.auto_repeat,
        Some(TrackRepeat::AutoFit(_))
    );

    // Step 1: Expand auto-repeat if present
    let expanded_tracks = expand_auto_repeat_tracks(params);

    tracing::debug!("After expansion: {} tracks", expanded_tracks.len());

    // Step 2: Add implicit tracks for items placed beyond the explicit grid
    let mut final_tracks =
        add_implicit_tracks_for_placements(expanded_tracks, params.placements, params.axis);

    tracing::debug!("After implicit tracks: {} tracks", final_tracks.len());

    // Step 3: Collapse empty auto-fit tracks (must happen after placement)
    final_tracks =
        collapse_auto_fit_tracks(final_tracks, params.placements, params.axis, is_auto_fit);
    let final_track_count = final_tracks.len();

    tracing::debug!("After collapsing auto-fit: {} tracks", final_track_count);

    let mut resolved = ResolvedTrackSizes::new(final_track_count);

    // Calculate total gap space
    let total_gap = if final_track_count > 1 {
        params.axis_tracks.gap * (final_track_count - 1) as f32
    } else {
        0.0
    };

    let available_for_tracks = params.available_size - total_gap;
    tracing::debug!(
        "Available space: total={}, gap_total={}, available_for_tracks={}",
        params.available_size,
        total_gap,
        available_for_tracks
    );

    let ctx = TrackSizingContext {
        items: params.items,
        placements: params.placements,
        axis: params.axis,
        available_for_tracks,
    };

    // Phase 1: Resolve fixed and percentage tracks
    let mut remaining_space = available_for_tracks;
    let (flex_tracks, auto_tracks) =
        resolve_non_flex_tracks(&mut resolved, &final_tracks, &mut remaining_space, &ctx);

    tracing::debug!(
        "After non-flex resolution: remaining_space={}, flex_tracks={:?}, auto_tracks={:?}",
        remaining_space,
        flex_tracks,
        auto_tracks
    );

    // Phase 1.5: If requested and no flex tracks, distribute remaining space to auto tracks
    if params.distribute_auto
        && flex_tracks.is_empty()
        && !auto_tracks.is_empty()
        && remaining_space > 0.0
    {
        distribute_auto_space(&mut resolved, &auto_tracks, remaining_space);
        remaining_space = 0.0;
    }

    // Phase 2: Distribute remaining space to flexible tracks
    distribute_flex_space(&mut resolved, &flex_tracks, remaining_space);

    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{GridTrackSize, TrackBreadth, TrackListType};

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

        let params = TrackSizingParams {
            axis_tracks: &axis_tracks,
            available_size: 400.0,
            items: &items,
            placements: &placements,
            axis: GridAxis::Column,
            distribute_auto: false,
        };
        let result = resolve_track_sizes(&params)
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

        let params = TrackSizingParams {
            axis_tracks: &axis_tracks,
            available_size: 400.0,
            items: &items,
            placements: &placements,
            axis: GridAxis::Column,
            distribute_auto: false,
        };
        let result = resolve_track_sizes(&params)
            .ok()
            .unwrap_or_else(|| ResolvedTrackSizes::new(0));

        assert!((result.base_sizes[0] - 100.0).abs() < EPSILON);
        // Remaining 300px distributed as 1:2
        assert!((result.base_sizes[1] - 100.0).abs() < FLEX_EPSILON);
        assert!((result.base_sizes[2] - 200.0).abs() < FLEX_EPSILON);
    }

    /// Test auto-fit with minmax(200px, 1fr).
    ///
    /// This simulates: grid-template-columns: repeat(auto-fit, minmax(200px, 1fr))
    /// With 569px available and 10px gap:
    /// - Should fit 2 columns (2*200 + 10 = 410 < 569)
    /// - After collapsing to 1 (one item), should give that column the full width
    ///
    /// # Panics
    /// Panics if track resolution fails or assertions fail.
    #[test]
    fn test_auto_fit_minmax_single_item() {
        use crate::placement::GridArea;
        use crate::types::TrackRepeat;

        // Create minmax(200px, 1fr) track definition
        let repeat_track =
            GridTrackSize::MinMax(TrackBreadth::Length(200.0), TrackBreadth::Flex(1.0));

        let axis_tracks = GridAxisTracks::with_auto_repeat(
            vec![], // No pre/post tracks
            10.0,   // gap
            TrackRepeat::AutoFit(vec![repeat_track]),
        );

        // Single item
        let items: Vec<GridItem<()>> = vec![GridItem::new(())];

        // Item placed in column 1-2 (first track)
        let placements = vec![GridArea {
            row_start: 1,
            row_end: 2,
            col_start: 1,
            col_end: 2,
        }];

        let params = TrackSizingParams {
            axis_tracks: &axis_tracks,
            available_size: 569.0, // Full width
            items: &items,
            placements: &placements,
            axis: GridAxis::Column,
            distribute_auto: false,
        };

        let result = resolve_track_sizes(&params)
            .ok()
            .unwrap_or_else(|| ResolvedTrackSizes::new(0));

        // Should have 1 column after collapsing empty tracks
        assert_eq!(
            result.base_sizes.len(),
            1,
            "Should collapse to 1 column, got {}",
            result.base_sizes.len()
        );

        // That column should get the full available width (569px)
        // Since it's minmax(200px, 1fr), it should grow to fill available space
        assert!(
            (result.base_sizes[0] - 569.0).abs() < 1.0,
            "Expected ~569px, got {}px",
            result.base_sizes[0]
        );
    }

    /// Test auto-fit with minmax(200px, 1fr) and multiple items.
    ///
    /// # Panics
    /// Panics if track resolution fails or assertions fail.
    #[test]
    fn test_auto_fit_minmax_multiple_items() {
        use crate::placement::GridArea;
        use crate::types::TrackRepeat;

        let repeat_track =
            GridTrackSize::MinMax(TrackBreadth::Length(200.0), TrackBreadth::Flex(1.0));
        let axis_tracks = GridAxisTracks::with_auto_repeat(
            vec![],
            10.0,
            TrackRepeat::AutoFit(vec![repeat_track]),
        );

        let items: Vec<GridItem<()>> = vec![GridItem::new(()); 5];

        // Place items in 2 columns (col 1: items 0,2,4; col 2: items 1,3)
        let placements = vec![
            GridArea {
                row_start: 1,
                row_end: 2,
                col_start: 1,
                col_end: 2,
            },
            GridArea {
                row_start: 1,
                row_end: 2,
                col_start: 2,
                col_end: 3,
            },
            GridArea {
                row_start: 2,
                row_end: 3,
                col_start: 1,
                col_end: 2,
            },
            GridArea {
                row_start: 2,
                row_end: 3,
                col_start: 2,
                col_end: 3,
            },
            GridArea {
                row_start: 3,
                row_end: 4,
                col_start: 1,
                col_end: 2,
            },
        ];

        let params = TrackSizingParams {
            axis_tracks: &axis_tracks,
            available_size: 569.0,
            items: &items,
            placements: &placements,
            axis: GridAxis::Column,
            distribute_auto: false,
        };

        let result = resolve_track_sizes(&params)
            .ok()
            .unwrap_or_else(|| ResolvedTrackSizes::new(0));

        assert_eq!(result.base_sizes.len(), 2);
        let expected = (569.0 - 10.0) / 2.0; // (total - gap) / 2 columns
        assert!((result.base_sizes[0] - expected).abs() < 1.0);
        assert!((result.base_sizes[1] - expected).abs() < 1.0);
    }
}
