//! Offset query - returns formulas for computing absolute position along an axis.
//!
//! Each node's absolute offset = parent's absolute offset + parent's padding
//! + parent's border + local offset within the parent's content area.
//!
//! The local offset is determined by the parent's layout mode (block/flex/grid).
//!
//! Positioned elements override this:
//! - `relative`: normal flow offset + top/left
//! - `absolute`: containing block offset + top/left (containing block = nearest positioned ancestor)
//! - `fixed`: viewport origin + top/left (or right→x for right-anchored)

use lightningcss::properties::position::Position;
use lightningcss::properties::{Property, PropertyId};
use rewrite_core::{Axis, Formula, SingleRelationship, StylerAccess, Subpixel};

use super::DisplayType;

/// Determine the CSS `position` value for a node.
fn position_of(styler: &dyn StylerAccess) -> Position {
    match styler.get_css_property(&PropertyId::Position) {
        Some(Property::Position(pos)) => pos,
        _ => Position::Static,
    }
}

/// Check if a node is a positioned ancestor (position != static).
fn is_positioned(styler: &dyn StylerAccess) -> bool {
    !matches!(position_of(styler), Position::Static)
}

/// Query function that returns a formula for the node's absolute position.
///
/// For `position: static` (default):
///   absolute_offset = parent.absolute_offset + parent.padding + parent.border + local_offset
///
/// For `position: relative`:
///   same as static, plus top/left offset
///
/// For `position: absolute`:
///   containing_block.offset + containing_block.padding + containing_block.border + top/left
///
/// For `position: fixed`:
///   top/left from viewport origin (or right→x computed from viewport width)
///
/// For root-level nodes (no parent display), returns constant 0.
pub fn offset_query(styler: &dyn StylerAccess, axis: Axis) -> Option<&'static Formula> {
    let pos = position_of(styler);

    match pos {
        Position::Fixed => Some(fixed_offset(styler, axis)),
        Position::Absolute => Some(absolute_offset(styler, axis)),
        Position::Relative => Some(relative_offset(styler, axis)),
        Position::Sticky(_) => sticky_offset(styler, axis),
        _ => static_offset(styler, axis),
    }
}

/// Normal flow offset (position: static).
fn static_offset(styler: &dyn StylerAccess, axis: Axis) -> Option<&'static Formula> {
    let parent = styler.related(SingleRelationship::Parent);
    if parent.node_id() == styler.node_id()
        || parent.get_css_property(&PropertyId::Display).is_none()
    {
        return Some(constant!(Subpixel::ZERO));
    }

    match axis {
        Axis::Horizontal => Some(add!(
            related!(Parent, offset_query, Axis::Horizontal),
            related_val!(Parent, css_prop!(PaddingLeft)),
            related_val!(Parent, css_prop!(BorderLeftWidth)),
            related!(Self_, local_offset_query, Axis::Horizontal),
        )),
        Axis::Vertical => Some(add!(
            related!(Parent, offset_query, Axis::Vertical),
            related_val!(Parent, css_prop!(PaddingTop)),
            related_val!(Parent, css_prop!(BorderTopWidth)),
            related!(Self_, local_offset_query, Axis::Vertical),
        )),
    }
}

/// Relative positioning: normal flow + top/left offset.
///
/// Uses a self-referential query: resolves `static_offset_query` for the
/// current node (which computes the normal flow position), then adds top/left.
fn relative_offset(_styler: &dyn StylerAccess, axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => add!(
            related!(Self_, static_offset_query, Axis::Horizontal),
            css_prop!(Left),
        ),
        Axis::Vertical => add!(
            related!(Self_, static_offset_query, Axis::Vertical),
            css_prop!(Top),
        ),
    }
}

/// Query function that always computes the static (normal flow) offset,
/// ignoring the element's own `position` property. Used by `relative_offset`
/// to get the base position before adding top/left.
fn static_offset_query(styler: &dyn StylerAccess, axis: Axis) -> Option<&'static Formula> {
    static_offset(styler, axis)
}

/// Sticky positioning: normal flow position (CSS Position §2.3).
///
/// A sticky element occupies its normal flow position. The inset values
/// (top/left/right/bottom) define scroll-triggered constraints that only
/// take effect when the element's scroll container is actively scrolled.
/// Without scroll state, sticky is identical to static positioning.
///
/// Full scroll-aware clamping will be added when the scroll infrastructure
/// feeds scroll offsets into the formula resolver.
fn sticky_offset(styler: &dyn StylerAccess, axis: Axis) -> Option<&'static Formula> {
    static_offset(styler, axis)
}

/// Fixed positioning: offset from viewport origin.
fn fixed_offset(styler: &dyn StylerAccess, axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => {
            // If `left` is set, use it. If `right` is set, compute from viewport width.
            if styler.get_css_property(&PropertyId::Left).is_some() {
                css_prop!(Left)
            } else if styler.get_css_property(&PropertyId::Right).is_some() {
                sub!(
                    viewport_width!(),
                    css_prop!(Right),
                    related!(Self_, super::size::size_query, Axis::Horizontal),
                )
            } else {
                constant!(Subpixel::ZERO)
            }
        }
        Axis::Vertical => {
            if styler.get_css_property(&PropertyId::Top).is_some() {
                css_prop!(Top)
            } else if styler.get_css_property(&PropertyId::Bottom).is_some() {
                sub!(
                    viewport_height!(),
                    css_prop!(Bottom),
                    related!(Self_, super::size::size_query, Axis::Vertical),
                )
            } else {
                constant!(Subpixel::ZERO)
            }
        }
    }
}

/// Absolute positioning: offset from the nearest positioned ancestor.
///
/// Walks up the parent chain to find the containing block (nearest ancestor
/// with position != static). Falls back to the viewport if none found.
fn absolute_offset(styler: &dyn StylerAccess, axis: Axis) -> &'static Formula {
    // Check if the parent is the containing block (positioned).
    let parent = styler.related(SingleRelationship::Parent);
    let parent_is_root = parent.node_id() == styler.node_id()
        || parent.get_css_property(&PropertyId::Display).is_none();

    if parent_is_root {
        // No positioned ancestor → position from viewport origin
        return match axis {
            Axis::Horizontal => css_prop!(Left),
            Axis::Vertical => css_prop!(Top),
        };
    }

    if is_positioned(parent.as_ref()) {
        // Parent is the containing block
        return match axis {
            Axis::Horizontal => add!(
                related!(Parent, offset_query, Axis::Horizontal),
                related_val!(Parent, css_prop!(PaddingLeft)),
                related_val!(Parent, css_prop!(BorderLeftWidth)),
                css_prop!(Left),
            ),
            Axis::Vertical => add!(
                related!(Parent, offset_query, Axis::Vertical),
                related_val!(Parent, css_prop!(PaddingTop)),
                related_val!(Parent, css_prop!(BorderTopWidth)),
                css_prop!(Top),
            ),
        };
    }

    // Parent is not positioned — the containing block is further up.
    // Use the parent's offset query to "pass through" the parent's
    // contribution, then add the absolute position offsets.
    // Since we can't walk arbitrary ancestors in a static formula tree,
    // we propagate through the parent's offset and add top/left.
    match axis {
        Axis::Horizontal => add!(
            related!(Parent, offset_query, Axis::Horizontal),
            related_val!(Parent, css_prop!(PaddingLeft)),
            related_val!(Parent, css_prop!(BorderLeftWidth)),
            css_prop!(Left),
        ),
        Axis::Vertical => add!(
            related!(Parent, offset_query, Axis::Vertical),
            related_val!(Parent, css_prop!(PaddingTop)),
            related_val!(Parent, css_prop!(BorderTopWidth)),
            css_prop!(Top),
        ),
    }
}

/// Local offset for an inline child within a block parent.
///
/// Inline children flow horizontally: x = sum of previous siblings' widths,
/// y = 0 (single-line simplification).
fn inline_child_offset(axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => {
            aggregate!(Sum, PrevSiblings, super::size::size_query, Axis::Horizontal)
        }
        Axis::Vertical => constant!(Subpixel::ZERO),
    }
}

/// Local offset within parent's content area, based on parent's layout mode.
///
/// If the child is an inline element inside a block parent, use inline
/// flow positioning (horizontal stacking) instead of the parent's
/// default block stacking.
///
/// If the child is a block element inside an inline parent (block-in-inline,
/// CSS 2.2 §9.2.1.1), use block stacking relative to the nearest block
/// container ancestor.
fn local_offset_query(styler: &dyn StylerAccess, axis: Axis) -> Option<&'static Formula> {
    // Block-in-inline: position as a block child of the block container.
    if super::is_block_in_inline(styler) {
        return Some(super::block::block_offset(styler, axis));
    }

    let parent = styler.related(SingleRelationship::Parent);
    let parent_display = DisplayType::of(parent.as_ref())?;

    // Inline parent containing block children: the inline acts as a block
    // container, so children use block stacking (CSS 2.2 §9.2.1.1).
    if matches!(parent_display, DisplayType::Inline)
        && super::inline_contains_block(parent.as_ref())
    {
        return Some(super::block::block_offset(styler, axis));
    }

    // Check if this child is inline within a block parent.
    // If the inline contains block children, it's treated as block-level
    // and uses block offset instead of inline flow (CSS 2.2 §9.2.1.1).
    if matches!(parent_display, DisplayType::Block) && !styler.is_intrinsic() {
        if let Some(DisplayType::Inline) = DisplayType::of_element(styler) {
            if !super::inline_contains_block(styler) {
                return Some(inline_child_offset(axis));
            }
        }
    }

    parent_display.offset(styler, axis)
}
