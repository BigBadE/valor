//! Size query - returns formulas for computing size along an axis.
//!
//! The size system uses the formula-based approach where Query functions
//! return static formulas that describe how to compute the size, rather
//! than computing values directly.
//!
//! Query functions return `Option<&'static Formula>` - returning `None` when
//! the required CSS properties aren't available yet (low confidence).

use lightningcss::properties::PropertyId;
use lightningcss::properties::display::{Display, DisplayKeyword};
use rewrite_core::{Axis, Formula};
use rewrite_css::ScopedStyler;

/// Size mode enumeration - re-exported from size_impl.
pub use rewrite_layout_size_impl::SizeMode;

/// Query function that returns a size formula based on the display property.
/// Returns `None` if the display property isn't available yet.
pub fn size_query(styler: &ScopedStyler<'_>, axis: Axis) -> Option<&'static Formula> {
    let display = styler.get_css_property(&PropertyId::Display)?;

    match display {
        lightningcss::properties::Property::Display(display) => match display {
            Display::Keyword(DisplayKeyword::None) => {
                // display: none - size is 0
                static ZERO: Formula = Formula::Constant(0);
                Some(&ZERO)
            }
            Display::Pair(pair) => match pair.inside {
                lightningcss::properties::display::DisplayInside::Flex(_) => {
                    rewrite_layout_flex::flex_size(styler, axis)
                }
                lightningcss::properties::display::DisplayInside::Grid => {
                    rewrite_layout_grid::grid_size(styler, axis)
                }
                lightningcss::properties::display::DisplayInside::Flow
                | lightningcss::properties::display::DisplayInside::FlowRoot => {
                    Some(rewrite_layout_block::block_size(axis))
                }
                _ => Some(rewrite_layout_block::block_size(axis)),
            },
            _ => Some(rewrite_layout_block::block_size(axis)),
        },
        _ => None,
    }
}

/// Query function for intrinsic minimum size.
/// Returns `None` if the display property isn't available yet.
pub fn min_content_size_query(styler: &ScopedStyler<'_>, axis: Axis) -> Option<&'static Formula> {
    let display = styler.get_css_property(&PropertyId::Display)?;

    match display {
        lightningcss::properties::Property::Display(display) => match display {
            Display::Keyword(DisplayKeyword::None) => {
                static ZERO: Formula = Formula::Constant(0);
                Some(&ZERO)
            }
            Display::Pair(pair) => match pair.inside {
                lightningcss::properties::display::DisplayInside::Flex(_) => {
                    Some(rewrite_layout_flex::flex_min_content_size(axis))
                }
                _ => Some(rewrite_layout_block::block_min_content_size(axis)),
            },
            _ => Some(rewrite_layout_block::block_min_content_size(axis)),
        },
        _ => None,
    }
}

/// Query function for intrinsic maximum size.
/// Returns `None` if the display property isn't available yet.
pub fn max_content_size_query(styler: &ScopedStyler<'_>, axis: Axis) -> Option<&'static Formula> {
    let display = styler.get_css_property(&PropertyId::Display)?;

    match display {
        lightningcss::properties::Property::Display(display) => match display {
            Display::Keyword(DisplayKeyword::None) => {
                static ZERO: Formula = Formula::Constant(0);
                Some(&ZERO)
            }
            Display::Pair(pair) => match pair.inside {
                lightningcss::properties::display::DisplayInside::Flex(_) => {
                    Some(rewrite_layout_flex::flex_max_content_size(axis))
                }
                _ => Some(rewrite_layout_block::block_max_content_size(axis)),
            },
            _ => Some(rewrite_layout_block::block_max_content_size(axis)),
        },
        _ => None,
    }
}
