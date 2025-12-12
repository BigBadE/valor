//! Grid layout algorithm.
//!
//! Spec: §12 Grid Sizing
//! <https://www.w3.org/TR/css-grid-2/#layout-algorithm>

use crate::placement::{GridArea, place_grid_items};
use crate::track_sizing::{
    GridAxis, GridAxisTracks, ResolvedTrackSizes, expand_auto_repeat_tracks, resolve_track_sizes,
};
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

/// Expand auto-repeat tracks to determine actual track counts for placement.
///
/// CRITICAL: This must happen BEFORE item placement. Otherwise, with repeat(auto-fit, ...),
/// the unexpanded track count is 0, causing all items to be placed in column 1,
/// which then collapses to 1 column instead of expanding to multiple columns.
fn expand_tracks_for_placement<NodeId>(
    items: &[GridItem<NodeId>],
    inputs: &GridContainerInputs,
) -> (usize, usize) {
    // Expand column tracks to determine actual column count
    let col_params = crate::track_sizing::TrackSizingParams {
        axis_tracks: &inputs.col_tracks,
        available_size: inputs.available_width,
        items,
        placements: &[], // No placements yet
        axis: GridAxis::Column,
        distribute_auto: false,
    };
    let expanded_col_tracks = expand_auto_repeat_tracks(&col_params);
    let col_count = expanded_col_tracks.len().max(1);

    // Expand row tracks to determine actual row count
    let row_params = crate::track_sizing::TrackSizingParams {
        axis_tracks: &inputs.row_tracks,
        available_size: inputs.available_height,
        items,
        placements: &[], // No placements yet
        axis: GridAxis::Row,
        distribute_auto: false,
    };
    let expanded_row_tracks = expand_auto_repeat_tracks(&row_params);
    let row_count = expanded_row_tracks.len().max(1);

    (row_count, col_count)
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
    tracing::debug!(
        "layout_grid: items={}, row_tracks.count()={}, col_tracks.count()={}",
        items.len(),
        inputs.row_tracks.count(),
        inputs.col_tracks.count()
    );

    // Step 1: Expand auto-repeat tracks to determine actual counts for placement
    let (row_count, col_count) = expand_tracks_for_placement(items, inputs);

    tracing::debug!(
        "After expansion: row_count={}, col_count={}",
        row_count,
        col_count
    );

    // Step 2: Place grid items using expanded track counts
    let placements = place_grid_items(items, row_count, col_count, inputs.auto_flow)?;

    tracing::debug!("Placements: {:?}", placements);

    // Step 2: Resolve column track sizes (don't distribute to auto tracks)
    let col_params = crate::track_sizing::TrackSizingParams {
        axis_tracks: &inputs.col_tracks,
        available_size: inputs.available_width,
        items,
        placements: &placements,
        axis: GridAxis::Column,
        distribute_auto: false,
    };
    let col_sizes = resolve_track_sizes(&col_params)?;

    // Step 3: Resolve row track sizes (distribute to auto tracks if explicit height)
    let row_params = crate::track_sizing::TrackSizingParams {
        axis_tracks: &inputs.row_tracks,
        available_size: inputs.available_height,
        items,
        placements: &placements,
        axis: GridAxis::Row,
        distribute_auto: inputs.has_explicit_height,
    };
    let row_sizes = resolve_track_sizes(&row_params)?;

    // Step 4: Actual grid dimensions are now in the resolved track sizes
    let actual_col_count = col_sizes.base_sizes.len();
    let actual_row_count = row_sizes.base_sizes.len();

    // Step 5: Position items in their grid areas
    let positioning_ctx = ItemPositioningContext {
        col_sizes: &col_sizes,
        row_sizes: &row_sizes,
        col_gap: inputs.col_tracks.gap,
        row_gap: inputs.row_tracks.gap,
        justify_items: inputs.justify_items,
        align_items: inputs.align_items,
    };
    let placed_items = position_grid_items(items, &placements, &positioning_ctx);

    // Calculate total grid size using actual track counts (including implicit tracks)
    let total_width = inputs.col_tracks.gap.mul_add(
        (actual_col_count - 1) as f32,
        col_sizes.base_sizes.iter().sum::<f32>(),
    );

    let total_height = inputs.row_tracks.gap.mul_add(
        (actual_row_count - 1) as f32,
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

/// Parameters for positioning grid items.
struct ItemPositioningContext<'ctx> {
    col_sizes: &'ctx ResolvedTrackSizes,
    row_sizes: &'ctx ResolvedTrackSizes,
    col_gap: f32,
    row_gap: f32,
    justify_items: GridAlignment,
    align_items: GridAlignment,
}

/// Position grid items in their grid areas.
fn position_grid_items<NodeId: Clone>(
    items: &[GridItem<NodeId>],
    placements: &[GridArea],
    ctx: &ItemPositioningContext<'_>,
) -> Vec<GridPlacedItem<NodeId>> {
    items
        .iter()
        .zip(placements.iter())
        .map(|(item, area)| {
            let (x_pos, width) = calculate_item_position_and_size(
                area.col_start,
                area.col_end,
                ctx.col_sizes,
                ctx.col_gap,
                ctx.justify_items,
            );

            let (y_pos, height) = calculate_item_position_and_size(
                area.row_start,
                area.row_end,
                ctx.row_sizes,
                ctx.row_gap,
                ctx.align_items,
            );

            GridPlacedItem {
                node_id: item.node_id.clone(),
                x: x_pos,
                y: y_pos,
                width,
                height,
                area: *area,
            }
        })
        .collect()
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

    /// Test auto-fit with minmax(200px, 1fr) creating 3 columns.
    ///
    /// This test verifies the fix for the bug where items were placed using the
    /// unexpanded track count, causing all items to be placed in column 1,
    /// which then collapsed to 1 column instead of the expected 3 columns.
    ///
    /// # Panics
    /// Panics if layout computation fails or assertions fail.
    #[test]
    fn test_auto_fit_three_columns() {
        use crate::types::TrackRepeat;

        // Simulate: grid-template-columns: repeat(auto-fit, minmax(200px, 1fr))
        // With 800px available and 10px gap:
        // - 3 columns should fit: 3*200 + 2*10 = 620 < 800
        // - 4 columns would not: 4*200 + 3*10 = 830 > 800
        let repeat_track =
            GridTrackSize::MinMax(TrackBreadth::Length(200.0), TrackBreadth::Flex(1.0));

        let col_tracks = GridAxisTracks::with_auto_repeat(
            vec![], // No pre/post tracks
            10.0,   // gap
            TrackRepeat::AutoFit(vec![repeat_track]),
        );

        let row_tracks = GridAxisTracks::new(
            vec![GridTrack {
                size: GridTrackSize::Breadth(TrackBreadth::Auto),
                track_type: TrackListType::Explicit,
            }],
            10.0,
        );

        // Create 5 items (like the "Font Sizes" grid in the fixture)
        let items: Vec<GridItem<usize>> = (0..5).map(GridItem::new).collect();

        let inputs = GridContainerInputs::new(row_tracks, col_tracks, 800.0, 600.0);

        let result = layout_grid(&items, &inputs)
            .ok()
            .unwrap_or_else(|| GridLayoutResult {
                items: vec![],
                total_width: 0.0,
                total_height: 0.0,
                col_sizes: ResolvedTrackSizes::new(0),
                row_sizes: ResolvedTrackSizes::new(0),
            });

        // CRITICAL ASSERTION: Should create 3 columns, not 1
        assert_eq!(
            result.col_sizes.base_sizes.len(),
            3,
            "Expected 3 columns with 800px available, got {}",
            result.col_sizes.base_sizes.len()
        );

        // Items should be distributed across columns:
        // Row 1: items 0, 1, 2 (columns 1, 2, 3)
        // Row 2: items 3, 4 (columns 1, 2)
        assert_eq!(result.items.len(), 5);

        // Item 0 should be in column 1
        assert_eq!(result.items[0].area.col_start, 1);
        assert_eq!(result.items[0].area.col_end, 2);

        // Item 1 should be in column 2
        assert_eq!(result.items[1].area.col_start, 2);
        assert_eq!(result.items[1].area.col_end, 3);

        // Item 2 should be in column 3
        assert_eq!(result.items[2].area.col_start, 3);
        assert_eq!(result.items[2].area.col_end, 4);

        // Item 3 should be in column 1, row 2
        assert_eq!(result.items[3].area.col_start, 1);
        assert_eq!(result.items[3].area.col_end, 2);
        assert_eq!(result.items[3].area.row_start, 2);

        // Item 4 should be in column 2, row 2
        assert_eq!(result.items[4].area.col_start, 2);
        assert_eq!(result.items[4].area.col_end, 3);
        assert_eq!(result.items[4].area.row_start, 2);

        // Each column should have equal width (distribute 1fr evenly)
        // Total width = 800px, gap = 2 * 10px = 20px, available = 780px
        // Each column = 780 / 3 = 260px
        let expected_col_width = (800.0 - 20.0) / 3.0;
        for (idx, &col_width) in result.col_sizes.base_sizes.iter().enumerate() {
            assert!(
                (col_width - expected_col_width).abs() < 1.0,
                "Column {idx} width should be ~{expected_col_width}px, got {col_width}px"
            );
        }
    }
}
