//! Grid container and item type definitions.
//!
//! Spec: CSS Grid Layout Module Level 2
//! <https://www.w3.org/TR/css-grid-2/>

/// Represents a track size in the grid.
///
/// Spec: §7.2.1 Track Sizing Functions
/// <https://www.w3.org/TR/css-grid-2/#track-sizing>
#[derive(Debug, Clone, PartialEq)]
pub enum TrackBreadth {
    /// Length in pixels
    Length(f32),
    /// Percentage of available space
    Percentage(f32),
    /// Flex factor (fr units)
    Flex(f32),
    /// Minimum content size
    MinContent,
    /// Maximum content size
    MaxContent,
    /// Automatic sizing
    Auto,
}

impl TrackBreadth {
    /// Check if this breadth is intrinsic (depends on content).
    pub fn is_intrinsic(&self) -> bool {
        matches!(self, Self::MinContent | Self::MaxContent | Self::Auto)
    }

    /// Check if this breadth is flexible (uses fr units).
    pub fn is_flexible(&self) -> bool {
        matches!(self, Self::Flex(_))
    }

    /// Get the flex factor, or 0.0 if not flexible.
    pub fn flex_factor(&self) -> f32 {
        match self {
            Self::Flex(factor) => *factor,
            _ => 0.0,
        }
    }
}

/// Track sizing function.
///
/// Spec: §7.2.1 Track Sizing Functions
#[derive(Debug, Clone, PartialEq)]
pub enum GridTrackSize {
    /// Fixed size
    Breadth(TrackBreadth),
    /// minmax(min, max)
    MinMax(TrackBreadth, TrackBreadth),
    /// fit-content(limit)
    FitContent(TrackBreadth),
}

impl GridTrackSize {
    /// Get the minimum breadth for this track size.
    pub fn min_breadth(&self) -> &TrackBreadth {
        match self {
            Self::Breadth(breadth) => breadth,
            Self::MinMax(min, _) => min,
            Self::FitContent(_) => &TrackBreadth::Auto,
        }
    }

    /// Get the maximum breadth for this track size.
    pub fn max_breadth(&self) -> &TrackBreadth {
        match self {
            Self::Breadth(breadth) => breadth,
            Self::MinMax(_, max) => max,
            Self::FitContent(limit) => limit,
        }
    }
}

/// Repeat pattern for track lists.
///
/// Spec: §7.2.3 Repeating Rows and Columns
#[derive(Debug, Clone, PartialEq)]
pub enum TrackRepeat {
    /// repeat(count, track-list)
    Count(usize, Vec<GridTrackSize>),
    /// repeat(auto-fill, track-list)
    AutoFill(Vec<GridTrackSize>),
    /// repeat(auto-fit, track-list)
    AutoFit(Vec<GridTrackSize>),
}

/// Type of track list (explicit or implicit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackListType {
    /// Explicit tracks from grid-template-*
    Explicit,
    /// Implicit tracks from grid-auto-*
    Implicit,
}

/// A track in the grid with its resolved size.
#[derive(Debug, Clone)]
pub struct GridTrack {
    /// Track sizing function
    pub size: GridTrackSize,
    /// Track type
    pub track_type: TrackListType,
}

/// Auto-placement algorithm direction.
///
/// Spec: §8.5 Grid Item Placement Algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GridAutoFlow {
    /// Place items row by row
    #[default]
    Row,
    /// Place items column by column
    Column,
    /// Pack items densely (try to fill holes)
    RowDense,
    /// Pack items densely in columns
    ColumnDense,
}

/// Represents a grid item with its style and content information.
#[derive(Debug, Clone)]
pub struct GridItem<NodeId = usize> {
    /// Node identifier (generic to support different node ID types)
    pub node_id: NodeId,
    /// Explicit row start position (if specified)
    pub row_start: Option<i32>,
    /// Explicit row end position (if specified)
    pub row_end: Option<i32>,
    /// Explicit column start position (if specified)
    pub col_start: Option<i32>,
    /// Explicit column end position (if specified)
    pub col_end: Option<i32>,
    /// Minimum content width
    pub min_content_width: f32,
    /// Maximum content width
    pub max_content_width: f32,
    /// Minimum content height
    pub min_content_height: f32,
    /// Maximum content height
    pub max_content_height: f32,
}

impl<NodeId> GridItem<NodeId> {
    /// Create a new grid item with the given node ID.
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            row_start: None,
            row_end: None,
            col_start: None,
            col_end: None,
            min_content_width: 0.0,
            max_content_width: 0.0,
            min_content_height: 0.0,
            max_content_height: 0.0,
        }
    }

    /// Check if this item has explicit row placement.
    pub fn has_explicit_row_placement(&self) -> bool {
        self.row_start.is_some() || self.row_end.is_some()
    }

    /// Check if this item has explicit column placement.
    pub fn has_explicit_col_placement(&self) -> bool {
        self.col_start.is_some() || self.col_end.is_some()
    }
}

/// Collect grid items from a container's children.
///
/// This is a placeholder that creates basic grid items.
/// In the full implementation, this would query the DOM for grid item properties.
///
/// # Errors
/// This function is infallible in the MVP implementation.
pub fn collect_grid_items(children: &[usize]) -> Result<Vec<GridItem>, String> {
    // MVP: Create basic grid items without explicit placement
    let items = children
        .iter()
        .map(|&node_id| GridItem::new(node_id))
        .collect();
    Ok(items)
}
