//! Helper functions for track sizing.

use crate::placement::GridArea;
use crate::types::GridItem;

use super::GridAxis;

/// Calculate the auto size for a track based on its content.
///
/// This is a simplified implementation that estimates content size.
/// In a full implementation, this would measure actual content.
pub fn calculate_auto_track_size<NodeId>(
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

            if matches!(axis, GridAxis::Row) {
                tracing::debug!(
                    "  Track row {}: item area={:?}, item_height={:.1}px, span={}, size_per_track={:.1}px, max_size was {:.1}px",
                    track_idx,
                    area,
                    item_size,
                    span,
                    size_per_track,
                    max_size
                );
            }

            max_size = max_size.max(size_per_track);
        }
    }

    if matches!(axis, GridAxis::Row) {
        tracing::info!(
            "calculate_auto_track_size: ROW track {} = {:.1}px",
            track_idx,
            max_size
        );
    }

    max_size
}
