//! CSS Grid layout formulas.

use rewrite_core::{Axis, Formula};

use super::size::size_query;

/// Compute grid container size formula.
pub fn grid_size(axis: Axis) -> &'static Formula {
    aggregate!(Sum, Children, size_query, axis)
}

/// Compute grid item offset formula.
pub fn grid_offset(axis: Axis) -> &'static Formula {
    aggregate!(Sum, PrevSiblings, size_query, axis)
}
