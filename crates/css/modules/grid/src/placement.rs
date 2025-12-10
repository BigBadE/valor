//! Grid item placement algorithm.
//!
//! Spec: §8 Placing Grid Items
//! <https://www.w3.org/TR/css-grid-2/#placement>

use crate::types::{GridAutoFlow, GridItem};

/// Position in the grid (1-indexed as per spec).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridPosition {
    /// Line number (1-indexed)
    pub line: i32,
}

impl GridPosition {
    /// Create a new grid position.
    pub fn new(line: i32) -> Self {
        Self { line }
    }

    /// Create an auto position (no explicit placement).
    pub fn auto() -> Self {
        Self { line: 0 }
    }

    /// Check if this is an auto position.
    pub fn is_auto(&self) -> bool {
        self.line == 0
    }
}

/// Area occupied by a grid item (row/column span).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridArea {
    /// Row start line (1-indexed)
    pub row_start: usize,
    /// Row end line (1-indexed, exclusive)
    pub row_end: usize,
    /// Column start line (1-indexed)
    pub col_start: usize,
    /// Column end line (1-indexed, exclusive)
    pub col_end: usize,
}

impl GridArea {
    /// Create a new grid area.
    pub fn new(row_start: usize, row_end: usize, col_start: usize, col_end: usize) -> Self {
        Self {
            row_start,
            row_end,
            col_start,
            col_end,
        }
    }

    /// Get the row span (number of rows occupied).
    pub fn row_span(&self) -> usize {
        self.row_end.saturating_sub(self.row_start)
    }

    /// Get the column span (number of columns occupied).
    pub fn col_span(&self) -> usize {
        self.col_end.saturating_sub(self.col_start)
    }

    /// Check if this area overlaps with another area.
    pub fn overlaps(&self, other: &Self) -> bool {
        self.row_start < other.row_end
            && self.row_end > other.row_start
            && self.col_start < other.col_end
            && self.col_end > other.col_start
    }
}

/// Place grid items according to the grid placement algorithm.
///
/// Spec: §8 Placing Grid Items
/// <https://www.w3.org/TR/css-grid-2/#placement>
///
/// This MVP implementation performs basic auto-placement in row-major order.
/// It does not yet handle:
/// - Explicit grid-row-start/end and grid-column-start/end properties
/// - Named grid lines
/// - Grid template areas
/// - Dense packing algorithm
///
/// # Errors
/// Returns an error if the placement algorithm fails (should not happen in MVP).
pub fn place_grid_items<NodeId>(
    items: &[GridItem<NodeId>],
    row_count: usize,
    col_count: usize,
    auto_flow: GridAutoFlow,
) -> Result<Vec<GridArea>, String> {
    let mut placements = Vec::with_capacity(items.len());
    let mut cursor_row = 1;
    let mut cursor_col = 1;

    // Simple auto-placement algorithm
    for item in items {
        // Check if item has explicit placement
        let area = if item.has_explicit_row_placement() || item.has_explicit_col_placement() {
            // Handle explicit placement
            let row_start = item.row_start.unwrap_or(cursor_row as i32).max(1) as usize;
            let col_start = item.col_start.unwrap_or(cursor_col as i32).max(1) as usize;
            let row_end = item
                .row_end
                .unwrap_or(row_start as i32 + 1)
                .max(row_start as i32 + 1) as usize;
            let col_end = item
                .col_end
                .unwrap_or(col_start as i32 + 1)
                .max(col_start as i32 + 1) as usize;

            GridArea::new(row_start, row_end, col_start, col_end)
        } else {
            // Auto-placement
            let area = GridArea::new(cursor_row, cursor_row + 1, cursor_col, cursor_col + 1);

            // Advance cursor based on auto-flow
            match auto_flow {
                GridAutoFlow::Row | GridAutoFlow::RowDense => {
                    cursor_col += 1;
                    if cursor_col > col_count {
                        cursor_col = 1;
                        cursor_row += 1;
                    }
                }
                GridAutoFlow::Column | GridAutoFlow::ColumnDense => {
                    cursor_row += 1;
                    if cursor_row > row_count {
                        cursor_row = 1;
                        cursor_col += 1;
                    }
                }
            }

            area
        };

        placements.push(area);
    }

    Ok(placements)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test grid area span calculation.
    ///
    /// # Panics
    /// Panics if assertions fail.
    #[test]
    fn test_grid_area_span() {
        let area = GridArea::new(1, 3, 2, 5);
        assert_eq!(area.row_span(), 2);
        assert_eq!(area.col_span(), 3);
    }

    /// Test grid area overlap detection.
    ///
    /// # Panics
    /// Panics if assertions fail.
    #[test]
    fn test_grid_area_overlaps() {
        let area1 = GridArea::new(1, 3, 1, 3);
        let area2 = GridArea::new(2, 4, 2, 4);
        let area3 = GridArea::new(4, 5, 4, 5);

        assert!(area1.overlaps(&area2));
        assert!(area2.overlaps(&area1));
        assert!(!area1.overlaps(&area3));
        assert!(!area3.overlaps(&area1));
    }

    /// Test basic grid item placement.
    ///
    /// # Panics
    /// Panics if placement fails or assertions fail.
    #[test]
    fn test_place_grid_items_basic() {
        let items = vec![GridItem::new(1), GridItem::new(2), GridItem::new(3)];

        let placements = place_grid_items(&items, 2, 2, GridAutoFlow::Row)
            .ok()
            .unwrap_or_default();

        assert_eq!(placements.len(), 3);
        assert_eq!(placements[0], GridArea::new(1, 2, 1, 2));
        assert_eq!(placements[1], GridArea::new(1, 2, 2, 3));
        assert_eq!(placements[2], GridArea::new(2, 3, 1, 2));
    }
}
