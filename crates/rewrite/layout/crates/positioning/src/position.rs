use rewrite_core::{NodeId, ScopedDb};
/// Positioned layout module implementing CSS position property.
///
/// This module handles:
/// - Static positioning (normal flow)
/// - Relative positioning (offset from normal position)
/// - Absolute positioning (relative to positioned ancestor)
/// - Fixed positioning (relative to viewport)
/// - Sticky positioning (hybrid of relative and fixed)
///
/// Spec: https://www.w3.org/TR/css-position-3/
use rewrite_css::Subpixels;
use rewrite_css::{CssKeyword, CssValue, PositionQuery};
use rewrite_css_dimensional::{PaddingQuery, PositionOffsetQuery};
use rewrite_layout_offset::OffsetQuery;
use rewrite_layout_offset_impl::StaticMarker;
use rewrite_layout_size_impl::ConstrainedMarker;
use rewrite_layout_util::{Axis, BlockMarker, InlineMarker};

// Type alias for SizeQuery with flex implementation
type SizeQuery<AxisParam, ModeParam> =
    rewrite_layout_size::SizeQueryGeneric<AxisParam, ModeParam, rewrite_layout_flex::FlexSize>;

/// Compute offset for positioned elements (absolute, fixed, sticky, relative).
///
/// # Positioning Schemes:
///
/// ## Static (default)
/// - Element positioned in normal flow
/// - top/right/bottom/left have no effect
///
/// ## Relative
/// - Element positioned relative to its normal position
/// - Offset by top/right/bottom/left values
/// - Original space in flow is preserved
///
/// ## Absolute
/// - Element removed from normal flow
/// - Positioned relative to nearest positioned ancestor (or containing block)
/// - Uses top/right/bottom/left to determine final position
///
/// ## Fixed
/// - Element removed from normal flow
/// - Positioned relative to viewport (initial containing block)
/// - Scrolls with viewport, not document
///
/// ## Sticky
/// - Hybrid: behaves as relative until scroll threshold, then becomes fixed
/// - Threshold determined by top/right/bottom/left values
pub fn compute_positioned_offset<AxisParam>(
    scoped: &mut ScopedDb,
    normal_flow_offset: Subpixels,
) -> Subpixels
where
    AxisParam: rewrite_layout_util::AxisMarker + 'static,
{
    let position = scoped.query::<PositionQuery>();

    match position {
        CssValue::Keyword(CssKeyword::Static) => normal_flow_offset,

        CssValue::Keyword(CssKeyword::Relative) => {
            compute_relative_offset::<AxisParam>(scoped, normal_flow_offset)
        }

        CssValue::Keyword(CssKeyword::Absolute) => {
            compute_absolute_offset::<AxisParam>(scoped, normal_flow_offset)
        }

        CssValue::Keyword(CssKeyword::Fixed) => compute_fixed_offset::<AxisParam>(scoped),

        CssValue::Keyword(CssKeyword::Sticky) => {
            compute_sticky_offset::<AxisParam>(scoped, normal_flow_offset)
        }

        _ => normal_flow_offset,
    }
}

/// Compute relative positioned offset.
///
/// The element is positioned relative to its normal position. The offset
/// values (top, right, bottom, left) shift the element from where it would
/// normally be, but the space it would have occupied is still reserved.
///
/// If both top and bottom are specified, top wins.
/// If both left and right are specified, left wins (in LTR).
fn compute_relative_offset<AxisParam>(scoped: &mut ScopedDb, normal_offset: Subpixels) -> Subpixels
where
    AxisParam: rewrite_layout_util::AxisMarker + 'static,
{
    use rewrite_css::{EndMarker, StartMarker};

    // Check start edge (top for block, left for inline)
    let start = if std::any::TypeId::of::<AxisParam>() == std::any::TypeId::of::<BlockMarker>() {
        scoped.query::<PositionOffsetQuery<rewrite_css::BlockMarker, StartMarker>>()
    } else {
        scoped.query::<PositionOffsetQuery<rewrite_css::InlineMarker, StartMarker>>()
    };

    if start != 0 {
        return normal_offset + start;
    }

    // Check end edge (bottom for block, right for inline)
    let end = if std::any::TypeId::of::<AxisParam>() == std::any::TypeId::of::<BlockMarker>() {
        scoped.query::<PositionOffsetQuery<rewrite_css::BlockMarker, EndMarker>>()
    } else {
        scoped.query::<PositionOffsetQuery<rewrite_css::InlineMarker, EndMarker>>()
    };

    if end != 0 {
        // End offset shifts in opposite direction
        return normal_offset - end;
    }

    normal_offset
}

/// Compute absolute positioned offset.
///
/// The element is positioned relative to its nearest positioned ancestor
/// (an ancestor with position other than static). If no such ancestor exists,
/// it's positioned relative to the initial containing block.
///
/// The containing block's edges are:
/// - For block axis: padding edge of containing block
/// - For inline axis: padding edge of containing block
///
/// Offset calculation:
/// - If 'top' is specified: distance from top edge of containing block
/// - If 'bottom' is specified and 'top' is auto: distance from bottom edge
/// - If both are auto: use static position
fn compute_absolute_offset<AxisParam>(
    scoped: &mut ScopedDb,
    static_position: Subpixels,
) -> Subpixels
where
    AxisParam: rewrite_layout_util::AxisMarker + 'static,
{
    use rewrite_css::{EndMarker, StartMarker};

    // Find containing block (nearest positioned ancestor)
    let containing_block = find_containing_block(scoped);

    let (cb_offset, cb_size) = if let Some(cb_node) = containing_block {
        let cb_offset = scoped.node_query::<OffsetQuery<AxisParam, StaticMarker>>(cb_node);
        let cb_size = scoped.node_query::<SizeQuery<AxisParam, ConstrainedMarker>>(cb_node);
        (cb_offset, cb_size)
    } else {
        // No positioned ancestor, use viewport (0, 0) with full size
        (0, get_viewport_size::<AxisParam>(scoped))
    };

    // Get padding of containing block
    let cb_padding_start = if let Some(cb_node) = containing_block {
        get_node_padding_start::<AxisParam>(scoped, cb_node)
    } else {
        0
    };

    // Check start offset (top for block, left for inline)
    let start = if std::any::TypeId::of::<AxisParam>() == std::any::TypeId::of::<BlockMarker>() {
        scoped.query::<PositionOffsetQuery<rewrite_css::BlockMarker, StartMarker>>()
    } else {
        scoped.query::<PositionOffsetQuery<rewrite_css::InlineMarker, StartMarker>>()
    };

    if start != 0 {
        return cb_offset + cb_padding_start + start;
    }

    // Check end offset (bottom for block, right for inline)
    let end = if std::any::TypeId::of::<AxisParam>() == std::any::TypeId::of::<BlockMarker>() {
        scoped.query::<PositionOffsetQuery<rewrite_css::BlockMarker, EndMarker>>()
    } else {
        scoped.query::<PositionOffsetQuery<rewrite_css::InlineMarker, EndMarker>>()
    };

    if end != 0 {
        let node_size = scoped.query::<SizeQuery<AxisParam, ConstrainedMarker>>();
        let cb_padding_end = if let Some(cb_node) = containing_block {
            get_node_padding_end::<AxisParam>(scoped, cb_node)
        } else {
            0
        };
        return cb_offset + cb_size - cb_padding_end - end - node_size;
    }

    // Both auto: use static position
    static_position
}

/// Compute fixed positioned offset.
///
/// The element is positioned relative to the viewport (initial containing block).
/// It does not move when the page is scrolled.
///
/// The offset is calculated from the viewport edges:
/// - top: distance from top of viewport
/// - bottom: distance from bottom of viewport
/// - left: distance from left of viewport
/// - right: distance from right of viewport
fn compute_fixed_offset<AxisParam>(scoped: &mut ScopedDb) -> Subpixels
where
    AxisParam: rewrite_layout_util::AxisMarker + 'static,
{
    use rewrite_css::{EndMarker, StartMarker};

    // Get viewport size
    let viewport_size = get_viewport_size::<AxisParam>(scoped);

    // Check start offset (top for block, left for inline)
    let start = if std::any::TypeId::of::<AxisParam>() == std::any::TypeId::of::<BlockMarker>() {
        scoped.query::<PositionOffsetQuery<rewrite_css::BlockMarker, StartMarker>>()
    } else {
        scoped.query::<PositionOffsetQuery<rewrite_css::InlineMarker, StartMarker>>()
    };

    if start != 0 {
        return start;
    }

    // Check end offset (bottom for block, right for inline)
    let end = if std::any::TypeId::of::<AxisParam>() == std::any::TypeId::of::<BlockMarker>() {
        scoped.query::<PositionOffsetQuery<rewrite_css::BlockMarker, EndMarker>>()
    } else {
        scoped.query::<PositionOffsetQuery<rewrite_css::InlineMarker, EndMarker>>()
    };

    if end != 0 {
        let node_size = scoped.query::<SizeQuery<AxisParam, ConstrainedMarker>>();
        return viewport_size - end - node_size;
    }

    // Both auto: position at origin (0, 0)
    0
}

/// Compute sticky positioned offset.
///
/// Sticky positioning is a hybrid of relative and fixed positioning.
/// The element is treated as relatively positioned until it crosses a
/// specified threshold (determined by top/right/bottom/left), at which
/// point it is treated as fixed positioned.
///
/// For now, we implement a simplified version that behaves as relative
/// positioning. A full implementation would require:
/// - Scroll offset tracking
/// - Containing block boundary detection
/// - Threshold calculation and comparison
fn compute_sticky_offset<AxisParam>(scoped: &mut ScopedDb, normal_offset: Subpixels) -> Subpixels
where
    AxisParam: rewrite_layout_util::AxisMarker + 'static,
{
    // Get current scroll state
    let scroll_container = super::sticky::get_sticky_scroll_container(scoped);
    let scroll_state = super::sticky::get_scroll_state(scoped, scroll_container);

    // Determine axis
    let axis = if std::any::TypeId::of::<AxisParam>() == std::any::TypeId::of::<BlockMarker>() {
        Axis::Block
    } else {
        Axis::Inline
    };

    // Compute sticky offset with scroll detection
    super::sticky::compute_sticky_offset(scoped, axis, normal_offset, &scroll_state)
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Find the containing block for absolutely positioned elements.
///
/// The containing block is the nearest ancestor with position other than static.
/// If no such ancestor exists, returns None (use initial containing block/viewport).
fn find_containing_block(scoped: &mut ScopedDb) -> Option<NodeId> {
    let mut current = scoped.parent_id();

    loop {
        if let Some(node) = current {
            let position = scoped.node_query::<PositionQuery>(node);
            match position {
                CssValue::Keyword(CssKeyword::Static) => {
                    // Not a containing block, keep searching
                    current = scoped.node_parent(node);
                }
                _ => {
                    // Found positioned ancestor
                    return Some(node);
                }
            }
        } else {
            // Reached root, no positioned ancestor
            return None;
        }
    }
}

/// Get viewport size for an axis.
///
/// Queries the actual viewport dimensions from ViewportInput.
fn get_viewport_size<AxisParam>(scoped: &mut ScopedDb) -> Subpixels
where
    AxisParam: rewrite_layout_util::AxisMarker + 'static,
{
    use rewrite_css::{ViewportInput, ViewportSize};

    let viewport = scoped
        .db()
        .get_input::<ViewportInput>(&())
        .unwrap_or_else(ViewportSize::default);

    if std::any::TypeId::of::<AxisParam>() == std::any::TypeId::of::<BlockMarker>() {
        (viewport.height * 64.0) as Subpixels // Height in subpixels
    } else {
        (viewport.width * 64.0) as Subpixels // Width in subpixels
    }
}

/// Get start padding of a specific node.
fn get_node_padding_start<AxisParam>(scoped: &mut ScopedDb, node: NodeId) -> Subpixels
where
    AxisParam: rewrite_layout_util::AxisMarker + 'static,
{
    use rewrite_css::StartMarker;

    if std::any::TypeId::of::<AxisParam>() == std::any::TypeId::of::<BlockMarker>() {
        scoped.node_query::<PaddingQuery<rewrite_css::BlockMarker, StartMarker>>(node)
    } else {
        scoped.node_query::<PaddingQuery<rewrite_css::InlineMarker, StartMarker>>(node)
    }
}

/// Get end padding of a specific node.
fn get_node_padding_end<AxisParam>(scoped: &mut ScopedDb, node: NodeId) -> Subpixels
where
    AxisParam: rewrite_layout_util::AxisMarker + 'static,
{
    use rewrite_css::EndMarker;

    if std::any::TypeId::of::<AxisParam>() == std::any::TypeId::of::<BlockMarker>() {
        scoped.node_query::<PaddingQuery<rewrite_css::BlockMarker, EndMarker>>(node)
    } else {
        scoped.node_query::<PaddingQuery<rewrite_css::InlineMarker, EndMarker>>(node)
    }
}

/// Check if a node establishes a containing block for absolutely positioned descendants.
///
/// A containing block is established by:
/// - Elements with position: relative, absolute, fixed, or sticky
/// - The root element
/// - Elements with transform, filter, perspective, etc. (not yet implemented)
pub fn establishes_containing_block(scoped: &mut ScopedDb, node: NodeId) -> bool {
    let position = scoped.node_query::<PositionQuery>(node);
    !matches!(position, CssValue::Keyword(CssKeyword::Static))
}
