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
use rewrite_core::{Axis, Formula, NodeId, PropertyResolver, Subpixel};

use super::DisplayType;

/// Determine the CSS `position` value for a node.
fn position_of(node: NodeId, ctx: &dyn PropertyResolver) -> Position {
    match ctx.get_css_property(node, &PropertyId::Position) {
        Some(Property::Position(pos)) => pos,
        _ => Position::Static,
    }
}

/// Check if a node is a positioned ancestor (position != static).
fn is_positioned(node: NodeId, ctx: &dyn PropertyResolver) -> bool {
    !matches!(position_of(node, ctx), Position::Static)
}

/// Query function that returns a formula for the node's absolute position.
pub fn offset_query(
    node: NodeId,
    ctx: &dyn PropertyResolver,
    axis: Axis,
) -> Option<&'static Formula> {
    let pos = position_of(node, ctx);

    match pos {
        Position::Fixed => Some(fixed_offset(node, ctx, axis)),
        Position::Absolute => Some(absolute_offset(node, ctx, axis)),
        Position::Relative => Some(relative_offset(node, ctx, axis)),
        Position::Sticky(_) => sticky_offset(node, ctx, axis),
        _ => static_offset(node, ctx, axis),
    }
}

/// Normal flow offset (position: static).
fn static_offset(
    node: NodeId,
    ctx: &dyn PropertyResolver,
    axis: Axis,
) -> Option<&'static Formula> {
    let parent = ctx.parent(node);
    if parent.is_none() || parent == Some(rewrite_core::NodeId::ROOT) || parent == Some(node) {
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
fn relative_offset(
    _node: NodeId,
    _ctx: &dyn PropertyResolver,
    axis: Axis,
) -> &'static Formula {
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
/// ignoring the element's own `position` property.
fn static_offset_query(
    node: NodeId,
    ctx: &dyn PropertyResolver,
    axis: Axis,
) -> Option<&'static Formula> {
    static_offset(node, ctx, axis)
}

/// Sticky positioning: normal flow position (CSS Position §2.3).
fn sticky_offset(
    node: NodeId,
    ctx: &dyn PropertyResolver,
    axis: Axis,
) -> Option<&'static Formula> {
    static_offset(node, ctx, axis)
}

/// Fixed positioning: offset from viewport origin.
fn fixed_offset(
    node: NodeId,
    ctx: &dyn PropertyResolver,
    axis: Axis,
) -> &'static Formula {
    match axis {
        Axis::Horizontal => {
            if ctx.get_css_property(node, &PropertyId::Left).is_some() {
                css_prop!(Left)
            } else if ctx.get_css_property(node, &PropertyId::Right).is_some() {
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
            if ctx.get_css_property(node, &PropertyId::Top).is_some() {
                css_prop!(Top)
            } else if ctx.get_css_property(node, &PropertyId::Bottom).is_some() {
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
fn absolute_offset(
    node: NodeId,
    ctx: &dyn PropertyResolver,
    axis: Axis,
) -> &'static Formula {
    let parent = ctx.parent(node);
    let parent_is_root = parent.is_none()
        || parent == Some(node)
        || parent == Some(rewrite_core::NodeId::ROOT);

    if parent_is_root {
        return match axis {
            Axis::Horizontal => css_prop!(Left),
            Axis::Vertical => css_prop!(Top),
        };
    }

    let parent_id = parent.unwrap_or(NodeId(0));
    if is_positioned(parent_id, ctx) {
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

    // Parent is not positioned — pass through parent's contribution.
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
fn inline_child_offset(axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => {
            aggregate!(Sum, PrevSiblings, super::size::size_query, Axis::Horizontal)
        }
        Axis::Vertical => constant!(Subpixel::ZERO),
    }
}

/// Local offset within parent's content area, based on parent's layout mode.
fn local_offset_query(
    node: NodeId,
    ctx: &dyn PropertyResolver,
    axis: Axis,
) -> Option<&'static Formula> {
    // Block-in-inline: position as a block child of the block container.
    if super::is_block_in_inline(node, ctx) {
        return Some(super::block::block_offset(node, ctx, axis));
    }

    let parent = ctx.parent(node).unwrap_or(NodeId(0));
    let parent_display = DisplayType::of(parent, ctx)?;

    // Inline parent containing block children: the inline acts as a block
    // container, so children use block stacking (CSS 2.2 §9.2.1.1).
    if matches!(parent_display, DisplayType::Inline)
        && super::inline_contains_block(parent, ctx)
    {
        return Some(super::block::block_offset(node, ctx, axis));
    }

    // Check if this child is inline within a block parent.
    if matches!(parent_display, DisplayType::Block) && !ctx.is_intrinsic(node) {
        if let Some(DisplayType::Inline) = DisplayType::of_element(node, ctx) {
            if !super::inline_contains_block(node, ctx) {
                return Some(inline_child_offset(axis));
            }
        }
    }

    parent_display.offset(node, ctx, axis)
}
