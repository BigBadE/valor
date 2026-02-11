//! Size query - returns formulas for computing size along an axis.
//!
//! Ported from `crates/layout/crates/size/src/lib.rs`.
//! Key change: dispatches to block/flex/grid modules in the same crate,
//! and provides `size_query_horizontal`/`size_query_vertical` wrappers
//! that match the `QueryFn` signature for use in `Formula::Related`.

use lightningcss::properties::PropertyId;
use lightningcss::properties::display::{Display, DisplayKeyword};
use rewrite_core::{Axis, Operation, SingleRelationship, StylerAccess};
use rewrite_css::{Formula, NodeStylerContext};

/// Query function that returns a size formula based on the display property.
/// Returns `None` if the display property isn't available yet.
pub fn size_query(styler: &NodeStylerContext<'_>, axis: Axis) -> Option<&'static Formula> {
    // Check for explicit size first
    let explicit_prop = match axis {
        Axis::Horizontal => PropertyId::Width,
        Axis::Vertical => PropertyId::Height,
    };
    if styler.get_css_property(&explicit_prop).is_some() {
        // Node has an explicit size — use CssValue which the resolver will convert
        match axis {
            Axis::Horizontal => {
                static EXPLICIT_W: Formula = Formula::CssValue(PropertyId::Width);
                return Some(&EXPLICIT_W);
            }
            Axis::Vertical => {
                static EXPLICIT_H: Formula = Formula::CssValue(PropertyId::Height);
                return Some(&EXPLICIT_H);
            }
        }
    }

    // Root node check: if parent == self, use viewport dimensions
    let parent = styler.related(SingleRelationship::Parent);
    if parent.node() == styler.node() {
        return match axis {
            Axis::Horizontal => {
                static VW: Formula = Formula::ViewportWidth;
                Some(&VW)
            }
            Axis::Vertical => {
                static VH: Formula = Formula::ViewportHeight;
                Some(&VH)
            }
        };
    }

    // No explicit size — dispatch on display mode
    let display = styler.get_css_property(&PropertyId::Display)?;

    match display {
        lightningcss::properties::Property::Display(display) => match display {
            Display::Keyword(DisplayKeyword::None) => {
                static ZERO: Formula = Formula::Constant(0);
                Some(&ZERO)
            }
            Display::Pair(pair) => match pair.inside {
                lightningcss::properties::display::DisplayInside::Flex(_) => {
                    super::flex::flex_size(styler, axis)
                }
                lightningcss::properties::display::DisplayInside::Grid => {
                    super::grid::grid_size(styler, axis)
                }
                lightningcss::properties::display::DisplayInside::Flow
                | lightningcss::properties::display::DisplayInside::FlowRoot => {
                    Some(super::block::block_size(axis))
                }
                _ => Some(super::block::block_size(axis)),
            },
            _ => Some(super::block::block_size(axis)),
        },
        _ => None,
    }
}

/// `QueryFn`-compatible wrapper for horizontal size query.
///
/// This is a `fn(&NodeStylerContext<'static>) -> Option<&'static Formula>`
/// and can be stored in static `Formula::Related` variants.
pub fn size_query_horizontal(styler: &NodeStylerContext<'_>) -> Option<&'static Formula> {
    size_query(styler, Axis::Horizontal)
}

/// `QueryFn`-compatible wrapper for vertical size query.
pub fn size_query_vertical(styler: &NodeStylerContext<'_>) -> Option<&'static Formula> {
    size_query(styler, Axis::Vertical)
}

/// Content-area size = border-box size minus padding and border.
/// Used by children to determine how much space is available inside the parent.
pub fn content_size_query(styler: &NodeStylerContext<'_>, axis: Axis) -> Option<&'static Formula> {
    let _outer = size_query(styler, axis)?;

    match axis {
        Axis::Horizontal => {
            static OUTER: Formula =
                Formula::Related(SingleRelationship::Self_, size_query_horizontal);
            static PL: Formula = Formula::CssValueOrDefault(PropertyId::PaddingLeft, 0);
            static PR: Formula = Formula::CssValueOrDefault(PropertyId::PaddingRight, 0);
            static BL: Formula = Formula::CssValueOrDefault(PropertyId::BorderLeftWidth, 0);
            static BR: Formula = Formula::CssValueOrDefault(PropertyId::BorderRightWidth, 0);
            static S1: Formula = Formula::Op(Operation::Sub, &OUTER, &PL);
            static S2: Formula = Formula::Op(Operation::Sub, &S1, &PR);
            static S3: Formula = Formula::Op(Operation::Sub, &S2, &BL);
            static RESULT: Formula = Formula::Op(Operation::Sub, &S3, &BR);
            Some(&RESULT)
        }
        Axis::Vertical => {
            static OUTER: Formula =
                Formula::Related(SingleRelationship::Self_, size_query_vertical);
            static PT: Formula = Formula::CssValueOrDefault(PropertyId::PaddingTop, 0);
            static PB: Formula = Formula::CssValueOrDefault(PropertyId::PaddingBottom, 0);
            static BT: Formula = Formula::CssValueOrDefault(PropertyId::BorderTopWidth, 0);
            static BB: Formula = Formula::CssValueOrDefault(PropertyId::BorderBottomWidth, 0);
            static S1: Formula = Formula::Op(Operation::Sub, &OUTER, &PT);
            static S2: Formula = Formula::Op(Operation::Sub, &S1, &PB);
            static S3: Formula = Formula::Op(Operation::Sub, &S2, &BT);
            static RESULT: Formula = Formula::Op(Operation::Sub, &S3, &BB);
            Some(&RESULT)
        }
    }
}

/// `QueryFn`-compatible wrapper for horizontal content-area size query.
pub fn content_size_query_horizontal(styler: &NodeStylerContext<'_>) -> Option<&'static Formula> {
    content_size_query(styler, Axis::Horizontal)
}

/// `QueryFn`-compatible wrapper for vertical content-area size query.
pub fn content_size_query_vertical(styler: &NodeStylerContext<'_>) -> Option<&'static Formula> {
    content_size_query(styler, Axis::Vertical)
}
