//! Layout query modules - compute size and offset formulas based on display mode.
//!
//! All layout query crates are consolidated here to eliminate circular
//! dependencies. Block can reference size, size can reference block, etc.

use lightningcss::properties::PropertyId;
use lightningcss::properties::display::{Display, DisplayInside, DisplayKeyword, DisplayOutside};
use lightningcss::properties::flex::FlexDirection;
use lightningcss::vendor_prefix::VendorPrefix;
use rewrite_core::{Axis, Formula, NodeId, PropertyResolver};

pub mod block;
pub mod flex;
pub mod grid;
pub mod offset;
pub mod property;
pub mod size;

pub use offset::offset_query;
pub use property::property_query;
pub use size::size_query;

/// Resolved layout mode for an element, used to dispatch size/offset queries.
pub enum DisplayType {
    Block,
    Inline,
    /// Flex container: (direction, is_inline).
    /// `is_inline` is true for `inline-flex`, false for `flex` (block-level).
    Flex(FlexDirection, bool),
    Grid,
}

impl DisplayType {
    /// Determine the display type from a node's Display property.
    ///
    /// For intrinsic nodes (text, replaced elements), checks the parent's
    /// display type instead, since intrinsic nodes don't own their layout mode.
    pub fn of(node: NodeId, ctx: &dyn PropertyResolver) -> Option<Self> {
        if ctx.is_intrinsic(node) {
            let parent = ctx.parent(node).unwrap_or(NodeId(0));
            return Self::of_element(parent, ctx);
        }
        Self::of_element(node, ctx)
    }

    /// Determine the display type from an element's own Display property.
    ///
    /// If no Display property is stored, the element is treated as `block`
    /// (the CSS initial value). Only non-block display values are stored
    /// in the database.
    pub(crate) fn of_element(node: NodeId, ctx: &dyn PropertyResolver) -> Option<Self> {
        let Some(prop) = ctx.get_css_property(node, &PropertyId::Display) else {
            // No stored Display → initial value is block.
            return Some(Self::Block);
        };
        match prop {
            lightningcss::properties::Property::Display(display) => match display {
                Display::Keyword(DisplayKeyword::None) => None,
                Display::Pair(pair) => match pair.inside {
                    DisplayInside::Flex(_) => {
                        let dir = match ctx
                            .get_css_property(node, &PropertyId::FlexDirection(VendorPrefix::None))
                        {
                            Some(lightningcss::properties::Property::FlexDirection(dir, _)) => dir,
                            _ => FlexDirection::Row,
                        };
                        let is_inline = matches!(pair.outside, DisplayOutside::Inline);
                        Some(Self::Flex(dir, is_inline))
                    }
                    DisplayInside::Grid => Some(Self::Grid),
                    DisplayInside::Flow if matches!(pair.outside, DisplayOutside::Inline) => {
                        Some(Self::Inline)
                    }
                    _ => Some(Self::Block),
                },
                _ => Some(Self::Block),
            },
            _ => Some(Self::Block),
        }
    }

    /// Return the size formula for an element with this display type.
    pub fn size(
        &self,
        node: NodeId,
        ctx: &dyn PropertyResolver,
        axis: Axis,
    ) -> Option<&'static Formula> {
        match self {
            Self::Block => Some(block::block_size(node, ctx, axis)),
            Self::Inline => Some(inline_size(axis)),
            Self::Flex(dir, is_inline) => {
                // Block-level flex containers (`display: flex`) fill parent width
                // like normal blocks. Only inline-flex uses content-based sizing.
                if !is_inline && axis == Axis::Horizontal {
                    Some(block::block_size(node, ctx, axis))
                } else {
                    Some(flex::flex_size(*dir, axis, node, ctx))
                }
            }
            Self::Grid => Some(grid::grid_size(axis)),
        }
    }

    /// Return the local offset formula for a child whose parent has this display type.
    pub fn offset(
        &self,
        node: NodeId,
        ctx: &dyn PropertyResolver,
        axis: Axis,
    ) -> Option<&'static Formula> {
        match self {
            Self::Block => Some(block::block_offset(node, ctx, axis)),
            Self::Inline => Some(inline_offset(axis)),
            Self::Flex(dir, _is_inline) => Some(flex::flex_offset(*dir, axis)),
            Self::Grid => Some(grid::grid_offset(axis)),
        }
    }
}

// ============================================================================
// Inline layout formulas
// ============================================================================

/// Size formula for an inline element.
fn inline_size(axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => inline_width!(),
        Axis::Vertical => inline_height!(),
    }
}

/// Offset formula for children of an inline element.
fn inline_offset(axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => {
            aggregate!(Sum, PrevSiblings, size::size_query, Axis::Horizontal)
        }
        Axis::Vertical => constant!(Subpixel::ZERO),
    }
}

use rewrite_core::Subpixel;

/// Check if a node is a block-level element whose parent is inline.
pub(crate) fn is_block_in_inline(node: NodeId, ctx: &dyn PropertyResolver) -> bool {
    if ctx.is_intrinsic(node) {
        return false;
    }
    let Some(DisplayType::Block) = DisplayType::of_element(node, ctx) else {
        return false;
    };
    let Some(parent) = ctx.parent(node) else {
        return false;
    };
    if parent == node {
        return false;
    }
    matches!(
        DisplayType::of_element(parent, ctx),
        Some(DisplayType::Inline)
    )
}

/// Check if an inline element contains at least one block-level child.
pub(crate) fn inline_contains_block(node: NodeId, ctx: &dyn PropertyResolver) -> bool {
    let children = ctx.children(node);
    children.iter().any(|&child| {
        !ctx.is_intrinsic(child)
            && matches!(
                DisplayType::of_element(child, ctx),
                Some(DisplayType::Block)
            )
    })
}
