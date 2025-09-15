//! Block layout helpers and utilities.

use std::collections::HashMap;
use js::NodeKey;
use crate::LayoutNodeKind;

use super::args::{ComputeGeomArgs, LayoutMaps};
use super::geometry::LayoutRect;
use super::styles::LayoutStyles;
use super::text::{measure_text_width, collapse_whitespace};

/// Returns true if the provided tag is a non-rendering element that should be skipped.
pub(crate) fn is_non_rendering_tag(tag: &str) -> bool {
    matches!(
        tag.to_lowercase().as_str(),
        "head" | "meta" | "title" | "link" | "style" | "script" | "base"
    )
}

/// Calculate the width of descendant inline text elements.
pub(crate) fn measure_descendant_inline_text_width(
    node: NodeKey,
    maps: &LayoutMaps,
    char_width: i32,
    base_font_size: f32,
) -> i32 {
    let mut sum = 0;
    // Use the node's own computed font size if available; otherwise inherit base_font_size
    let mut current_font_size = base_font_size;
    if let Some(comp_map) = maps.computed_by_key
        && let Some(cs) = comp_map.get(&node)
    { current_font_size = cs.font_size; }
    if let Some(kind) = maps.kind_by_key.get(&node) {
        match kind {
            LayoutNodeKind::InlineText { text } => {
                let collapsed = collapse_whitespace(text);
                let metrics = measure_text_width(&collapsed, current_font_size, char_width);
                sum += metrics.width;
            }
            LayoutNodeKind::Block { .. } => {
                if let Some(children) = maps.children_by_key.get(&node) {
                    for c in children {
                        sum += measure_descendant_inline_text_width(*c, maps, char_width, current_font_size);
                    }
                }
            }
            LayoutNodeKind::Document => {}
        }
    }
    sum
}

/// Layout block children with margin collapsing.
/// Returns (additional_content_height, updated_child_y).
pub(crate) fn layout_block_children(
    block_children: &[NodeKey],
    _parent_node: NodeKey,
    maps: &LayoutMaps,
    rects: &mut HashMap<NodeKey, LayoutRect>,
    args: ComputeGeomArgs,
    content_width: i32,
    child_y: i32,
    styles: &LayoutStyles,
    is_html: bool,
    is_body: bool,
    depth: usize,
    y_cursor: &mut i32,
) -> (i32, i32) {
    if block_children.is_empty() {
        return (0, child_y);
    }

    let mut content_height = 0;
    let parent_margin_top: i32 = if is_html || is_body { 0 } else { styles.margin.top.round() as i32 };
    let mut prev_bottom_margin: i32 = 0;
    let mut current_y = child_y;
    let mut is_first_block = true;
    let mut _first_collapsed_delta: i32 = 0;

    for child in block_children {
        // Resolve child margins from computed styles (default 0)
        let (child_margin_top, child_margin_bottom) = if let Some(computed_map) = maps.computed_by_key {
            if let Some(computed_style) = computed_map.get(child) {
                (computed_style.margin.top.round() as i32, computed_style.margin.bottom.round() as i32)
            } else { (0, 0) }
        } else { (0, 0) };

        // Collapse adjacent vertical margins: on first child, collapse against parent's top margin
        if is_first_block {
            let delta = std::cmp::max(parent_margin_top, child_margin_top) - parent_margin_top;
            _first_collapsed_delta = delta;
            current_y = child_y + delta;
            is_first_block = false;
        } else {
            let collapsed_margin = std::cmp::max(prev_bottom_margin, child_margin_top);
            current_y += collapsed_margin;
        }

        // For block children, pass down the parent content width as viewport_width for percent resolution
        let child_args = ComputeGeomArgs { viewport_width: content_width, body_margin: 0, line_height: args.line_height, char_width: args.char_width, v_gap: args.v_gap };
        let _top_passed = current_y;
        *y_cursor = current_y;
        let (child_consumed, child_content_height) = super::compute::layout_node(*child, depth + 1, maps, rects, child_args, y_cursor);
        // Accumulate the child's used outer height (consumed), not just its content height,
        // so the parent's content height reflects actual occupied space.
        let _ = child_content_height; // retained for clarity; used for future refinements
        content_height += child_consumed;
        
        let next_y = if let Some(rect) = rects.get(child) { rect.y + child_consumed } else { current_y + child_consumed };
        current_y = next_y;
        prev_bottom_margin = child_margin_bottom;
    }

    (content_height, current_y)
}
