use rewrite_core::ScopedDb;
use rewrite_css::{EndMarker, PaddingQuery, StartMarker};

use crate::{BlockMarker, ConstrainedMarker, LayoutsMarker, OffsetQuery, SizeQuery, Subpixels};

/// Compute the parent start position (offset + leading padding) for any axis.
pub fn parent_start<Axis: LayoutsMarker + 'static>(scoped: &mut ScopedDb) -> Subpixels {
    use rewrite_core::Relationship;

    // Check if we have a parent
    let parent_ids = scoped
        .db()
        .resolve_relationship(scoped.node(), Relationship::Parent);
    if parent_ids.is_empty() {
        // No parent - we're at the root, start at 0
        return 0;
    }

    let offset = scoped.parent::<OffsetQuery<Axis>>();

    // Map layout axis to CSS axis marker
    let padding = if std::any::TypeId::of::<Axis>() == std::any::TypeId::of::<BlockMarker>() {
        scoped.parent::<PaddingQuery<rewrite_css::BlockMarker, StartMarker>>()
    } else {
        scoped.parent::<PaddingQuery<rewrite_css::InlineMarker, StartMarker>>()
    };

    offset + padding
}

/// Compute the sum of padding (start + end) for the parent on the inline axis.
/// This is used for calculating available space.
pub fn parent_padding_sum_inline(scoped: &mut ScopedDb) -> Subpixels {
    let start = scoped.parent::<PaddingQuery<rewrite_css::InlineMarker, StartMarker>>();
    let end = scoped.parent::<PaddingQuery<rewrite_css::InlineMarker, EndMarker>>();
    start + end
}

/// Compute offset from parent start plus all previous siblings' sizes.
/// This is the standard static flow offset calculation.
pub fn get_offset<Axis: LayoutsMarker + 'static>(scoped: &mut ScopedDb) -> Subpixels {
    let parent = parent_start::<Axis>(scoped);
    let siblings: Subpixels = scoped
        .prev_siblings::<SizeQuery<Axis, ConstrainedMarker>>()
        .sum();
    parent + siblings
}
