//! Float layout implementation.

use rewrite_core::ScopedDb;
use rewrite_css::Subpixels;
use rewrite_layout_offset_impl::OffsetMode;
use rewrite_layout_size_impl::SizeMode;
use rewrite_layout_util::{Axis, Dispatcher};

/// Compute the size of a float.
pub fn compute_float_size<D>(_scoped: &mut ScopedDb, _axis: Axis, _mode: SizeMode) -> Subpixels
where
    D: Dispatcher<(Axis, SizeMode), Returns = Subpixels>,
{
    // TODO: Implement float sizing algorithm
    // Will use D::query(scoped, (axis, mode)) to query sizes
    0
}

/// Compute the offset of a float.
pub fn compute_float_offset<D>(_scoped: &mut ScopedDb, _axis: Axis, _mode: OffsetMode) -> Subpixels
where
    D: Dispatcher<(Axis, OffsetMode), Returns = Subpixels>,
{
    // TODO: Implement float positioning algorithm
    0
}
