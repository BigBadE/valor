//! Offset query - returns formulas for computing position offset along an axis.
//!
//! The offset system uses the formula-based approach where Query functions
//! return static formulas that describe how to compute the offset.
//!
//! Query functions return `Option<&'static Formula>` - returning `None` when
//! the required CSS properties aren't available yet (low confidence).

use lightningcss::properties::PropertyId;
use lightningcss::properties::display::{Display, DisplayKeyword};
use rewrite_core::{Axis, Formula};
use rewrite_css::ScopedStyler;

/// Offset mode enumeration - re-exported from offset_impl.
pub use rewrite_layout_offset_impl::OffsetMode;

/// Query function that returns an offset formula based on parent's display property.
///
/// The offset is computed based on the parent's layout mode, since the parent
/// determines how its children are positioned.
///
/// Returns `None` if the parent's display property isn't available yet.
pub fn offset_query(styler: &ScopedStyler<'_>, axis: Axis) -> Option<&'static Formula> {
    // Get parent - if no parent, we can't determine offset
    let parent = styler.parent()?;
    let parent_display = parent.get_css_property(&PropertyId::Display)?;

    match parent_display {
        lightningcss::properties::Property::Display(display) => match display {
            Display::Keyword(DisplayKeyword::None) => {
                // Parent is display: none - offset doesn't matter
                static ZERO: Formula = Formula::Constant(0);
                Some(&ZERO)
            }
            Display::Pair(pair) => match pair.inside {
                lightningcss::properties::display::DisplayInside::Flex(_) => {
                    rewrite_layout_flex::flex_offset(styler, axis)
                }
                lightningcss::properties::display::DisplayInside::Grid => {
                    rewrite_layout_grid::grid_offset(styler, axis)
                }
                lightningcss::properties::display::DisplayInside::Flow
                | lightningcss::properties::display::DisplayInside::FlowRoot => {
                    Some(rewrite_layout_block::block_offset(axis))
                }
                _ => Some(rewrite_layout_block::block_offset(axis)),
            },
            _ => Some(rewrite_layout_block::block_offset(axis)),
        },
        _ => None,
    }
}
