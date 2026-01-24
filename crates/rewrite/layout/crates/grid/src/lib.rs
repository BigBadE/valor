//! CSS Grid layout implementation.

use rewrite_core::ScopedDb;
use rewrite_css::Subpixels;
use rewrite_layout_offset_impl::OffsetMode;
use rewrite_layout_size_impl::SizeMode;
use rewrite_layout_util::{Axis, Dispatcher};

/// Compute the size of a grid container.
pub fn compute_grid_size<D>(_scoped: &mut ScopedDb, _axis: Axis, _mode: SizeMode) -> Subpixels
where
    D: Dispatcher<(Axis, SizeMode), Returns = Subpixels>,
{
    // TODO: Implement grid sizing algorithm
    // Will use D::query(scoped, (axis, mode)) to query child sizes
    0
}

/// Compute the offset of a grid item.
pub fn compute_grid_offset<D>(_scoped: &mut ScopedDb, _axis: Axis, _mode: OffsetMode) -> Subpixels
where
    D: Dispatcher<(Axis, OffsetMode), Returns = Subpixels>,
{
    // TODO: Implement grid positioning algorithm
    0
}
