//! Positioning layout implementation (relative, absolute, fixed, sticky).
//!
//! This module provides positioned layout computation including margin collapsing.

pub mod margin;
pub mod position;
pub mod sticky;

// Re-export commonly used functions
pub use margin::{
    can_collapse_with_parent_start, compute_collapsed_margin_end, compute_collapsed_margin_start,
    get_effective_margin_end, get_effective_margin_start, get_margin_for_offset,
};

pub use position::{compute_positioned_offset, establishes_containing_block};

pub use sticky::{
    ScrollState, StickyConstraint, calculate_sticky_boundaries, compute_sticky_offset,
    get_scroll_state, get_sticky_scroll_container, has_sticky_threshold, is_currently_sticking,
};
