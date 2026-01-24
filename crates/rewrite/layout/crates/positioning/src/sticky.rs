use rewrite_core::{NodeId, ScopedDb};
/// Sticky positioning module implementing CSS Position Level 3 sticky positioning.
///
/// This module handles:
/// - Sticky positioning thresholds
/// - Scroll container detection
/// - Sticky offset calculation with scroll state
/// - Containing block boundaries
///
/// Spec: https://www.w3.org/TR/css-position-3/#sticky-pos
use rewrite_css::Subpixels;
use rewrite_css::{CssKeyword, CssValue, OverflowQuery};
use rewrite_css_dimensional::PositionOffsetQuery;
use rewrite_layout_offset::OffsetQuery;
use rewrite_layout_offset_impl::StaticMarker;
use rewrite_layout_size_impl::ConstrainedMarker;
use rewrite_layout_util::{Axis, BlockMarker, InlineMarker};

// Type alias for SizeQuery with flex implementation
type SizeQuery<AxisParam, ModeParam> =
    rewrite_layout_size::SizeQueryGeneric<AxisParam, ModeParam, rewrite_layout_flex::FlexSize>;

/// Represents the scroll state of a scrolling container.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScrollState {
    /// Scroll offset in the block direction (vertical scroll).
    pub block_scroll: Subpixels,
    /// Scroll offset in the inline direction (horizontal scroll).
    pub inline_scroll: Subpixels,
}

/// Represents a sticky positioning constraint.
#[derive(Debug, Clone, Copy)]
pub struct StickyConstraint {
    /// The containing block node.
    pub containing_block: NodeId,
    /// The scrolling container node.
    pub scroll_container: NodeId,
    /// The normal flow position (where the element would be without sticky).
    pub normal_position: Subpixels,
    /// The sticky threshold (top/left offset from scroll container edge).
    pub threshold: Subpixels,
    /// The maximum offset (bottom edge of containing block).
    pub max_offset: Subpixels,
}

/// Compute the offset for a sticky positioned element.
///
/// # Sticky Positioning Algorithm:
///
/// 1. The element is positioned according to normal flow (relative positioning)
/// 2. When the scroll position reaches a threshold, the element "sticks" to that position
/// 3. The element stops being sticky when it reaches the boundary of its containing block
///
/// ## Behavior:
/// - Initially: Positioned in normal flow (like `position: relative`)
/// - Scrolling down: When scroll position makes element reach threshold, it sticks
/// - Scrolling more: Element stays at threshold position relative to viewport
/// - Near end: Element unsticks when containing block boundary is reached
///
/// ## Example:
/// ```css
/// .sticky {
///     position: sticky;
///     top: 20px;  /* Threshold: stick when 20px from top of scroll container */
/// }
/// ```
pub fn compute_sticky_offset(
    scoped: &mut ScopedDb,
    axis: Axis,
    normal_offset: Subpixels,
    scroll_state: &ScrollState,
) -> Subpixels {
    use rewrite_css::{EndMarker, StartMarker};

    // Get sticky threshold from top/left properties
    let threshold = if std::any::TypeId::of::<Axis>() == std::any::TypeId::of::<Axis>() {
        match axis {
            Axis::Block => {
                scoped.query::<PositionOffsetQuery<rewrite_css::BlockMarker, StartMarker>>()
            }
            Axis::Inline => {
                scoped.query::<PositionOffsetQuery<rewrite_css::InlineMarker, StartMarker>>()
            }
        }
    } else {
        0
    };

    // If no threshold specified, behave as relative positioning
    if threshold == 0 {
        return normal_offset;
    }

    // Find the scroll container (nearest ancestor with overflow: scroll/auto)
    let scroll_container = find_scroll_container(scoped);
    let Some(scroll_node) = scroll_container else {
        // No scroll container, behave as relative positioning
        return normal_offset;
    };

    // Get scroll container's offset and size
    let container_offset = match axis {
        Axis::Block => scoped.node_query::<OffsetQuery<BlockMarker, StaticMarker>>(scroll_node),
        Axis::Inline => scoped.node_query::<OffsetQuery<InlineMarker, StaticMarker>>(scroll_node),
    };

    // Get current scroll position
    let scroll_offset = match axis {
        Axis::Block => scroll_state.block_scroll,
        Axis::Inline => scroll_state.inline_scroll,
    };

    // Calculate the viewport-relative position where the element should stick
    let stick_position = container_offset + threshold + scroll_offset;

    // Get containing block boundaries
    let containing_block = find_containing_block_for_sticky(scoped);
    let cb_end = if let Some(cb_node) = containing_block {
        let cb_offset = match axis {
            Axis::Block => scoped.node_query::<OffsetQuery<BlockMarker, StaticMarker>>(cb_node),
            Axis::Inline => scoped.node_query::<OffsetQuery<InlineMarker, StaticMarker>>(cb_node),
        };
        let cb_size = match axis {
            Axis::Block => scoped.node_query::<SizeQuery<BlockMarker, ConstrainedMarker>>(cb_node),
            Axis::Inline => {
                scoped.node_query::<SizeQuery<InlineMarker, ConstrainedMarker>>(cb_node)
            }
        };
        cb_offset + cb_size
    } else {
        Subpixels::MAX
    };

    let element_size = match axis {
        Axis::Block => scoped.query::<SizeQuery<BlockMarker, ConstrainedMarker>>(),
        Axis::Inline => scoped.query::<SizeQuery<InlineMarker, ConstrainedMarker>>(),
    };

    // Apply sticky positioning logic:
    // 1. If normal position is below stick position: use normal position (not scrolled enough)
    // 2. If stick position + element size exceeds containing block end: unstick (reached boundary)
    // 3. Otherwise: stick at stick position

    if normal_offset >= stick_position {
        // Not scrolled enough, use normal position
        normal_offset
    } else if stick_position + element_size > cb_end {
        // Would exceed containing block, clamp to boundary
        cb_end - element_size
    } else {
        // Stick to threshold position
        stick_position
    }
}

/// Find the nearest scrolling container (ancestor with overflow: scroll or auto).
///
/// A scrolling container is an element with:
/// - `overflow: scroll`, `overflow: auto`, or
/// - `overflow-x/overflow-y: scroll/auto`
///
/// If no scrolling container is found, returns None (uses viewport as scroll container).
fn find_scroll_container(scoped: &mut ScopedDb) -> Option<NodeId> {
    let mut current = scoped.parent_id();

    while let Some(node) = current {
        let overflow = scoped.node_query::<OverflowQuery>(node);

        if is_scroll_container_overflow(&overflow) {
            return Some(node);
        }

        current = scoped.node_parent(node);
    }

    None
}

/// Check if an overflow value creates a scrolling container.
fn is_scroll_container_overflow(overflow: &CssValue) -> bool {
    matches!(
        overflow,
        CssValue::Keyword(CssKeyword::Scroll) | CssValue::Keyword(CssKeyword::Auto)
    )
}

/// Find the containing block for sticky positioning.
///
/// For sticky positioning, the containing block is the nearest ancestor
/// block-level box. This is different from absolute positioning, which uses
/// the nearest positioned ancestor.
fn find_containing_block_for_sticky(scoped: &mut ScopedDb) -> Option<NodeId> {
    // For sticky, the containing block is the parent block container
    scoped.parent_id()
}

/// Check if an element has sticky positioning with a valid threshold.
pub fn has_sticky_threshold(scoped: &mut ScopedDb, axis: Axis) -> bool {
    use rewrite_css::{PositionQuery, StartMarker};

    let position = scoped.query::<PositionQuery>();
    if !matches!(position, CssValue::Keyword(CssKeyword::Sticky)) {
        return false;
    }

    let threshold = match axis {
        Axis::Block => scoped.query::<PositionOffsetQuery<rewrite_css::BlockMarker, StartMarker>>(),
        Axis::Inline => {
            scoped.query::<PositionOffsetQuery<rewrite_css::InlineMarker, StartMarker>>()
        }
    };

    threshold != 0
}

/// Get the current scroll state for the viewport or a scrolling container.
///
/// This is a placeholder that returns zero scroll. In a real implementation,
/// this would be provided by:
/// - Window scroll position (for viewport scrolling)
/// - Element scroll position (for overflow containers)
/// - Integration with the event loop / windowing system
///
/// TODO: Connect to actual scroll state from the windowing system or a
/// ScrollStateInput query.
pub fn get_scroll_state(_scoped: &mut ScopedDb, _container: Option<NodeId>) -> ScrollState {
    // Placeholder: return no scroll
    ScrollState {
        block_scroll: 0,
        inline_scroll: 0,
    }
}

/// Calculate sticky boundaries for visualization or debugging.
///
/// Returns (min_offset, max_offset) - the range where the element can stick.
pub fn calculate_sticky_boundaries(
    scoped: &mut ScopedDb,
    axis: Axis,
    normal_offset: Subpixels,
) -> (Subpixels, Subpixels) {
    use rewrite_css::StartMarker;

    let threshold = match axis {
        Axis::Block => scoped.query::<PositionOffsetQuery<rewrite_css::BlockMarker, StartMarker>>(),
        Axis::Inline => {
            scoped.query::<PositionOffsetQuery<rewrite_css::InlineMarker, StartMarker>>()
        }
    };

    let scroll_container = find_scroll_container(scoped);
    let container_offset = if let Some(scroll_node) = scroll_container {
        match axis {
            Axis::Block => scoped.node_query::<OffsetQuery<BlockMarker, StaticMarker>>(scroll_node),
            Axis::Inline => {
                scoped.node_query::<OffsetQuery<InlineMarker, StaticMarker>>(scroll_node)
            }
        }
    } else {
        0
    };

    let min_offset = container_offset + threshold;

    let containing_block = find_containing_block_for_sticky(scoped);
    let max_offset = if let Some(cb_node) = containing_block {
        let cb_offset = match axis {
            Axis::Block => scoped.node_query::<OffsetQuery<BlockMarker, StaticMarker>>(cb_node),
            Axis::Inline => scoped.node_query::<OffsetQuery<InlineMarker, StaticMarker>>(cb_node),
        };
        let cb_size = match axis {
            Axis::Block => scoped.node_query::<SizeQuery<BlockMarker, ConstrainedMarker>>(cb_node),
            Axis::Inline => {
                scoped.node_query::<SizeQuery<InlineMarker, ConstrainedMarker>>(cb_node)
            }
        };
        let element_size = match axis {
            Axis::Block => scoped.query::<SizeQuery<BlockMarker, ConstrainedMarker>>(),
            Axis::Inline => scoped.query::<SizeQuery<InlineMarker, ConstrainedMarker>>(),
        };
        cb_offset + cb_size - element_size
    } else {
        Subpixels::MAX
    };

    (min_offset.max(normal_offset), max_offset)
}

// ============================================================================
// Integration with Position Module
// ============================================================================

/// Check if sticky positioning is active (currently sticking).
///
/// This is useful for rendering or debugging to know if the element is
/// currently in its sticky state vs. normal flow.
pub fn is_currently_sticking(
    scoped: &mut ScopedDb,
    axis: Axis,
    current_offset: Subpixels,
    normal_offset: Subpixels,
) -> bool {
    current_offset != normal_offset && has_sticky_threshold(scoped, axis)
}

/// Get the effective scroll container for an element.
///
/// This is the ancestor that determines sticky positioning behavior.
pub fn get_sticky_scroll_container(scoped: &mut ScopedDb) -> Option<NodeId> {
    find_scroll_container(scoped)
}
