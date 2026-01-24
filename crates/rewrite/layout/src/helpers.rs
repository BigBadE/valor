use crate::PaddingQuery;
use rewrite_core::ScopedDb;
use rewrite_css::{EndMarker, StartMarker};

use crate::{
    BlockMarker, BlockOffsetQuery, BlockSizeQuery, InlineMarker, InlineOffsetQuery,
    InlineSizeQuery, Subpixels,
};

/// Compute the parent start position (offset + leading padding) for inline axis.
pub fn parent_start_inline(scoped: &mut ScopedDb) -> Subpixels {
    use rewrite_core::Relationship;

    // Check if we have a parent
    let parent_ids = scoped
        .db()
        .resolve_relationship(scoped.node(), Relationship::Parent);
    if parent_ids.is_empty() {
        // No parent - we're at the root, start at 0
        return 0;
    }

    // Map layout axis to concrete query types
    if std::any::TypeId::of::<Axis>() == std::any::TypeId::of::<BlockMarker>() {
        let offset = scoped.parent::<BlockOffsetQuery>();
        let padding = scoped.parent::<PaddingQuery<rewrite_css::BlockMarker, StartMarker>>();
        offset + padding
    } else {
        let offset = scoped.parent::<InlineOffsetQuery>();
        let padding = scoped.parent::<PaddingQuery<rewrite_css::InlineMarker, StartMarker>>();
        offset + padding
    }
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

    // Map axis to concrete query type
    let siblings: Subpixels =
        if std::any::TypeId::of::<Axis>() == std::any::TypeId::of::<BlockMarker>() {
            scoped.prev_siblings::<BlockSizeQuery>().sum()
        } else {
            scoped.prev_siblings::<InlineSizeQuery>().sum()
        };

    parent + siblings
}
