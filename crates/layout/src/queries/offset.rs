//! Offset query - returns formulas for computing position offset along an axis.
//!
//! Ported from `crates/layout/crates/offset/src/lib.rs`.
//! The offset is determined by the parent's layout mode — the parent decides
//! how its children are positioned.

use lightningcss::properties::PropertyId;
use lightningcss::properties::display::{Display, DisplayKeyword};
use lightningcss::properties::flex::FlexDirection;
use lightningcss::vendor_prefix::VendorPrefix;
use rewrite_core::{Axis, SingleRelationship, StylerAccess};
use rewrite_css::{Formula, NodeStylerContext};

/// Query function that returns an offset formula based on parent's display property.
///
/// Returns `None` if the parent's display property isn't available yet.
pub fn offset_query(styler: &NodeStylerContext<'_>, axis: Axis) -> Option<&'static Formula> {
    // Navigate to parent
    let parent = styler.related(SingleRelationship::Parent);
    let parent_display = parent.get_css_property(&PropertyId::Display)?;

    match parent_display {
        lightningcss::properties::Property::Display(display) => match display {
            Display::Keyword(DisplayKeyword::None) => {
                static ZERO: Formula = Formula::Constant(0);
                Some(&ZERO)
            }
            Display::Pair(pair) => match pair.inside {
                lightningcss::properties::display::DisplayInside::Flex(_) => {
                    // Read parent's flex-direction for proper axis dispatch
                    let flex_dir = match parent
                        .get_css_property(&PropertyId::FlexDirection(VendorPrefix::None))
                    {
                        Some(lightningcss::properties::Property::FlexDirection(dir, _)) => *dir,
                        _ => FlexDirection::Row,
                    };
                    Some(super::flex::flex_offset(flex_dir, axis))
                }
                lightningcss::properties::display::DisplayInside::Grid => {
                    super::grid::grid_offset(&parent, axis)
                }
                lightningcss::properties::display::DisplayInside::Flow
                | lightningcss::properties::display::DisplayInside::FlowRoot => {
                    Some(super::block::block_offset(axis))
                }
                _ => Some(super::block::block_offset(axis)),
            },
            _ => Some(super::block::block_offset(axis)),
        },
        _ => None,
    }
}

/// `QueryFn`-compatible wrapper for horizontal offset query.
pub fn offset_query_horizontal(styler: &NodeStylerContext<'_>) -> Option<&'static Formula> {
    offset_query(styler, Axis::Horizontal)
}

/// `QueryFn`-compatible wrapper for vertical offset query.
pub fn offset_query_vertical(styler: &NodeStylerContext<'_>) -> Option<&'static Formula> {
    offset_query(styler, Axis::Vertical)
}
