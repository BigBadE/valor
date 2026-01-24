//! Block layout implementation.

use rewrite_core::{NodeId, Relationship, ScopedDb};
use rewrite_css::{
    CssKeyword, CssValue, EndMarker, InlineMarker, PositionQuery, StartMarker, Subpixels,
    ViewportInput, ViewportSize,
    storage::{LayoutHeightQuery, LayoutWidthQuery},
};
use rewrite_css_dimensional::{BorderWidthQuery, MarginQuery, PaddingQuery, PositionOffsetQuery};
use rewrite_layout_offset_impl::OffsetMode;
use rewrite_layout_size_impl::{SizeDispatcher, SizeMode};
use rewrite_layout_util::{Axis, Dispatcher};

/// Compute the size of a block.
pub fn compute_block_size<D>(scoped: &mut ScopedDb, axis: Axis, mode: SizeMode) -> Subpixels
where
    D: SizeDispatcher + 'static,
{
    match (axis, mode) {
        (Axis::Inline, SizeMode::Constrained) => {
            // Check for explicit width first
            let width = scoped.query::<LayoutWidthQuery>();
            if width > 0 {
                // LayoutWidthQuery returns content-box width, need to add padding/border for border-box
                return add_padding_and_border(scoped, width, axis);
            }

            // Inline constrained: use parent's content box width minus own margins
            let parent_ids = scoped
                .db()
                .resolve_relationship(scoped.node(), Relationship::Parent);

            let parent_content_width = if let Some(&parent) = parent_ids.first() {
                // Get parent's inline size (border-box)
                let mut parent_scoped = scoped.scoped_to(parent);
                let parent_border_box =
                    D::query(&mut parent_scoped, Axis::Inline, SizeMode::Constrained);

                // Subtract parent's padding and border to get content box
                let parent_padding_start =
                    parent_scoped.query::<PaddingQuery<InlineMarker, StartMarker>>();
                let parent_padding_end =
                    parent_scoped.query::<PaddingQuery<InlineMarker, EndMarker>>();
                let parent_border_start =
                    parent_scoped.query::<BorderWidthQuery<InlineMarker, StartMarker>>();
                let parent_border_end =
                    parent_scoped.query::<BorderWidthQuery<InlineMarker, EndMarker>>();

                parent_border_box
                    .saturating_sub(parent_padding_start)
                    .saturating_sub(parent_padding_end)
                    .saturating_sub(parent_border_start)
                    .saturating_sub(parent_border_end)
            } else {
                // No parent - use viewport width
                let viewport = scoped
                    .db()
                    .get_input::<ViewportInput>(&())
                    .unwrap_or_else(ViewportSize::default);
                (viewport.width * 64.0) as Subpixels
            };

            // Subtract own margins to get this element's content box width
            let margin_start = scoped.query::<MarginQuery<InlineMarker, StartMarker>>();
            let margin_end = scoped.query::<MarginQuery<InlineMarker, EndMarker>>();

            let content_width = parent_content_width
                .saturating_sub(margin_start)
                .saturating_sub(margin_end);

            // Add own padding and border to get border-box width (to match getBoundingClientRect)
            add_padding_and_border(scoped, content_width, axis)
        }
        (Axis::Block, SizeMode::Constrained) => {
            // Check for explicit height first
            let height = scoped.query::<LayoutHeightQuery>();
            if height > 0 {
                // LayoutHeightQuery returns content-box height, need to add padding/border for border-box
                return add_padding_and_border(scoped, height, axis);
            }

            // Height is auto: sum of children's block sizes + text content height
            let children_size: Subpixels = scoped
                .db()
                .resolve_relationship(scoped.node(), Relationship::Children)
                .iter()
                .map(|&child| {
                    let mut child_scoped = scoped.scoped_to(child);
                    D::query(&mut child_scoped, Axis::Block, SizeMode::Constrained)
                })
                .sum();

            // Add text content height
            let text_height = rewrite_layout_text::compute_text_content_height(scoped);
            let content_size = children_size + text_height;

            add_padding_and_border(scoped, content_size, axis)
        }

        (Axis::Block, SizeMode::Intrinsic) => {
            // Intrinsic block size: sum of children's intrinsic block sizes
            let content_size: Subpixels = scoped
                .db()
                .resolve_relationship(scoped.node(), Relationship::Children)
                .iter()
                .map(|&child| {
                    let mut child_scoped = scoped.scoped_to(child);
                    D::query(&mut child_scoped, Axis::Block, SizeMode::Intrinsic)
                })
                .sum();
            add_padding_and_border(scoped, content_size, axis)
        }
        (Axis::Inline, SizeMode::Intrinsic) => {
            // Intrinsic inline size: max of children's intrinsic inline sizes (they stack)
            let content_size: Subpixels = scoped
                .db()
                .resolve_relationship(scoped.node(), Relationship::Children)
                .iter()
                .map(|&child| {
                    let mut child_scoped = scoped.scoped_to(child);
                    D::query(&mut child_scoped, Axis::Inline, SizeMode::Intrinsic)
                })
                .max()
                .unwrap_or(0);
            add_padding_and_border(scoped, content_size, axis)
        }
    }
}

/// Add padding and border to a content size.
fn add_padding_and_border(scoped: &mut ScopedDb, content_size: Subpixels, axis: Axis) -> Subpixels {
    let (padding, border) = match axis {
        Axis::Block => {
            let padding_start =
                scoped.query::<PaddingQuery<rewrite_css::BlockMarker, StartMarker>>();
            let padding_end = scoped.query::<PaddingQuery<rewrite_css::BlockMarker, EndMarker>>();
            let border_start =
                scoped.query::<BorderWidthQuery<rewrite_css::BlockMarker, StartMarker>>();
            let border_end =
                scoped.query::<BorderWidthQuery<rewrite_css::BlockMarker, EndMarker>>();
            (padding_start + padding_end, border_start + border_end)
        }
        Axis::Inline => {
            let padding_start = scoped.query::<PaddingQuery<InlineMarker, StartMarker>>();
            let padding_end = scoped.query::<PaddingQuery<InlineMarker, EndMarker>>();
            let border_start = scoped.query::<BorderWidthQuery<InlineMarker, StartMarker>>();
            let border_end = scoped.query::<BorderWidthQuery<InlineMarker, EndMarker>>();
            (padding_start + padding_end, border_start + border_end)
        }
    };

    content_size + padding + border
}

/// Compute the offset of a block.
pub fn compute_block_offset<OffsetDisp, SizeDisp>(
    scoped: &mut ScopedDb,
    axis: Axis,
    mode: OffsetMode,
) -> Subpixels
where
    OffsetDisp: Dispatcher<(Axis, OffsetMode), Returns = Subpixels> + 'static,
    SizeDisp: SizeDispatcher + 'static,
{
    // Check if element has positioned layout
    let position = scoped.query::<PositionQuery>();

    match position {
        // Relative positioning: compute static offset, then apply position offset
        CssValue::Keyword(CssKeyword::Relative) => {
            let static_offset = compute_static_offset::<OffsetDisp, SizeDisp>(scoped, axis);
            apply_relative_offset(scoped, axis, static_offset)
        }

        // Absolute/Fixed positioning
        CssValue::Keyword(CssKeyword::Absolute) | CssValue::Keyword(CssKeyword::Fixed) => {
            let static_position = compute_static_offset::<OffsetDisp, SizeDisp>(scoped, axis);
            apply_positioned_offset::<OffsetDisp, SizeDisp>(
                scoped,
                axis,
                static_position,
                &position,
            )
        }

        // Sticky positioning - for now, just use static (full implementation needs scroll state)
        CssValue::Keyword(CssKeyword::Sticky) => {
            compute_static_offset::<OffsetDisp, SizeDisp>(scoped, axis)
        }

        // Static positioning (default) or any other value
        _ => compute_static_offset::<OffsetDisp, SizeDisp>(scoped, axis),
    }
}

/// Apply relative positioning offset.
///
/// Relative positioning offsets the element from its normal position without
/// affecting the layout of other elements (the space is reserved).
fn apply_relative_offset(scoped: &mut ScopedDb, axis: Axis, normal_offset: Subpixels) -> Subpixels {
    match axis {
        Axis::Block => {
            // Check top first (takes precedence over bottom)
            let top = scoped.query::<PositionOffsetQuery<rewrite_css::BlockMarker, StartMarker>>();
            if top != 0 {
                return normal_offset + top;
            }

            // Check bottom
            let bottom = scoped.query::<PositionOffsetQuery<rewrite_css::BlockMarker, EndMarker>>();
            if bottom != 0 {
                return normal_offset - bottom;
            }

            normal_offset
        }
        Axis::Inline => {
            // Check left first (takes precedence over right in LTR)
            let left = scoped.query::<PositionOffsetQuery<InlineMarker, StartMarker>>();
            if left != 0 {
                return normal_offset + left;
            }

            // Check right
            let right = scoped.query::<PositionOffsetQuery<InlineMarker, EndMarker>>();
            if right != 0 {
                return normal_offset - right;
            }

            normal_offset
        }
    }
}

/// Apply absolute or fixed positioning offset.
///
/// For absolute positioning, the element is positioned relative to its nearest
/// positioned ancestor. For fixed positioning, it's positioned relative to the
/// viewport.
fn apply_positioned_offset<OffsetDisp, SizeDisp>(
    scoped: &mut ScopedDb,
    axis: Axis,
    static_position: Subpixels,
    position_value: &CssValue,
) -> Subpixels
where
    OffsetDisp: Dispatcher<(Axis, OffsetMode), Returns = Subpixels> + 'static,
    SizeDisp: SizeDispatcher + 'static,
{
    let is_fixed = matches!(position_value, CssValue::Keyword(CssKeyword::Fixed));

    // Find containing block
    let containing_block = if is_fixed {
        // Fixed positioning uses viewport as containing block
        None
    } else {
        // Absolute positioning uses nearest positioned ancestor
        find_positioned_ancestor(scoped)
    };

    let (cb_offset, cb_size) = if let Some(cb_node) = containing_block {
        let mut cb_scoped = scoped.scoped_to(cb_node);
        let offset = OffsetDisp::query(&mut cb_scoped, (axis, OffsetMode::Static));
        let size = SizeDisp::query(&mut cb_scoped, axis, SizeMode::Constrained);

        // Add border and padding to get to content edge
        let (border_start, padding_start) = match axis {
            Axis::Block => {
                let b =
                    cb_scoped.query::<BorderWidthQuery<rewrite_css::BlockMarker, StartMarker>>();
                let p = cb_scoped.query::<PaddingQuery<rewrite_css::BlockMarker, StartMarker>>();
                (b, p)
            }
            Axis::Inline => {
                let b = cb_scoped.query::<BorderWidthQuery<InlineMarker, StartMarker>>();
                let p = cb_scoped.query::<PaddingQuery<InlineMarker, StartMarker>>();
                (b, p)
            }
        };

        (offset + border_start + padding_start, size)
    } else {
        // Use viewport
        (0, get_viewport_size(scoped, axis))
    };

    match axis {
        Axis::Block => {
            // Check top first
            let top = scoped.query::<PositionOffsetQuery<rewrite_css::BlockMarker, StartMarker>>();
            if top != 0 {
                return cb_offset + top;
            }

            // Check bottom
            let bottom = scoped.query::<PositionOffsetQuery<rewrite_css::BlockMarker, EndMarker>>();
            if bottom != 0 {
                let node_size = SizeDisp::query(scoped, axis, SizeMode::Constrained);
                let (border_end, padding_end) = if let Some(cb_node) = containing_block {
                    let mut cb_scoped = scoped.scoped_to(cb_node);
                    let b =
                        cb_scoped.query::<BorderWidthQuery<rewrite_css::BlockMarker, EndMarker>>();
                    let p = cb_scoped.query::<PaddingQuery<rewrite_css::BlockMarker, EndMarker>>();
                    (b, p)
                } else {
                    (0, 0)
                };
                return cb_offset + cb_size - border_end - padding_end - bottom - node_size;
            }

            // Both auto: use static position
            static_position
        }
        Axis::Inline => {
            // Check left first
            let left = scoped.query::<PositionOffsetQuery<InlineMarker, StartMarker>>();
            if left != 0 {
                return cb_offset + left;
            }

            // Check right
            let right = scoped.query::<PositionOffsetQuery<InlineMarker, EndMarker>>();
            if right != 0 {
                let node_size = SizeDisp::query(scoped, axis, SizeMode::Constrained);
                let (border_end, padding_end) = if let Some(cb_node) = containing_block {
                    let mut cb_scoped = scoped.scoped_to(cb_node);
                    let b = cb_scoped.query::<BorderWidthQuery<InlineMarker, EndMarker>>();
                    let p = cb_scoped.query::<PaddingQuery<InlineMarker, EndMarker>>();
                    (b, p)
                } else {
                    (0, 0)
                };
                return cb_offset + cb_size - border_end - padding_end - right - node_size;
            }

            // Both auto: use static position
            static_position
        }
    }
}

/// Find the nearest positioned ancestor (containing block for absolute positioning).
fn find_positioned_ancestor(scoped: &mut ScopedDb) -> Option<NodeId> {
    let parent_ids = scoped
        .db()
        .resolve_relationship(scoped.node(), Relationship::Parent);

    let mut current = parent_ids.first().copied();

    while let Some(node) = current {
        let mut node_scoped = scoped.scoped_to(node);
        let position = node_scoped.query::<PositionQuery>();

        match position {
            CssValue::Keyword(CssKeyword::Static) => {
                // Keep searching up
                let parents = scoped.db().resolve_relationship(node, Relationship::Parent);
                current = parents.first().copied();
            }
            _ => {
                // Found a positioned ancestor
                return Some(node);
            }
        }
    }

    None
}

/// Get viewport size for an axis.
fn get_viewport_size(scoped: &mut ScopedDb, axis: Axis) -> Subpixels {
    let viewport = scoped
        .db()
        .get_input::<ViewportInput>(&())
        .unwrap_or_else(ViewportSize::default);

    match axis {
        Axis::Block => (viewport.height * 64.0) as Subpixels,
        Axis::Inline => (viewport.width * 64.0) as Subpixels,
    }
}

/// Compute static (normal flow) offset for a block.
fn compute_static_offset<OffsetDisp, SizeDisp>(scoped: &mut ScopedDb, axis: Axis) -> Subpixels
where
    OffsetDisp: Dispatcher<(Axis, OffsetMode), Returns = Subpixels> + 'static,
    SizeDisp: SizeDispatcher + 'static,
{
    let mode = OffsetMode::Static;

    // Check if this is the root element (no parent) - root is always at 0,0
    let parent_ids = scoped
        .db()
        .resolve_relationship(scoped.node(), Relationship::Parent);
    if parent_ids.is_empty() {
        return 0;
    }

    match axis {
        Axis::Inline => {
            // Inline offset: parent's content edge + own start margin
            let parent_ids = scoped
                .db()
                .resolve_relationship(scoped.node(), Relationship::Parent);

            let parent_offset = if let Some(&parent) = parent_ids.first() {
                let mut parent_scoped = scoped.scoped_to(parent);
                OffsetDisp::query(&mut parent_scoped, (Axis::Inline, mode))
            } else {
                0
            };

            // Only add margin if we have a parent (root element has no margin)
            let margin_start = if !parent_ids.is_empty() {
                scoped.query::<MarginQuery<InlineMarker, StartMarker>>()
            } else {
                0
            };

            parent_offset + margin_start
        }
        Axis::Block => {
            // Block offset: parent's content edge + sum of previous siblings' sizes + own start margin
            let parent_ids = scoped
                .db()
                .resolve_relationship(scoped.node(), Relationship::Parent);

            let parent_offset = if let Some(&parent) = parent_ids.first() {
                let mut parent_scoped = scoped.scoped_to(parent);
                OffsetDisp::query(&mut parent_scoped, (Axis::Block, mode))
            } else {
                0
            };

            // Sum previous siblings' block sizes using the SizeDisp parameter
            let prev_siblings = scoped
                .db()
                .resolve_relationship(scoped.node(), Relationship::PreviousSiblings);
            let prev_sizes: Subpixels = prev_siblings
                .iter()
                .map(|&sibling| {
                    let mut sibling_scoped = scoped.scoped_to(sibling);
                    // Query sibling's block size using the size dispatcher
                    SizeDisp::query(&mut sibling_scoped, Axis::Block, SizeMode::Constrained)
                })
                .sum();

            // Use margin collapsing logic to get the effective margin (only if we have a parent)
            let margin_start = if !parent_ids.is_empty() {
                rewrite_layout_margin::get_effective_margin_start(scoped)
            } else {
                0
            };

            parent_offset + prev_sizes + margin_start
        }
    }
}
