//! Size query - returns formulas for computing size along an axis.
//!
//! Dispatches to block/flex/grid modules based on the element's display mode.

use lightningcss::properties::PropertyId;
use rewrite_core::{Axis, Formula, SingleRelationship, StylerAccess};

use super::DisplayType;

/// Query function that returns a size formula based on the display property.
/// Returns `None` if the display property isn't available yet.
pub fn size_query(styler: &dyn StylerAccess, axis: Axis) -> Option<&'static Formula> {
    // Intrinsic nodes (text nodes): return InlineWidth/InlineHeight so the
    // resolver's inline aggregation handles text measurement and line breaking.
    if styler.is_intrinsic() {
        DisplayType::of(styler)?;
        return match axis {
            Axis::Horizontal => Some(inline_width!()),
            Axis::Vertical => Some(inline_height!()),
        };
    }

    // Resolve display type for elements.
    let display_type = DisplayType::of(styler);

    // CSS 2.2 §10.2 / §10.5: width and height do not apply to
    // non-replaced inline elements. Skip the explicit CSS check
    // so inline elements size from their content.
    let is_inline = matches!(display_type, Some(DisplayType::Inline));

    // Root element check: use viewport dimensions if this node is at the top
    // of the layout tree. The root is detected by checking if the parent is
    // NodeId(0) (the DOM root) or if the node is its own parent.
    let parent = styler.related(SingleRelationship::Parent);
    let parent_is_document_root = parent.node_id() == rewrite_core::NodeId::ROOT;
    let node_is_own_parent = parent.node_id() == styler.node_id();
    if parent_is_document_root || node_is_own_parent {
        return match axis {
            Axis::Horizontal => Some(viewport_width!()),
            Axis::Vertical => Some(viewport_height!()),
        };
    }

    // Flex item detection: if the parent is a flex container, this
    // element is a flex item and should be sized by the flex algorithm.
    // The flex algorithm handles explicit CSS size via flex-basis.
    // Per CSS Flexbox §4.1, absolutely/fixed positioned children are
    // out-of-flow and do not participate in flex layout sizing.
    let parent_display = DisplayType::of_element(parent.as_ref());
    if let Some(DisplayType::Flex(dir, _)) = parent_display {
        let is_out_of_flow = matches!(
            styler.get_css_property(&PropertyId::Position),
            Some(lightningcss::properties::Property::Position(
                lightningcss::properties::position::Position::Absolute
                    | lightningcss::properties::position::Position::Fixed
            ))
        );
        if !is_out_of_flow {
            return Some(super::flex::flex_item_size(dir, axis));
        }
    }

    // Check for explicit size (CSS width/height).
    if !is_inline {
        let explicit_prop = match axis {
            Axis::Horizontal => PropertyId::Width,
            Axis::Vertical => PropertyId::Height,
        };
        if let Some(prop) = styler.get_css_property(&explicit_prop) {
            // Handle keyword sizes (min-content, max-content) which cannot
            // be resolved to a numeric value by css_val!.
            if let Some(keyword_formula) = keyword_size_formula(prop, styler, axis) {
                return Some(keyword_formula);
            }
            return match axis {
                Axis::Horizontal => Some(css_val!(Width)),
                Axis::Vertical => Some(css_val!(Height)),
            };
        }
    }

    // Inline element containing a block child: per CSS 2.2 §9.2.1.1,
    // the inline is broken around the block and treated as block-level
    // for sizing purposes (fills parent content width).
    if matches!(display_type, Some(DisplayType::Inline)) && super::inline_contains_block(styler) {
        return Some(super::block::block_size(styler, axis));
    }

    display_type?.size(styler, axis)
}

/// Content-area size = border-box size minus padding and border.
pub fn content_size_query(_styler: &dyn StylerAccess, axis: Axis) -> Option<&'static Formula> {
    match axis {
        Axis::Horizontal => Some(sub!(
            related!(Self_, size_query, Axis::Horizontal),
            css_prop!(PaddingLeft),
            css_prop!(PaddingRight),
            css_prop!(BorderLeftWidth),
            css_prop!(BorderRightWidth),
        )),
        Axis::Vertical => Some(sub!(
            related!(Self_, size_query, Axis::Vertical),
            css_prop!(PaddingTop),
            css_prop!(PaddingBottom),
            css_prop!(BorderTopWidth),
            css_prop!(BorderBottomWidth),
        )),
    }
}

/// Resolve keyword size values (`min-content`, `max-content`) to intrinsic
/// sizing formulas based on the element's display type.
///
/// Returns `None` if the property is not a keyword size (e.g. a length),
/// in which case the caller should fall back to `css_val!`.
fn keyword_size_formula(
    prop: lightningcss::properties::Property<'static>,
    styler: &dyn StylerAccess,
    axis: Axis,
) -> Option<&'static Formula> {
    use lightningcss::properties::Property::{Height, Width};
    use lightningcss::properties::size::Size;

    let size = match prop {
        Width(s) | Height(s) => s,
        _ => return None,
    };

    match size {
        Size::MinContent(_) => {
            if let Some(DisplayType::Flex(dir, _)) = DisplayType::of_element(styler) {
                Some(super::flex::flex_min_content_size(dir, axis, styler))
            } else {
                // For non-flex containers, use the inline min-content measurement.
                Some(match axis {
                    Axis::Horizontal => add!(
                        min_content_width!(),
                        css_prop!(PaddingLeft),
                        css_prop!(PaddingRight),
                        css_prop!(BorderLeftWidth),
                        css_prop!(BorderRightWidth),
                    ),
                    // Vertical min-content = auto height (content height).
                    Axis::Vertical => {
                        return DisplayType::of_element(styler)
                            .and_then(|dt| dt.size(styler, axis));
                    }
                })
            }
        }
        Size::MaxContent(_) => {
            if let Some(DisplayType::Flex(dir, _)) = DisplayType::of_element(styler) {
                Some(super::flex::flex_size(dir, axis, styler))
            } else {
                Some(match axis {
                    Axis::Horizontal => add!(
                        max_content_width!(),
                        css_prop!(PaddingLeft),
                        css_prop!(PaddingRight),
                        css_prop!(BorderLeftWidth),
                        css_prop!(BorderRightWidth),
                    ),
                    // Vertical max-content = auto height (content height).
                    Axis::Vertical => {
                        return DisplayType::of_element(styler)
                            .and_then(|dt| dt.size(styler, axis));
                    }
                })
            }
        }
        _ => None,
    }
}

/// Margin-box size = border-box size + margins.
pub fn margin_box_size_query(_styler: &dyn StylerAccess, axis: Axis) -> Option<&'static Formula> {
    match axis {
        Axis::Horizontal => Some(add!(
            related!(Self_, size_query, Axis::Horizontal),
            css_prop!(MarginLeft),
            css_prop!(MarginRight),
        )),
        Axis::Vertical => Some(add!(
            related!(Self_, size_query, Axis::Vertical),
            css_prop!(MarginTop),
            css_prop!(MarginBottom),
        )),
    }
}
