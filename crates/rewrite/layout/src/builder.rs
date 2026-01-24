//! Layout tree builder that walks the DOM and constructs the layout tree.
//!
//! This module coordinates all the layout computations and builds the final
//! LayoutBox tree that can be passed to the renderer.

use crate::{
    BlockOffsetQuery, BlockSizeQuery, BoxType, EdgeSizes, InlineOffsetQuery, InlineSizeQuery,
    LayoutBox, Rect,
};
use rewrite_core::{Database, DependencyContext, NodeId, ScopedDb};
use rewrite_css::{CssKeyword, CssValue, DisplayQuery, EndMarker, PositionQuery, StartMarker};
use rewrite_css_dimensional::{BorderWidthQuery, MarginQuery, PaddingQuery};

/// Build the layout tree from a DOM node.
///
/// This walks the DOM tree and creates a LayoutBox for each element,
/// computing its position, size, and properties.
pub fn build_layout_tree(db: &Database, root: NodeId) -> Option<LayoutBox> {
    let mut ctx = DependencyContext::new();
    build_layout_box(db, root, &mut ctx)
}

/// Build a single layout box for a node and its descendants.
fn build_layout_box(db: &Database, node: NodeId, ctx: &mut DependencyContext) -> Option<LayoutBox> {
    let mut scoped = ScopedDb::new(db, node, ctx);

    // Skip elements that should not be displayed (UA stylesheet defaults)
    // TODO: Replace with proper UA stylesheet
    if should_skip_element(&mut scoped) {
        return None;
    }

    // Check display property to determine box type
    let display = scoped.query::<DisplayQuery>();
    let position = scoped.query::<PositionQuery>();
    let float_value = scoped.query::<rewrite_css::FloatQuery>();

    // Determine box type
    let box_type = determine_box_type(&display, &position, &float_value);

    // Skip display:none elements
    if matches!(display, CssValue::Keyword(CssKeyword::None)) {
        return None;
    }

    // Create the layout box
    let mut layout_box = LayoutBox::new(node, box_type);

    // Compute position and size
    let inline_offset = scoped.query::<InlineOffsetQuery>();
    let block_offset = scoped.query::<BlockOffsetQuery>();
    let inline_size = scoped.query::<InlineSizeQuery>();
    let block_size = scoped.query::<BlockSizeQuery>();

    layout_box.content_rect = Rect::new(inline_offset, block_offset, inline_size, block_size);

    // Compute box model edges (padding, border, margin)
    layout_box.padding = get_padding(&mut scoped);
    layout_box.border = get_border(&mut scoped);
    layout_box.margin = get_margin(&mut scoped);

    // Build children
    match box_type {
        BoxType::Block
        | BoxType::InlineBlock
        | BoxType::Flex
        | BoxType::Grid
        | BoxType::Table
        | BoxType::TableRow => {
            // Build children recursively
            let children = scoped
                .db()
                .resolve_relationship(node, rewrite_core::Relationship::Children);
            for &child in &children {
                if let Some(child_box) = build_layout_box(db, child, ctx) {
                    layout_box.add_child(child_box);
                }
            }
        }
        BoxType::Inline => {
            // Inline elements may have inline children
            let children = scoped
                .db()
                .resolve_relationship(node, rewrite_core::Relationship::Children);
            for &child in &children {
                if let Some(child_box) = build_layout_box(db, child, ctx) {
                    layout_box.add_child(child_box);
                }
            }
        }
        BoxType::TableCell => {
            // Table cells contain block content
            let children = scoped
                .db()
                .resolve_relationship(node, rewrite_core::Relationship::Children);
            for &child in &children {
                if let Some(child_box) = build_layout_box(db, child, ctx) {
                    layout_box.add_child(child_box);
                }
            }
        }
        BoxType::Text | BoxType::Line => {
            // Text boxes and line boxes don't have children
        }
        BoxType::Absolute | BoxType::Fixed => {
            // Positioned boxes can have children
            let children = scoped
                .db()
                .resolve_relationship(node, rewrite_core::Relationship::Children);
            for &child in &children {
                if let Some(child_box) = build_layout_box(db, child, ctx) {
                    layout_box.add_child(child_box);
                }
            }
        }
        BoxType::Float => {
            // Floats can have children
            let children = scoped
                .db()
                .resolve_relationship(node, rewrite_core::Relationship::Children);
            for &child in &children {
                if let Some(child_box) = build_layout_box(db, child, ctx) {
                    layout_box.add_child(child_box);
                }
            }
        }
        BoxType::FlexItem | BoxType::GridItem => {
            // Flex and grid items can have children
            let children = scoped
                .db()
                .resolve_relationship(node, rewrite_core::Relationship::Children);
            for &child in &children {
                if let Some(child_box) = build_layout_box(db, child, ctx) {
                    layout_box.add_child(child_box);
                }
            }
        }
    }

    Some(layout_box)
}

/// Determine the box type from CSS properties.
fn determine_box_type(display: &CssValue, position: &CssValue, float_value: &CssValue) -> BoxType {
    // Check position first (highest precedence)
    match position {
        CssValue::Keyword(CssKeyword::Absolute) => return BoxType::Absolute,
        CssValue::Keyword(CssKeyword::Fixed) => return BoxType::Fixed,
        _ => {}
    }

    // Check float
    if !matches!(float_value, CssValue::Keyword(CssKeyword::None)) {
        return BoxType::Float;
    }

    // Check display
    match display {
        CssValue::Keyword(CssKeyword::Block) => BoxType::Block,
        CssValue::Keyword(CssKeyword::Inline) => BoxType::Inline,
        CssValue::Keyword(CssKeyword::InlineBlock) => BoxType::InlineBlock,
        CssValue::Keyword(CssKeyword::Flex) => BoxType::Flex,
        CssValue::Keyword(CssKeyword::Grid) => BoxType::Grid,
        CssValue::Keyword(CssKeyword::Table) => BoxType::Table,
        CssValue::Keyword(CssKeyword::TableRow) => BoxType::TableRow,
        CssValue::Keyword(CssKeyword::TableCell) => BoxType::TableCell,
        _ => BoxType::Block, // Default to block
    }
}

/// Check if an element should be skipped (not displayed) based on element name.
/// This is a temporary workaround until we have a proper UA stylesheet.
fn should_skip_element(scoped: &mut ScopedDb) -> bool {
    // Get the element's tag name
    use rewrite_html::TagNameQuery;
    if let Some(tag_name) = scoped.query::<TagNameQuery>() {
        let should_skip = matches!(
            tag_name.as_str(),
            "head" | "script" | "style" | "meta" | "link" | "title"
        );
        if should_skip {
            eprintln!("Skipping element: {}", tag_name);
        }
        should_skip
    } else {
        false
    }
}

/// Get padding edges from CSS properties.
fn get_padding(scoped: &mut ScopedDb) -> EdgeSizes {
    let top = scoped.query::<PaddingQuery<rewrite_css::BlockMarker, StartMarker>>();
    let bottom = scoped.query::<PaddingQuery<rewrite_css::BlockMarker, EndMarker>>();
    let left = scoped.query::<PaddingQuery<rewrite_css::InlineMarker, StartMarker>>();
    let right = scoped.query::<PaddingQuery<rewrite_css::InlineMarker, EndMarker>>();

    EdgeSizes::new(top, right, bottom, left)
}

/// Get border widths from CSS properties.
fn get_border(scoped: &mut ScopedDb) -> EdgeSizes {
    let top = scoped.query::<BorderWidthQuery<rewrite_css::BlockMarker, StartMarker>>();
    let bottom = scoped.query::<BorderWidthQuery<rewrite_css::BlockMarker, EndMarker>>();
    let left = scoped.query::<BorderWidthQuery<rewrite_css::InlineMarker, StartMarker>>();
    let right = scoped.query::<BorderWidthQuery<rewrite_css::InlineMarker, EndMarker>>();

    EdgeSizes::new(top, right, bottom, left)
}

/// Get margin edges from CSS properties.
fn get_margin(scoped: &mut ScopedDb) -> EdgeSizes {
    let top = scoped.query::<MarginQuery<rewrite_css::BlockMarker, StartMarker>>();
    let bottom = scoped.query::<MarginQuery<rewrite_css::BlockMarker, EndMarker>>();
    let left = scoped.query::<MarginQuery<rewrite_css::InlineMarker, StartMarker>>();
    let right = scoped.query::<MarginQuery<rewrite_css::InlineMarker, EndMarker>>();

    EdgeSizes::new(top, right, bottom, left)
}

/// Compute the total size of a layout tree (for debugging/testing).
pub fn compute_tree_size(layout_box: &LayoutBox) -> (usize, usize) {
    let mut node_count = 1;
    let mut max_depth = 1;

    for child in &layout_box.children {
        let (child_nodes, child_depth) = compute_tree_size(child);
        node_count += child_nodes;
        max_depth = max_depth.max(child_depth + 1);
    }

    (node_count, max_depth)
}

/// Dump a layout tree to a string (for debugging).
pub fn dump_layout_tree(layout_box: &LayoutBox, indent: usize) -> String {
    let mut result = String::new();

    let indent_str = "  ".repeat(indent);
    result.push_str(&format!(
        "{}Box({:?}) @ ({}, {}) {}x{}\n",
        indent_str,
        layout_box.box_type,
        layout_box.content_rect.x,
        layout_box.content_rect.y,
        layout_box.content_rect.width,
        layout_box.content_rect.height
    ));

    for child in &layout_box.children {
        result.push_str(&dump_layout_tree(child, indent + 1));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_box_type_determination() {
        let absolute = CssValue::Keyword(CssKeyword::Absolute);
        let block = CssValue::Keyword(CssKeyword::Block);
        let none = CssValue::Keyword(CssKeyword::None);

        assert_eq!(
            determine_box_type(&block, &absolute, &none),
            BoxType::Absolute
        );

        let static_pos = CssValue::Keyword(CssKeyword::Static);
        assert_eq!(
            determine_box_type(&block, &static_pos, &none),
            BoxType::Block
        );

        let flex = CssValue::Keyword(CssKeyword::Flex);
        assert_eq!(determine_box_type(&flex, &static_pos, &none), BoxType::Flex);
    }
}
