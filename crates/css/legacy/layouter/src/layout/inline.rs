//! Inline layout: arranging inline text and inline elements within a line box.

use std::collections::HashMap;
use js::NodeKey;
use style_engine::{SizeSpecified, Display};

use crate::LayoutNodeKind;
use super::args::{ComputeGeomArgs, LayoutMaps};
use super::geometry::LayoutRect;
use super::block::measure_descendant_inline_text_width;
use super::text::{measure_text_width, collapse_whitespace, greedy_line_break_widths};

/// Layout inline children with line wrapping and positioning.
/// Returns (additional_content_height, updated_child_y).
pub(crate) fn layout_inline_children(
    inline_children: &[NodeKey],
    node: NodeKey,
    maps: &LayoutMaps,
    rects: &mut HashMap<NodeKey, LayoutRect>,
    args: ComputeGeomArgs,
    x_position: i32,
    child_y: i32,
    content_width: i32,
    padding_left: i32,
    is_html: bool,
    is_body: bool,
) -> (i32, i32) {
    let mut additional_content_height = 0;
    let mut updated_child_y = child_y;
    
    if inline_children.is_empty() {
        return (additional_content_height, updated_child_y);
    }

    let mut inline_x = x_position + if is_html || is_body { 0 } else { padding_left };
    let mut child_y_inline = child_y;
    let mut had_inline_content_on_current_line = false;
    
    let parent_line_height = if let Some(computed_map) = maps.computed_by_key
        && let Some(parent_computed_style) = computed_map.get(&node)
    {
        (parent_computed_style.line_height * parent_computed_style.font_size).round() as i32
    } else { args.line_height };
    
    let line_height = parent_line_height;
    let mut min_inline_y: Option<i32> = None;
    let parent_font_size: f32 = if let Some(computed_map) = maps.computed_by_key { 
        computed_map.get(&node).map(|computed_style| computed_style.font_size).unwrap_or(16.0) 
    } else { 16.0 };
    let space_width: i32 = ((1.0_f32 * args.char_width as f32) * (parent_font_size / 16.0)).round() as i32;
    let line_start_x = inline_x;
    
    for (i, child) in inline_children.iter().enumerate() {
        // Insert a collapsed space between inline items (approximation)
        if i > 0 { inline_x += space_width; }
        
        match maps.kind_by_key.get(child) {
            Some(LayoutNodeKind::InlineText { text }) => {
                // Collapse whitespace per CSS Text 3 (simplified)
                let collapsed = collapse_whitespace(text);
                let metrics = measure_text_width(&collapsed, parent_font_size, args.char_width);
                let child_height = metrics.line_height;
                // Greedy UAX #14 line breaking: compute line widths for this text node
                let remaining = (line_start_x + content_width - inline_x).max(0);
                let line_widths = greedy_line_break_widths(&collapsed, parent_font_size, args.char_width, remaining, content_width);
                if line_widths.is_empty() {
                    continue;
                }
                // Wrap lines, updating inline cursor and content height
                let line_count = line_widths.len() as i32;
                if line_count > 1 {
                    // We are starting mid-line; the first width consumes remaining space.
                    // Subsequent lines occupy full width except possibly the last.
                    additional_content_height += (line_count - 1) * line_height;
                    child_y_inline += (line_count - 1) * line_height;
                }
                let first_line_width = line_widths[0];
                let last_line_width = *line_widths.last().unwrap();
                let y_offset = ((parent_line_height - child_height) / 2).max(0);
                let y_position = child_y_inline + y_offset;
                // Bounding rect covering all lines for this node
                let bounding_width = if line_count == 1 { first_line_width } else { content_width };
                let bounding_height = child_height * line_count;
                rects.insert(*child, LayoutRect { x: inline_x, y: y_position, width: bounding_width, height: bounding_height });
                if min_inline_y.map(|min_y| y_position < min_y).unwrap_or(true) { min_inline_y = Some(y_position); }
                // Advance inline cursor: end at last line's width on its line
                if line_count == 1 {
                    inline_x += first_line_width;
                } else {
                    inline_x = line_start_x + last_line_width;
                }
                had_inline_content_on_current_line = true;
            }
            Some(LayoutNodeKind::Block { tag }) => {
                // Decide whether this element participates as inline or forces a block boundary
                let mut treat_as_block = false;
                if let Some(computed_map) = maps.computed_by_key
                    && let Some(cs) = computed_map.get(child)
                { treat_as_block = cs.display != Display::Inline; }
                if treat_as_block {
                    // Treat block-in-inline as an anonymous block boundary:
                    // finalize current inline line if it has content; if at line start, still account for
                    // an initial empty line box before placing the block to mirror line box creation.
                    if inline_x > line_start_x {
                        // close current line with content
                        additional_content_height += line_height;
                        child_y_inline += line_height;
                        inline_x = line_start_x;
                    } else {
                        // line start: create an empty line box before the block
                        additional_content_height += line_height;
                        child_y_inline += line_height;
                    }
                    // Reset inline content state for new line after block
                    had_inline_content_on_current_line = false;
                }
                // Inline element (or block treated as replaced): estimate width from specified width or sum of descendant inline text
                let mut width = 0;
                let mut width_is_auto = true;
                if let Some(computed_map) = maps.computed_by_key
                    && let Some(computed_style) = computed_map.get(child)
                {
                    match computed_style.width {
                        SizeSpecified::Px(px) => { width = px.round() as i32; width_is_auto = false; }
                        SizeSpecified::Percent(percentage) => { width = (percentage * content_width as f32).round() as i32; width_is_auto = false; }
                        SizeSpecified::Auto => {}
                    }
                }
                if width == 0 {
                    let child_font_size: f32 = if let Some(computed_map) = maps.computed_by_key { 
                        computed_map.get(child).map(|computed_style| computed_style.font_size).unwrap_or(parent_font_size) 
                    } else { parent_font_size };
                    // Special-case <img> with no intrinsic size: fallback to alt-text heuristic
                    if tag.eq_ignore_ascii_case("img") {
                        let alt_text_len = maps
                            .attrs_by_key
                            .get(child)
                            .and_then(|attrs| attrs.get("alt"))
                            .map(|s| s.chars().count() as i32)
                            .unwrap_or(0);
                        if alt_text_len > 0 && width_is_auto {
                            // Heuristic: 0.6em per character
                            width = (0.8_f32 * child_font_size * alt_text_len as f32).round() as i32;
                            if width <= 0 { width = (0.8_f32 * parent_font_size).round() as i32; }
                        }
                    }
                    // If still zero, approximate using descendant inline text if any
                    if width == 0 {
                        width = measure_descendant_inline_text_width(*child, maps, args.char_width, child_font_size);
                    }
                }
                
                // Height
                let child_font_size: f32 = if let Some(computed_map) = maps.computed_by_key { 
                    computed_map.get(child).map(|computed_style| computed_style.font_size).unwrap_or(parent_font_size) 
                } else { parent_font_size };
                // Height: use parent line-height for inline-level boxes; replaced like <img> also use line box height
                let _is_img = tag.eq_ignore_ascii_case("img");
                let child_height = parent_line_height;
                let y_position = child_y_inline;
                
                rects.insert(*child, LayoutRect { x: inline_x, y: y_position, width: width.max(0), height: child_height.max(0) });
                
                if min_inline_y.map(|min_y| y_position < min_y).unwrap_or(true) { min_inline_y = Some(y_position); }
                if treat_as_block {
                    // Advance to next line after block
                    additional_content_height += child_height.max(line_height);
                    child_y_inline += child_height.max(line_height);
                    inline_x = line_start_x;
                } else {
                    // Inline element continues on the same line
                    inline_x += width.max(0);
                    had_inline_content_on_current_line = true;
                }
            }
            _ => {}
        }
    }
    // Add the last line only if it has content remaining on the line
    if inline_x > line_start_x || had_inline_content_on_current_line {
        additional_content_height += line_height;
        child_y_inline += line_height;
    }
    updated_child_y = child_y_inline;
    
    // Ensure rects exist for all inline element children (safety if skipped elsewhere)
    let inline_y_base = child_y_inline;
    for child_key in inline_children {
        if rects.get(child_key).is_none() {
            // Compute width from specified width or descendant inline text
            let mut width = 0;
            if let Some(computed_map) = maps.computed_by_key
                && let Some(computed_style) = computed_map.get(child_key)
            {
                match computed_style.width {
                    SizeSpecified::Px(px) => { width = px.round() as i32; }
                    SizeSpecified::Percent(percentage) => { width = (percentage * content_width as f32).round() as i32; }
                    SizeSpecified::Auto => {}
                }
            }
            if width == 0 { width = measure_descendant_inline_text_width(*child_key, maps, args.char_width, parent_font_size); }
            
            // Height from parent line-height
            let height = if let Some(computed_map) = maps.computed_by_key
                && let Some(parent_computed_style) = computed_map.get(&node)
            {
                (parent_computed_style.line_height * parent_computed_style.font_size).round() as i32
            } else { args.line_height };
            
            rects.insert(*child_key, LayoutRect { x: inline_x, y: inline_y_base, width, height });
        }
    }
    
    (additional_content_height, updated_child_y)
}
