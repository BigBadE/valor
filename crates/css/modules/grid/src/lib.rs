//! CSS Grid Layout Module Level 2
//! Spec: <https://www.w3.org/TR/css-grid-2/>
//!
//! This module implements CSS Grid layout, a two-dimensional layout system
//! that lets you lay out content in rows and columns.

// Grid container and item types
mod types;
pub use types::{
    GridAutoFlow, GridItem, GridTrack, GridTrackSize, TrackBreadth, TrackListType, TrackRepeat,
    collect_grid_items,
};

// Track sizing algorithm
mod track_sizing;
pub use track_sizing::{GridAxis, GridAxisTracks, ResolvedTrackSizes, resolve_track_sizes};

// Grid placement algorithm
mod placement;
pub use placement::{GridArea, GridPosition, place_grid_items};

// Grid layout algorithm
mod layout;
pub use layout::{
    GridAlignment, GridContainerInputs, GridLayoutResult, GridPlacedItem, layout_grid,
};
