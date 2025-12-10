//! Grid layout algorithm.
//!
//! Spec: §12 Grid Sizing
//! <https://www.w3.org/TR/css-grid-2/#layout-algorithm>

use crate::placement::{GridArea, place_grid_items};
use crate::track_sizing::{GridAxis, GridAxisTracks, ResolvedTrackSizes, resolve_track_sizes};
use crate::types::{GridAutoFlow, GridItem};

/// Alignment values for grid items.
///
/// Spec: §10 Alignment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GridAlignment {
    /// Start alignment
    Start,
    /// End alignment
    End,
    /// Center alignment
    Center,
    /// Stretch to fill
    #[default]
    Stretch,
}

/// Input parameters for grid layout.
#[derive(Debug, Clone)]
pub struct GridContainerInputs {
    /// Row tracks definition
    pub row_tracks: GridAxisTracks,
    /// Column tracks definition
    pub col_tracks: GridAxisTracks,
    /// Auto-flow direction
    pub auto_flow: GridAutoFlow,
    /// Available width for the grid
    pub available_width: f32,
    /// Available height for the grid
    pub available_height: f32,
    /// Align items in their grid area (cross-axis)
    pub align_items: GridAlignment,
    /// Justify items in their grid area (main-axis)
    pub justify_items: GridAlignment,
    /// Whether container has explicit height (distribute space to auto row tracks)
    pub has_explicit_height: bool,
}

impl GridContainerInputs {
    /// Create a new grid container inputs with default values.
    pub fn new(
        row_tracks: GridAxisTracks,
        col_tracks: GridAxisTracks,
        available_width: f32,
        available_height: f32,
    ) -> Self {
        Self {
            row_tracks,
            col_tracks,
            auto_flow: GridAutoFlow::default(),
            available_width,
            available_height,
            align_items: GridAlignment::default(),
            justify_items: GridAlignment::default(),
            has_explicit_height: false,
        }
    }
}

/// A grid item with its final position and size.
#[derive(Debug, Clone)]
pub struct GridPlacedItem<NodeId = usize> {
    /// Node identifier (generic to support different node ID types)
    pub node_id: NodeId,
    /// Final x position (border-box)
    pub x: f32,
    /// Final y position (border-box)
    pub y: f32,
    /// Final width (border-box)
    pub width: f32,
    /// Final height (border-box)
    pub height: f32,
    /// Grid area occupied
    pub area: GridArea,
}

/// Result of grid layout computation.
#[derive(Debug, Clone)]
pub struct GridLayoutResult<NodeId = usize> {
    /// Placed items with their positions
    pub items: Vec<GridPlacedItem<NodeId>>,
    /// Total width consumed by the grid
    pub total_width: f32,
    /// Total height consumed by the grid
    pub total_height: f32,
    /// Resolved column sizes
    pub col_sizes: ResolvedTrackSizes,
    /// Resolved row sizes
    pub row_sizes: ResolvedTrackSizes,
}

/// Run the grid layout algorithm.
///
/// Spec: §12 Grid Sizing Algorithm
/// <https://www.w3.org/TR/css-grid-2/#layout-algorithm>
///
/// This MVP implementation:
/// 1. Places items using the auto-placement algorithm
/// 2. Resolves track sizes for rows and columns
/// 3. Positions items within their grid areas
/// 4. Applies basic alignment
///
/// # Errors
/// Returns an error if layout computation fails.
pub fn layout_grid<NodeId: Clone>(
    items: &[GridItem<NodeId>],
    inputs: &GridContainerInputs,
) -> Result<GridLayoutResult<NodeId>, String> {
    // Ensure we have at least one track in each dimension
    let row_count = inputs.row_tracks.count().max(1);
    let col_count = inputs.col_tracks.count().max(1);

    // Step 1: Place grid items
    let placements = place_grid_items(items, row_count, col_count, inputs.auto_flow)?;

    // Step 2: Resolve column track sizes (don't distribute to auto tracks)
    let col_sizes = resolve_track_sizes(
        &inputs.col_tracks,
        inputs.available_width,
        items,
        &placements,
        GridAxis::Column,
        false, // Don't distribute to auto column tracks
    )?;

    // Step 3: Resolve row track sizes
    // Distribute to auto tracks if container has explicit height
    let row_sizes = resolve_track_sizes(
        &inputs.row_tracks,
        inputs.available_height,
        items,
        &placements,
        GridAxis::Row,
        inputs.has_explicit_height, // Distribute if explicit height
    )?;

    // Step 4: Position items in their grid areas
    let mut placed_items = Vec::with_capacity(items.len());

    for (item, area) in items.iter().zip(placements.iter()) {
        let (x_pos, width) = calculate_item_position_and_size(
            area.col_start,
            area.col_end,
            &col_sizes,
            inputs.col_tracks.gap,
            inputs.justify_items,
        );

        let (y_pos, height) = calculate_item_position_and_size(
            area.row_start,
            area.row_end,
            &row_sizes,
            inputs.row_tracks.gap,
            inputs.align_items,
        );

        placed_items.push(GridPlacedItem {
            node_id: item.node_id.clone(),
            x: x_pos,
            y: y_pos,
            width,
            height,
            area: *area,
        });
    }

    // Calculate total grid size
    let total_width = inputs.col_tracks.gap.mul_add(
        (col_count - 1) as f32,
        col_sizes.base_sizes.iter().sum::<f32>(),
    );

    let total_height = inputs.row_tracks.gap.mul_add(
        (row_count - 1) as f32,
        row_sizes.base_sizes.iter().sum::<f32>(),
    );

    Ok(GridLayoutResult {
        items: placed_items,
        total_width,
        total_height,
        col_sizes,
        row_sizes,
    })
}

/// Calculate the position and size of an item in its grid area along one axis.
fn calculate_item_position_and_size(
    start_line: usize,
    end_line: usize,
    track_sizes: &ResolvedTrackSizes,
    gap: f32,
    _alignment: GridAlignment,
) -> (f32, f32) {
    // Calculate the area start position and size
    let mut area_start = 0.0;
    let mut area_size = 0.0;

    // Sum up track sizes before start_line
    for idx in 0..(start_line.saturating_sub(1)) {
        if idx < track_sizes.base_sizes.len() {
            area_start += track_sizes.base_sizes[idx];
            // Add gap after each track (gaps go between tracks)
            area_start += gap;
        }
    }

    // Sum up track sizes in the span
    for idx in (start_line.saturating_sub(1))..(end_line.saturating_sub(1)) {
        if idx < track_sizes.base_sizes.len() {
            area_size += track_sizes.base_sizes[idx];
            if idx > start_line.saturating_sub(1) {
                area_size += gap;
            }
        }
    }

    // For MVP, always stretch to fill the area
    // In full implementation, alignment would adjust position and size
    let position = area_start;
    let size = area_size;

    (position, size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{GridTrack, GridTrackSize, TrackBreadth, TrackListType};

    /// Test basic grid layout.
    ///
    /// # Panics
    /// Panics if layout computation fails or assertions fail.
    #[test]
    fn test_grid_layout_basic() {
        let items = vec![
            GridItem::new(1),
            GridItem::new(2),
            GridItem::new(3),
            GridItem::new(4),
        ];

        let row_tracks = GridAxisTracks::new(
            vec![
                GridTrack {
                    size: GridTrackSize::Breadth(TrackBreadth::Length(100.0)),
                    track_type: TrackListType::Explicit,
                },
                GridTrack {
                    size: GridTrackSize::Breadth(TrackBreadth::Length(100.0)),
                    track_type: TrackListType::Explicit,
                },
            ],
            10.0,
        );

        let col_tracks = GridAxisTracks::new(
            vec![
                GridTrack {
                    size: GridTrackSize::Breadth(TrackBreadth::Length(150.0)),
                    track_type: TrackListType::Explicit,
                },
                GridTrack {
                    size: GridTrackSize::Breadth(TrackBreadth::Length(150.0)),
                    track_type: TrackListType::Explicit,
                },
            ],
            10.0,
        );

        let inputs = GridContainerInputs::new(row_tracks, col_tracks, 400.0, 300.0);

        let result = layout_grid(&items, &inputs)
            .ok()
            .unwrap_or_else(|| GridLayoutResult {
                items: vec![],
                total_width: 0.0,
                total_height: 0.0,
                col_sizes: ResolvedTrackSizes::new(0),
                row_sizes: ResolvedTrackSizes::new(0),
            });

        assert_eq!(result.items.len(), 4);
        assert!((result.total_width - 310.0).abs() < 0.1); // 150 + 150 + 10 gap
        assert!((result.total_height - 210.0).abs() < 0.1); // 100 + 100 + 10 gap
    }
}
