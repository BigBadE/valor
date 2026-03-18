//! Size query - returns formulas for computing size along an axis.
//!
//! Dispatches to block/flex/grid modules based on the element's display mode.

use lightningcss::properties::PropertyId;
use rewrite_core::{Axis, Formula, NodeId, PropertyResolver};

use super::DisplayType;

/// Query function that returns a size formula based on the display property.
/// Returns `None` if the display property isn't available yet.
pub fn size_query(node: NodeId, ctx: &dyn PropertyResolver, axis: Axis) -> Option<&'static Formula> {
    // Intrinsic nodes (text nodes): return InlineWidth/InlineHeight so the
    // resolver's inline aggregation handles text measurement and line breaking.
    if ctx.is_intrinsic(node) {
        DisplayType::of(node, ctx)?;
        return match axis {
            Axis::Horizontal => Some(inline_width!()),
            Axis::Vertical => Some(inline_height!()),
        };
    }

    // Resolve display type for elements.
    let display_type = DisplayType::of(node, ctx);

    // CSS 2.2 §10.2 / §10.5: width and height do not apply to
    // non-replaced inline elements. Skip the explicit CSS check
    // so inline elements size from their content.
    let is_inline = matches!(display_type, Some(DisplayType::Inline));

    // Root element check: use viewport dimensions if this node is at the top
    // of the layout tree.
    let parent = ctx.parent(node);
    let parent_is_document_root = parent == Some(rewrite_core::NodeId::ROOT);
    let node_is_own_parent = parent == Some(node);
    let no_parent = parent.is_none();
    if parent_is_document_root || node_is_own_parent || no_parent {
        return match axis {
            Axis::Horizontal => Some(viewport_width!()),
            Axis::Vertical => Some(viewport_height!()),
        };
    }

    let parent_id = parent.unwrap_or(NodeId(0));

    // Flex item detection: if the parent is a flex container, this
    // element is a flex item and should be sized by the flex algorithm.
    let parent_display = DisplayType::of_element(parent_id, ctx);
    if let Some(DisplayType::Flex(dir, _)) = parent_display {
        let is_out_of_flow = matches!(
            ctx.get_css_property(node, &PropertyId::Position),
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
        if let Some(prop) = ctx.get_css_property(node, &explicit_prop) {
            // Handle keyword sizes (min-content, max-content) which cannot
            // be resolved to a numeric value by css_val!.
            if let Some(keyword_formula) = keyword_size_formula(prop, node, ctx, axis) {
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
    if matches!(display_type, Some(DisplayType::Inline))
        && super::inline_contains_block(node, ctx)
    {
        return Some(super::block::block_size(node, ctx, axis));
    }

    display_type?.size(node, ctx, axis)
}

/// Content-area size = border-box size minus padding and border.
pub fn content_size_query(
    _node: NodeId,
    _ctx: &dyn PropertyResolver,
    axis: Axis,
) -> Option<&'static Formula> {
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
fn keyword_size_formula(
    prop: lightningcss::properties::Property<'static>,
    node: NodeId,
    ctx: &dyn PropertyResolver,
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
            if let Some(DisplayType::Flex(dir, _)) = DisplayType::of_element(node, ctx) {
                Some(super::flex::flex_min_content_size(dir, axis, node, ctx))
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
                        return DisplayType::of_element(node, ctx)
                            .and_then(|dt| dt.size(node, ctx, axis));
                    }
                })
            }
        }
        Size::MaxContent(_) => {
            if let Some(DisplayType::Flex(dir, _)) = DisplayType::of_element(node, ctx) {
                Some(super::flex::flex_size(dir, axis, node, ctx))
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
                        return DisplayType::of_element(node, ctx)
                            .and_then(|dt| dt.size(node, ctx, axis));
                    }
                })
            }
        }
        _ => None,
    }
}

/// Margin-box size = border-box size + margins.
pub fn margin_box_size_query(
    _node: NodeId,
    _ctx: &dyn PropertyResolver,
    axis: Axis,
) -> Option<&'static Formula> {
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
