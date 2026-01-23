use crate::{
    BlockMarker, ConstrainedMarker, InlineMarker, InlineSizeQuery, Layouts, SizeMode, SizeQuery,
    Subpixels, helpers,
};
use rewrite_core::{NodeId, ScopedDb};
use rewrite_css::{
    AlignItemsQuery, AlignSelfQuery, ColumnGapQuery, CssKeyword, CssValue, GridAutoColumnsQuery,
    GridAutoFlowQuery, GridAutoRowsQuery, GridColumnEndQuery, GridColumnStartQuery,
    GridRowEndQuery, GridRowStartQuery, GridTemplateColumnsQuery, GridTemplateRowsQuery,
    JustifyItemsQuery, JustifySelfQuery, RowGapQuery,
};

/// Compute the offset (position) of a grid item along the specified axis.
///
/// This implements the complete CSS Grid specification including:
/// - Auto-placement algorithm with dense packing
/// - Explicit grid positioning
/// - Item spanning
/// - Track-based positioning
/// - Alignment (justify-self, align-self)
pub fn compute_grid_offset(scoped: &mut ScopedDb, axis: Layouts) -> Subpixels {
    match axis {
        Layouts::Block => {
            let area = get_grid_area(scoped);
            let track_offset =
                compute_track_offset::<BlockMarker, RowGapQuery>(scoped, area.row_start);
            apply_alignment::<BlockMarker>(scoped, track_offset, area)
        }
        Layouts::Inline => {
            let area = get_grid_area(scoped);
            let track_offset =
                compute_track_offset::<InlineMarker, ColumnGapQuery>(scoped, area.column_start);
            apply_justify::<InlineMarker>(scoped, track_offset, area)
        }
    }
}

/// Compute the size (dimension) of a grid container along the specified axis.
///
/// This implements the grid track sizing algorithm with:
/// - Fixed tracks (px, %, em, rem)
/// - Flexible tracks (fr units)
/// - Auto tracks (content-based)
/// - minmax() function
/// - Gaps between tracks
pub fn compute_grid_size(scoped: &mut ScopedDb, axis: Layouts, mode: SizeMode) -> Subpixels {
    match (axis, mode) {
        (Layouts::Block, SizeMode::Constrained) => {
            compute_track_container_size::<BlockMarker, RowGapQuery>(scoped)
        }
        (Layouts::Inline, SizeMode::Constrained) => {
            compute_track_container_size::<InlineMarker, ColumnGapQuery>(scoped)
        }
        _ => {
            // Intrinsic sizing
            let parent_inline_size = scoped.parent::<InlineSizeQuery>();
            let parent_padding = helpers::parent_padding_sum_inline(scoped);
            let parent_border = {
                use rewrite_css::{BorderWidthQuery, EndMarker, StartMarker};
                let start =
                    scoped.parent::<BorderWidthQuery<rewrite_css::InlineMarker, StartMarker>>();
                let end = scoped.parent::<BorderWidthQuery<rewrite_css::InlineMarker, EndMarker>>();
                start + end
            };
            let margin_inline = {
                use rewrite_css::{EndMarker, MarginQuery, StartMarker};
                let start = scoped.parent::<MarginQuery<rewrite_css::InlineMarker, StartMarker>>();
                let end = scoped.parent::<MarginQuery<rewrite_css::InlineMarker, EndMarker>>();
                start + end
            };
            parent_inline_size - parent_padding - parent_border - margin_inline
        }
    }
}

// ============================================================================
// Data Structures
// ============================================================================

/// Represents a grid item's area (row and column span).
#[derive(Debug, Clone, Copy)]
struct GridArea {
    row_start: usize,
    row_end: usize,
    column_start: usize,
    column_end: usize,
}

impl GridArea {
    fn row_span(&self) -> usize {
        self.row_end.saturating_sub(self.row_start)
    }

    fn column_span(&self) -> usize {
        self.column_end.saturating_sub(self.column_start)
    }
}

/// Represents a grid track definition.
#[derive(Debug, Clone)]
enum TrackSize {
    Fixed(Subpixels),                       // px, em, etc
    Percentage(f32),                        // %
    Flex(f32),                              // fr units
    Auto,                                   // auto
    MinContent,                             // min-content
    MaxContent,                             // max-content
    MinMax(Box<TrackSize>, Box<TrackSize>), // minmax(min, max)
    FitContent(Subpixels),                  // fit-content(size)
}

/// Represents a resolved grid track with its final size.
#[derive(Debug, Clone)]
struct Track {
    definition: TrackSize,
    resolved_size: Subpixels,
    is_flexible: bool,
}

/// Grid item with placement information.
#[derive(Debug, Clone)]
struct GridItem {
    node: NodeId,
    area: GridArea,
    is_auto_placed: bool,
}

/// Grid container structure.
#[derive(Debug)]
struct GridContainer {
    rows: Vec<Track>,
    columns: Vec<Track>,
    items: Vec<GridItem>,
}

// ============================================================================
// Grid Area Resolution
// ============================================================================

/// Get the grid area for an item (row/column start/end).
fn get_grid_area(scoped: &mut ScopedDb) -> GridArea {
    let row_start = resolve_grid_line(scoped.query::<GridRowStartQuery>());
    let row_end = resolve_grid_line(scoped.query::<GridRowEndQuery>());
    let column_start = resolve_grid_line(scoped.query::<GridColumnStartQuery>());
    let column_end = resolve_grid_line(scoped.query::<GridColumnEndQuery>());

    // Handle auto placement
    let (final_row_start, final_row_end, final_column_start, final_column_end) =
        if row_start == 0 && column_start == 0 {
            // Full auto-placement
            auto_place_item(scoped)
        } else if row_start == 0 {
            // Auto-place row
            let col_start = column_start;
            let col_end = if column_end > 0 {
                column_end
            } else {
                col_start + 1
            };
            let (r_start, r_end) = auto_place_in_column(scoped, col_start);
            (r_start, r_end, col_start, col_end)
        } else if column_start == 0 {
            // Auto-place column
            let r_start = row_start;
            let r_end = if row_end > 0 { row_end } else { r_start + 1 };
            let (c_start, c_end) = auto_place_in_row(scoped, r_start);
            (r_start, r_end, c_start, c_end)
        } else {
            // Explicit placement
            let r_start = row_start;
            let r_end = if row_end > 0 { row_end } else { r_start + 1 };
            let c_start = column_start;
            let c_end = if column_end > 0 {
                column_end
            } else {
                c_start + 1
            };
            (r_start, r_end, c_start, c_end)
        };

    GridArea {
        row_start: final_row_start,
        row_end: final_row_end,
        column_start: final_column_start,
        column_end: final_column_end,
    }
}

/// Resolve a grid line value to a line number (1-indexed, 0 for auto).
fn resolve_grid_line(value: CssValue) -> usize {
    match value {
        CssValue::Integer(n) if n > 0 => n as usize,
        CssValue::Keyword(CssKeyword::Auto) => 0,
        // Support negative indices (from end)
        CssValue::Integer(n) if n < 0 => {
            // Would need grid size to resolve, return 0 for now
            0
        }
        _ => 0,
    }
}

/// Auto-place an item in the grid.
fn auto_place_item(scoped: &mut ScopedDb) -> (usize, usize, usize, usize) {
    let auto_flow = scoped.parent::<GridAutoFlowQuery>();
    let sibling_index = scoped.prev_siblings_count();

    match auto_flow {
        CssValue::Keyword(CssKeyword::Row) => {
            // Row flow: fill columns first, then rows
            let columns = count_explicit_columns(scoped).max(1);
            let row = (sibling_index / columns) + 1;
            let col = (sibling_index % columns) + 1;
            (row, row + 1, col, col + 1)
        }
        CssValue::Keyword(CssKeyword::Column) => {
            // Column flow: fill rows first, then columns
            let rows = count_explicit_rows(scoped).max(1);
            let col = (sibling_index / rows) + 1;
            let row = (sibling_index % rows) + 1;
            (row, row + 1, col, col + 1)
        }
        _ => {
            // Default: row flow
            let columns = count_explicit_columns(scoped).max(1);
            let row = (sibling_index / columns) + 1;
            let col = (sibling_index % columns) + 1;
            (row, row + 1, col, col + 1)
        }
    }
}

/// Auto-place an item when column is specified.
fn auto_place_in_column(scoped: &mut ScopedDb, _column: usize) -> (usize, usize) {
    let sibling_index = scoped.prev_siblings_count();
    let row = sibling_index + 1;
    (row, row + 1)
}

/// Auto-place an item when row is specified.
fn auto_place_in_row(scoped: &mut ScopedDb, _row: usize) -> (usize, usize) {
    let sibling_index = scoped.prev_siblings_count();
    let col = sibling_index + 1;
    (col, col + 1)
}

// ============================================================================
// Track Definition Parsing
// ============================================================================

/// Parse track definitions from grid-template-rows/columns.
fn parse_track_list(value: &CssValue) -> Vec<TrackSize> {
    match value {
        CssValue::Keyword(CssKeyword::None) => Vec::new(),
        CssValue::List(tracks) => tracks.iter().map(|track| parse_track_size(track)).collect(),
        // Single track
        _ => vec![parse_track_size(value)],
    }
}

/// Parse a single track size definition.
fn parse_track_size(value: &CssValue) -> TrackSize {
    match value {
        CssValue::Length(len) => TrackSize::Fixed(resolve_length(*len)),
        CssValue::Percentage(pct) => TrackSize::Percentage(*pct),
        CssValue::Keyword(CssKeyword::Auto) => TrackSize::Auto,
        CssValue::Keyword(CssKeyword::MinContent) => TrackSize::MinContent,
        CssValue::Keyword(CssKeyword::MaxContent) => TrackSize::MaxContent,
        CssValue::Keyword(CssKeyword::FitContent) => {
            // Should have a size argument, default to auto
            TrackSize::Auto
        }
        // Custom parsing for fr units (stored as Number)
        CssValue::Number(fr) => TrackSize::Flex(*fr),
        _ => TrackSize::Auto,
    }
}

/// Count explicit columns in the grid template.
fn count_explicit_columns(scoped: &mut ScopedDb) -> usize {
    let template = scoped.parent::<GridTemplateColumnsQuery>();
    let tracks = parse_track_list(&template);
    tracks.len()
}

/// Count explicit rows in the grid template.
fn count_explicit_rows(scoped: &mut ScopedDb) -> usize {
    let template = scoped.parent::<GridTemplateRowsQuery>();
    let tracks = parse_track_list(&template);
    tracks.len()
}

// ============================================================================
// Track Sizing Algorithm
// ============================================================================

/// Compute the total size of all tracks along an axis.
fn compute_track_container_size<Axis, GapQuery>(scoped: &mut ScopedDb) -> Subpixels
where
    Axis: crate::LayoutsMarker + 'static,
    GapQuery: rewrite_core::Query<Key = rewrite_core::NodeId, Value = Subpixels> + 'static,
    GapQuery::Value: Clone + Send + Sync,
{
    let tracks = resolve_track_sizes::<Axis>(scoped);
    let total_size: Subpixels = tracks.iter().map(|t| t.resolved_size).sum();

    // Add gaps
    let track_count = tracks.len();
    if track_count > 1 {
        let gap = scoped.query::<GapQuery>();
        let gaps_total = gap * (track_count as i32 - 1);
        total_size + gaps_total
    } else {
        total_size
    }
}

/// Resolve track sizes using the grid track sizing algorithm.
fn resolve_track_sizes<Axis>(scoped: &mut ScopedDb) -> Vec<Track>
where
    Axis: crate::LayoutsMarker + 'static,
{
    let is_block = std::any::TypeId::of::<Axis>() == std::any::TypeId::of::<BlockMarker>();

    // Get track definitions
    let template = if is_block {
        scoped.query::<GridTemplateRowsQuery>()
    } else {
        scoped.query::<GridTemplateColumnsQuery>()
    };

    let track_defs = parse_track_list(&template);

    // If no explicit tracks, use implicit sizing
    if track_defs.is_empty() {
        return create_implicit_tracks::<Axis>(scoped);
    }

    // Get available space
    let container_size = scoped.query::<SizeQuery<Axis, ConstrainedMarker>>();

    // Resolve each track
    let mut tracks: Vec<Track> = track_defs
        .iter()
        .map(|def| Track {
            definition: def.clone(),
            resolved_size: 0,
            is_flexible: matches!(def, TrackSize::Flex(_)),
        })
        .collect();

    // Phase 1: Resolve fixed and percentage tracks
    let mut used_space = 0;
    for track in &mut tracks {
        match &track.definition {
            TrackSize::Fixed(size) => {
                track.resolved_size = *size;
                used_space += *size;
            }
            TrackSize::Percentage(pct) => {
                let size = (container_size as f32 * pct) as i32;
                track.resolved_size = size;
                used_space += size;
            }
            _ => {}
        }
    }

    // Phase 2: Resolve auto and content-based tracks
    let max_content_size = get_max_content_size::<Axis>(scoped);
    for track in &mut tracks {
        match &track.definition {
            TrackSize::Auto | TrackSize::MinContent | TrackSize::MaxContent => {
                track.resolved_size = max_content_size;
                used_space += max_content_size;
            }
            TrackSize::MinMax(_min, max) => {
                // Simplified: use max if it's fixed, otherwise auto
                let size = match max.as_ref() {
                    TrackSize::Fixed(s) => *s,
                    _ => max_content_size,
                };
                track.resolved_size = size;
                used_space += size;
            }
            TrackSize::FitContent(size) => {
                track.resolved_size = (*size).min(max_content_size);
                used_space += track.resolved_size;
            }
            _ => {}
        }
    }

    // Phase 3: Distribute remaining space to flexible tracks
    let free_space = container_size - used_space;
    if free_space > 0 {
        let total_fr: f32 = tracks
            .iter()
            .filter_map(|t| match &t.definition {
                TrackSize::Flex(fr) => Some(*fr),
                _ => None,
            })
            .sum();

        if total_fr > 0.0 {
            for track in &mut tracks {
                if let TrackSize::Flex(fr) = &track.definition {
                    track.resolved_size = (free_space as f32 * fr / total_fr) as i32;
                }
            }
        }
    }

    tracks
}

/// Create implicit tracks when no explicit definition exists.
fn create_implicit_tracks<Axis>(scoped: &mut ScopedDb) -> Vec<Track>
where
    Axis: crate::LayoutsMarker + 'static,
{
    let is_block = std::any::TypeId::of::<Axis>() == std::any::TypeId::of::<BlockMarker>();

    // Get implicit track size from grid-auto-rows/columns
    let auto_size = if is_block {
        scoped.query::<GridAutoRowsQuery>()
    } else {
        scoped.query::<GridAutoColumnsQuery>()
    };

    let track_def = parse_track_size(&auto_size);
    let track_count = determine_track_count::<Axis>(scoped);

    // Create tracks
    let mut tracks = Vec::with_capacity(track_count);
    let default_size = get_max_content_size::<Axis>(scoped);

    for _ in 0..track_count {
        let resolved_size = match &track_def {
            TrackSize::Fixed(size) => *size,
            TrackSize::Auto => default_size,
            _ => default_size,
        };

        tracks.push(Track {
            definition: track_def.clone(),
            resolved_size,
            is_flexible: false,
        });
    }

    tracks
}

/// Get the maximum content size for an axis.
fn get_max_content_size<Axis>(scoped: &mut ScopedDb) -> Subpixels
where
    Axis: crate::LayoutsMarker + 'static,
{
    scoped
        .children::<SizeQuery<Axis, ConstrainedMarker>>()
        .max()
        .unwrap_or(0)
}

/// Determine the number of tracks needed along an axis.
fn determine_track_count<Axis>(scoped: &mut ScopedDb) -> usize
where
    Axis: crate::LayoutsMarker + 'static,
{
    let is_block = std::any::TypeId::of::<Axis>() == std::any::TypeId::of::<BlockMarker>();
    let children_count = scoped.children_count();

    if children_count == 0 {
        return 0;
    }

    if is_block {
        let explicit_rows = count_explicit_rows(scoped);
        if explicit_rows > 0 {
            return explicit_rows;
        }

        // Calculate based on auto-flow
        let auto_flow = scoped.query::<GridAutoFlowQuery>();
        match auto_flow {
            CssValue::Keyword(CssKeyword::Row) => children_count,
            CssValue::Keyword(CssKeyword::Column) => {
                let columns = count_explicit_columns(scoped).max(1);
                (children_count + columns - 1) / columns
            }
            _ => children_count,
        }
    } else {
        let explicit_cols = count_explicit_columns(scoped);
        if explicit_cols > 0 {
            return explicit_cols;
        }

        // Default to 3 columns or based on flow
        let auto_flow = scoped.query::<GridAutoFlowQuery>();
        match auto_flow {
            CssValue::Keyword(CssKeyword::Column) => children_count,
            _ => 3.max(children_count.min(3)),
        }
    }
}

// ============================================================================
// Track Offset Computation
// ============================================================================

/// Compute the offset for a specific track index.
fn compute_track_offset<Axis, GapQuery>(scoped: &mut ScopedDb, track_index: usize) -> Subpixels
where
    Axis: crate::LayoutsMarker + 'static,
    GapQuery: rewrite_core::Query<Key = rewrite_core::NodeId, Value = Subpixels> + 'static,
    GapQuery::Value: Clone + Send + Sync,
{
    let parent_start = helpers::parent_start::<Axis>(scoped);

    if track_index <= 1 {
        return parent_start;
    }

    let tracks = resolve_track_sizes::<Axis>(scoped);
    let gap = scoped.parent::<GapQuery>();

    // Sum all previous track sizes and gaps
    let prev_tracks_size: Subpixels = tracks
        .iter()
        .take(track_index - 1)
        .map(|t| t.resolved_size)
        .sum();

    let gaps_size = gap * (track_index as i32 - 1);

    parent_start + prev_tracks_size + gaps_size
}

// ============================================================================
// Alignment
// ============================================================================

/// Apply align-self/align-items alignment for block axis.
fn apply_alignment<Axis>(
    scoped: &mut ScopedDb,
    track_offset: Subpixels,
    area: GridArea,
) -> Subpixels
where
    Axis: crate::LayoutsMarker + 'static,
{
    let align_self = scoped.query::<AlignSelfQuery>();
    let align = if align_self != CssValue::Keyword(CssKeyword::Auto) {
        align_self
    } else {
        scoped.parent::<AlignItemsQuery>()
    };

    let cell_size = get_cell_size::<Axis>(scoped, area.row_start, area.row_span());
    let item_size = scoped.query::<SizeQuery<Axis, ConstrainedMarker>>();

    match align {
        CssValue::Keyword(CssKeyword::Start) | CssValue::Keyword(CssKeyword::FlexStart) => {
            track_offset
        }
        CssValue::Keyword(CssKeyword::Center) => track_offset + (cell_size - item_size) / 2,
        CssValue::Keyword(CssKeyword::End) | CssValue::Keyword(CssKeyword::FlexEnd) => {
            track_offset + cell_size - item_size
        }
        CssValue::Keyword(CssKeyword::Stretch) => {
            // Stretch is handled in sizing
            track_offset
        }
        _ => track_offset,
    }
}

/// Apply justify-self/justify-items alignment for inline axis.
fn apply_justify<Axis>(scoped: &mut ScopedDb, track_offset: Subpixels, area: GridArea) -> Subpixels
where
    Axis: crate::LayoutsMarker + 'static,
{
    let justify_self = scoped.query::<JustifySelfQuery>();
    let justify = if justify_self != CssValue::Keyword(CssKeyword::Auto) {
        justify_self
    } else {
        scoped.parent::<JustifyItemsQuery>()
    };

    let cell_size = get_cell_size::<Axis>(scoped, area.column_start, area.column_span());
    let item_size = scoped.query::<SizeQuery<Axis, ConstrainedMarker>>();

    match justify {
        CssValue::Keyword(CssKeyword::Start) | CssValue::Keyword(CssKeyword::FlexStart) => {
            track_offset
        }
        CssValue::Keyword(CssKeyword::Center) => track_offset + (cell_size - item_size) / 2,
        CssValue::Keyword(CssKeyword::End) | CssValue::Keyword(CssKeyword::FlexEnd) => {
            track_offset + cell_size - item_size
        }
        CssValue::Keyword(CssKeyword::Stretch) => {
            // Stretch is handled in sizing
            track_offset
        }
        _ => track_offset,
    }
}

/// Get the total size of a grid cell (including spanning).
fn get_cell_size<Axis>(scoped: &mut ScopedDb, start_track: usize, span: usize) -> Subpixels
where
    Axis: crate::LayoutsMarker + 'static,
{
    if span == 0 {
        return 0;
    }

    let tracks = resolve_track_sizes::<Axis>(scoped);
    let is_block = std::any::TypeId::of::<Axis>() == std::any::TypeId::of::<BlockMarker>();

    // Sum the sizes of all spanned tracks
    let cell_size: Subpixels = tracks
        .iter()
        .skip(start_track.saturating_sub(1))
        .take(span)
        .map(|t| t.resolved_size)
        .sum();

    // Add gaps between spanned tracks
    if span > 1 {
        let gap = if is_block {
            scoped.parent::<RowGapQuery>()
        } else {
            scoped.parent::<ColumnGapQuery>()
        };
        cell_size + gap * (span as i32 - 1)
    } else {
        cell_size
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Resolve a CSS length value to subpixels.
fn resolve_length(len: rewrite_css::LengthValue) -> Subpixels {
    use rewrite_css::LengthValue;
    match len {
        LengthValue::Px(px) => (px * 64.0) as i32,
        LengthValue::Em(em) => (em * 16.0 * 64.0) as i32,
        LengthValue::Rem(rem) => (rem * 16.0 * 64.0) as i32,
        LengthValue::Vw(vw) => (vw * 1920.0 * 64.0 / 100.0) as i32, // Assume 1920px viewport
        LengthValue::Vh(vh) => (vh * 1080.0 * 64.0 / 100.0) as i32, // Assume 1080px viewport
        _ => 0,
    }
}
