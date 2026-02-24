//! Layout query modules - compute size and offset formulas based on display mode.
//!
//! All layout query crates are consolidated here to eliminate circular
//! dependencies. Block can reference size, size can reference block, etc.

use lightningcss::properties::PropertyId;
use lightningcss::properties::display::{Display, DisplayInside, DisplayKeyword, DisplayOutside};
use lightningcss::properties::flex::FlexDirection;
use lightningcss::vendor_prefix::VendorPrefix;
use rewrite_core::{Axis, Formula, SingleRelationship, StylerAccess};

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
    /// Determine the display type from a styler's Display property.
    ///
    /// For intrinsic nodes (text, replaced elements), checks the parent's
    /// display type instead, since intrinsic nodes don't own their layout mode.
    pub fn of(styler: &dyn StylerAccess) -> Option<Self> {
        if styler.is_intrinsic() {
            let parent = styler.related(SingleRelationship::Parent);
            return Self::of_element(parent.as_ref());
        }
        Self::of_element(styler)
    }

    /// Determine the display type from an element's own Display property.
    pub(crate) fn of_element(styler: &dyn StylerAccess) -> Option<Self> {
        let prop = styler.get_css_property(&PropertyId::Display)?;
        match prop {
            lightningcss::properties::Property::Display(display) => match display {
                Display::Keyword(DisplayKeyword::None) => None,
                Display::Pair(pair) => match pair.inside {
                    DisplayInside::Flex(_) => {
                        let dir = match styler
                            .get_css_property(&PropertyId::FlexDirection(VendorPrefix::None))
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
            _ => None,
        }
    }

    /// Return the size formula for an element with this display type.
    pub fn size(&self, styler: &dyn StylerAccess, axis: Axis) -> Option<&'static Formula> {
        match self {
            Self::Block => Some(block::block_size(styler, axis)),
            Self::Inline => Some(inline_size(axis)),
            Self::Flex(dir, is_inline) => {
                // Block-level flex containers (`display: flex`) fill parent width
                // like normal blocks. Only inline-flex uses content-based sizing.
                if !is_inline && axis == Axis::Horizontal {
                    Some(block::block_size(styler, axis))
                } else {
                    Some(flex::flex_size(*dir, axis, styler))
                }
            }
            Self::Grid => Some(grid::grid_size(axis)),
        }
    }

    /// Return the local offset formula for a child whose parent has this display type.
    pub fn offset(&self, styler: &dyn StylerAccess, axis: Axis) -> Option<&'static Formula> {
        match self {
            Self::Block => Some(block::block_offset(styler, axis)),
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
///
/// Returns InlineWidth/InlineHeight so the resolver's inline aggregation
/// handles line breaking and text measurement. The inline element's
/// dimensions are computed during aggregation by measuring its children.
fn inline_size(axis: Axis) -> &'static Formula {
    match axis {
        Axis::Horizontal => inline_width!(),
        Axis::Vertical => inline_height!(),
    }
}

/// Offset formula for children of an inline element.
///
/// Children of an inline element flow horizontally (same as inline flow).
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
/// This is the "block-in-inline" case from CSS 2.2 §9.2.1.1 that
/// requires anonymous block box generation.
pub(crate) fn is_block_in_inline(styler: &dyn StylerAccess) -> bool {
    if styler.is_intrinsic() {
        return false;
    }
    let Some(DisplayType::Block) = DisplayType::of_element(styler) else {
        return false;
    };
    let parent = styler.related(SingleRelationship::Parent);
    // Parent must be an inline flow element (not the same node, i.e. exists).
    if parent.node_id() == styler.node_id() {
        return false;
    }
    matches!(
        DisplayType::of_element(parent.as_ref()),
        Some(DisplayType::Inline)
    )
}

/// Check if an inline element contains at least one block-level child.
/// When true, the inline must be treated as block-level for sizing
/// and positioning (CSS 2.2 §9.2.1.1).
pub(crate) fn inline_contains_block(styler: &dyn StylerAccess) -> bool {
    use rewrite_core::MultiRelationship;
    let children = styler.related_iter(MultiRelationship::Children);
    children.iter().any(|child| {
        !child.is_intrinsic()
            && matches!(
                DisplayType::of_element(child.as_ref()),
                Some(DisplayType::Block)
            )
    })
}
