/// Table layout module implementing CSS 2.2 table layout algorithms.
///
/// This module handles:
/// - Table structure (table, row, cell)
/// - Automatic table layout algorithm
/// - Fixed table layout algorithm
/// - Cell spanning (rowspan, colspan)
/// - Border collapsing
///
/// Spec: https://www.w3.org/TR/CSS22/tables.html
use crate::{BlockMarker, ConstrainedMarker, InlineMarker, SizeQuery, Subpixels};
use rewrite_core::{NodeId, ScopedDb};
use rewrite_css::{BorderCollapseQuery, CssKeyword, CssValue, DisplayQuery, TableLayoutQuery};

/// Represents a table grid structure.
#[derive(Debug, Clone)]
pub struct TableGrid {
    /// Number of columns in the table.
    pub column_count: usize,
    /// Number of rows in the table.
    pub row_count: usize,
    /// Column widths.
    pub column_widths: Vec<Subpixels>,
    /// Row heights.
    pub row_heights: Vec<Subpixels>,
    /// Grid cells (indexed by [row][column]).
    pub cells: Vec<Vec<Option<TableCell>>>,
}

/// Represents a table cell.
#[derive(Debug, Clone)]
pub struct TableCell {
    /// The node ID of the cell element.
    pub node: NodeId,
    /// Starting row index (0-based).
    pub row: usize,
    /// Starting column index (0-based).
    pub column: usize,
    /// Row span (number of rows).
    pub rowspan: usize,
    /// Column span (number of columns).
    pub colspan: usize,
    /// Intrinsic width of cell content.
    pub intrinsic_width: Subpixels,
    /// Intrinsic height of cell content.
    pub intrinsic_height: Subpixels,
}

/// Table layout algorithm mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableLayoutMode {
    /// Auto layout: column widths based on content.
    Auto,
    /// Fixed layout: column widths from first row.
    Fixed,
}

/// Get the table layout mode from the table-layout property.
pub fn get_table_layout_mode(scoped: &mut ScopedDb) -> TableLayoutMode {
    let table_layout = scoped.query::<TableLayoutQuery>();

    match table_layout {
        CssValue::Keyword(CssKeyword::Fixed) => TableLayoutMode::Fixed,
        CssValue::Keyword(CssKeyword::Auto) | _ => TableLayoutMode::Auto,
    }
}

/// Build the table grid structure from DOM.
///
/// This analyzes the table structure and creates a grid representation
/// accounting for rowspan and colspan.
pub fn build_table_grid(scoped: &mut ScopedDb) -> TableGrid {
    // Get table rows
    let rows = get_table_rows(scoped);
    let row_count = rows.len();

    if row_count == 0 {
        return TableGrid {
            column_count: 0,
            row_count: 0,
            column_widths: vec![],
            row_heights: vec![],
            cells: vec![],
        };
    }

    // Determine column count by scanning all cells
    let mut column_count = 0;
    for &row_node in &rows {
        let cells = get_row_cells(scoped, row_node);
        let mut col_index = 0;
        for &cell_node in &cells {
            let colspan = get_colspan(scoped, cell_node);
            col_index += colspan;
        }
        column_count = column_count.max(col_index);
    }

    // Initialize grid
    let mut cells = vec![vec![None; column_count]; row_count];

    // Fill grid with cells, accounting for spans
    for (row_idx, &row_node) in rows.iter().enumerate() {
        let row_cells = get_row_cells(scoped, row_node);
        let mut col_idx = 0;

        for &cell_node in &row_cells {
            // Skip occupied cells (from previous rowspans)
            while col_idx < column_count && cells[row_idx][col_idx].is_some() {
                col_idx += 1;
            }

            if col_idx >= column_count {
                break;
            }

            let rowspan = get_rowspan(scoped, cell_node);
            let colspan = get_colspan(scoped, cell_node);

            let intrinsic_width =
                scoped.node_query::<SizeQuery<InlineMarker, crate::IntrinsicMarker>>(cell_node);
            let intrinsic_height =
                scoped.node_query::<SizeQuery<BlockMarker, crate::IntrinsicMarker>>(cell_node);

            let cell = TableCell {
                node: cell_node,
                row: row_idx,
                column: col_idx,
                rowspan,
                colspan,
                intrinsic_width,
                intrinsic_height,
            };

            // Fill grid cells for span
            for r in 0..rowspan.min(row_count - row_idx) {
                for c in 0..colspan.min(column_count - col_idx) {
                    cells[row_idx + r][col_idx + c] = Some(cell.clone());
                }
            }

            col_idx += colspan;
        }
    }

    TableGrid {
        column_count,
        row_count,
        column_widths: vec![0; column_count],
        row_heights: vec![0; row_count],
        cells,
    }
}

/// Compute column widths using the automatic table layout algorithm.
///
/// The algorithm:
/// 1. Calculate minimum and maximum widths for each column
/// 2. Distribute available width proportionally based on content
pub fn compute_auto_column_widths(grid: &mut TableGrid, available_width: Subpixels) {
    if grid.column_count == 0 {
        return;
    }

    // Calculate minimum and preferred widths for each column
    let mut min_widths = vec![0; grid.column_count];
    let mut max_widths = vec![0; grid.column_count];

    for row in &grid.cells {
        for cell_opt in row {
            if let Some(cell) = cell_opt {
                // Only process cells in their starting column
                if cell.column < grid.column_count {
                    if cell.colspan == 1 {
                        // Single column cell
                        min_widths[cell.column] = min_widths[cell.column].max(cell.intrinsic_width);
                        max_widths[cell.column] = max_widths[cell.column].max(cell.intrinsic_width);
                    } else {
                        // Multi-column cell: distribute width across columns
                        let width_per_col = cell.intrinsic_width / cell.colspan as i32;
                        for c in 0..cell.colspan {
                            if cell.column + c < grid.column_count {
                                min_widths[cell.column + c] =
                                    min_widths[cell.column + c].max(width_per_col);
                                max_widths[cell.column + c] =
                                    max_widths[cell.column + c].max(width_per_col);
                            }
                        }
                    }
                }
            }
        }
    }

    let total_min: Subpixels = min_widths.iter().sum();
    let total_max: Subpixels = max_widths.iter().sum();

    if available_width <= total_min {
        // Not enough space, use minimum widths
        grid.column_widths = min_widths;
    } else if available_width >= total_max {
        // Plenty of space, use maximum widths
        grid.column_widths = max_widths;
    } else {
        // Distribute available width proportionally
        let extra_space = available_width - total_min;
        let distributable = total_max - total_min;

        if distributable > 0 {
            for (i, (&min, &max)) in min_widths.iter().zip(&max_widths).enumerate() {
                let column_range = max - min;
                let column_share =
                    (extra_space as f32 * column_range as f32 / distributable as f32) as Subpixels;
                grid.column_widths[i] = min + column_share;
            }
        } else {
            grid.column_widths = min_widths;
        }
    }
}

/// Compute column widths using the fixed table layout algorithm.
///
/// The algorithm:
/// 1. Use widths from the first row
/// 2. If no widths specified, divide equally
pub fn compute_fixed_column_widths(grid: &mut TableGrid, available_width: Subpixels) {
    if grid.column_count == 0 {
        return;
    }

    // Get widths from first row cells
    let mut specified_widths = vec![None; grid.column_count];
    let mut specified_count = 0;

    if !grid.cells.is_empty() {
        for (col_idx, cell_opt) in grid.cells[0].iter().enumerate() {
            if let Some(cell) = cell_opt {
                if cell.row == 0 && cell.column == col_idx {
                    // This is the primary cell for this column
                    let width = get_explicit_width(cell);
                    if let Some(w) = width {
                        specified_widths[col_idx] = Some(w);
                        specified_count += 1;
                    }
                }
            }
        }
    }

    if specified_count == grid.column_count {
        // All widths specified
        grid.column_widths = specified_widths.iter().map(|w| w.unwrap_or(0)).collect();
    } else {
        // Some widths unspecified: distribute remaining space equally
        let specified_total: Subpixels = specified_widths.iter().filter_map(|&w| w).sum();
        let unspecified_count = grid.column_count - specified_count;

        let remaining = available_width - specified_total;
        let width_per_unspecified = if unspecified_count > 0 {
            remaining / unspecified_count as i32
        } else {
            0
        };

        grid.column_widths = specified_widths
            .iter()
            .map(|&w| w.unwrap_or(width_per_unspecified))
            .collect();
    }
}

/// Compute row heights based on cell content.
pub fn compute_row_heights(grid: &mut TableGrid) {
    for row_idx in 0..grid.row_count {
        let mut max_height = 0;

        for col_idx in 0..grid.column_count {
            if let Some(cell) = &grid.cells[row_idx][col_idx] {
                // Only process cells starting in this row
                if cell.row == row_idx {
                    if cell.rowspan == 1 {
                        max_height = max_height.max(cell.intrinsic_height);
                    } else {
                        // Multi-row cell: distribute height
                        let height_per_row = cell.intrinsic_height / cell.rowspan as i32;
                        max_height = max_height.max(height_per_row);
                    }
                }
            }
        }

        grid.row_heights[row_idx] = max_height;
    }
}

/// Layout the table grid.
pub fn layout_table(scoped: &mut ScopedDb, available_width: Subpixels) -> TableGrid {
    let mut grid = build_table_grid(scoped);
    let layout_mode = get_table_layout_mode(scoped);

    match layout_mode {
        TableLayoutMode::Auto => {
            compute_auto_column_widths(&mut grid, available_width);
        }
        TableLayoutMode::Fixed => {
            compute_fixed_column_widths(&mut grid, available_width);
        }
    }

    compute_row_heights(&mut grid);

    grid
}

/// Get cell position within the table grid.
pub fn get_cell_position(grid: &TableGrid, cell_node: NodeId) -> Option<(Subpixels, Subpixels)> {
    for (row_idx, row) in grid.cells.iter().enumerate() {
        for (col_idx, cell_opt) in row.iter().enumerate() {
            if let Some(cell) = cell_opt {
                if cell.node == cell_node && cell.row == row_idx && cell.column == col_idx {
                    // Calculate position
                    let inline_offset: Subpixels = grid.column_widths[..col_idx].iter().sum();
                    let block_offset: Subpixels = grid.row_heights[..row_idx].iter().sum();
                    return Some((inline_offset, block_offset));
                }
            }
        }
    }
    None
}

/// Calculate the total table size.
pub fn calculate_table_size(grid: &TableGrid) -> (Subpixels, Subpixels) {
    let width: Subpixels = grid.column_widths.iter().sum();
    let height: Subpixels = grid.row_heights.iter().sum();
    (width, height)
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Get all row elements in a table.
fn get_table_rows(scoped: &mut ScopedDb) -> Vec<NodeId> {
    let mut rows = Vec::new();
    let children = scoped
        .db()
        .resolve_relationship(scoped.node(), rewrite_core::Relationship::Children);

    for &child in &children {
        let display = scoped.node_query::<DisplayQuery>(child);
        match display {
            CssValue::Keyword(CssKeyword::TableRow) => {
                rows.push(child);
            }
            // Table row groups (tbody, thead, tfoot) would go here when those keywords are added
            // For now, we only support direct table-row children
            _ => {}
        }
    }

    rows
}

/// Get all cell elements in a row.
fn get_row_cells(scoped: &mut ScopedDb, row_node: NodeId) -> Vec<NodeId> {
    let mut cells = Vec::new();
    let children = scoped
        .db()
        .resolve_relationship(row_node, rewrite_core::Relationship::Children);

    for &child in &children {
        let display = scoped.node_query::<DisplayQuery>(child);
        if matches!(display, CssValue::Keyword(CssKeyword::TableCell)) {
            cells.push(child);
        }
    }

    cells
}

/// Get rowspan attribute value (default 1).
fn get_rowspan(scoped: &mut ScopedDb, cell_node: NodeId) -> usize {
    // TODO: Query rowspan attribute from HTML
    // For now, return default
    1
}

/// Get colspan attribute value (default 1).
fn get_colspan(scoped: &mut ScopedDb, cell_node: NodeId) -> usize {
    // TODO: Query colspan attribute from HTML
    // For now, return default
    1
}

/// Get explicit width from cell (if specified).
fn get_explicit_width(cell: &TableCell) -> Option<Subpixels> {
    // TODO: Query width property from cell
    // For now, return None (use intrinsic width)
    None
}

/// Check if border-collapse is enabled.
pub fn is_border_collapse(scoped: &mut ScopedDb) -> bool {
    let border_collapse = scoped.query::<BorderCollapseQuery>();
    matches!(border_collapse, CssValue::Keyword(CssKeyword::Collapse))
}

/// Calculate collapsed border width between two cells.
///
/// When border-collapse is enabled, adjacent borders are collapsed.
/// The wider border wins.
pub fn calculate_collapsed_border(cell1_border: Subpixels, cell2_border: Subpixels) -> Subpixels {
    cell1_border.max(cell2_border)
}
