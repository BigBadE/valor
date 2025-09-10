use crate::{LayoutNodeKind, Layouter};
use js::NodeKey;
use std::collections::HashMap;
use style_engine::{ComputedStyle, Display, SizeSpecified, Edges};

// Consolidated argument structs for geometry computation
#[derive(Debug, Clone, Copy)]
pub struct ComputeGeomArgs {
    pub viewport_width: i32,
    pub body_margin: i32,
    pub line_height: i32,
    pub char_width: i32,
    pub v_gap: i32,
}

pub struct LayoutMaps<'a> {
    pub kind_by_key: &'a HashMap<NodeKey, LayoutNodeKind>,
    pub children_by_key: &'a HashMap<NodeKey, Vec<NodeKey>>,
    pub computed_by_key: Option<&'a HashMap<NodeKey, ComputedStyle>>,
}

/// Information extracted from computed styles for a layout node
#[derive(Debug, Clone)]
struct LayoutStyles {
    display_inline: bool,
    display_none: bool,
    margin: Edges,
    padding: Edges,
    width_spec: Option<SizeSpecified>,
    height_spec: Option<SizeSpecified>,
}

impl Default for LayoutStyles {
    fn default() -> Self {
        Self {
            display_inline: false,
            display_none: false,
            margin: Edges { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 },
            padding: Edges { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 },
            width_spec: None,
            height_spec: None,
        }
    }
}

/// Extract layout-relevant styles from computed styles for a given node
fn extract_layout_styles(node: NodeKey, maps: &LayoutMaps) -> LayoutStyles {
    let mut styles = LayoutStyles::default();
    
    if let Some(computed_map) = maps.computed_by_key {
        if let Some(computed_style) = computed_map.get(&node) {
            styles.display_none = computed_style.display == Display::None;
            styles.display_inline = computed_style.display == Display::Inline;
            styles.margin = computed_style.margin;
            styles.padding = computed_style.padding;
            styles.width_spec = Some(computed_style.width);
            styles.height_spec = Some(computed_style.height);
        }
    }
    
    styles
}

/// Check if the given tag represents a non-rendering element
fn is_non_rendering_tag(tag: &str) -> bool {
    matches!(
        tag.to_lowercase().as_str(),
        "head" | "meta" | "title" | "link" | "style" | "script" | "base"
    )
}

/// Layout inline children with line wrapping and positioning
/// Returns (additional_content_height, updated_child_y)
fn layout_inline_children(
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
    
    let parent_line_height = if let Some(computed_map) = maps.computed_by_key {
        if let Some(parent_computed_style) = computed_map.get(&node) {
            (parent_computed_style.line_height * parent_computed_style.font_size).round() as i32
        } else { 
            args.line_height 
        }
    } else { 
        args.line_height 
    };
    
    let line_height = parent_line_height;
    let mut min_inline_y: Option<i32> = None;
    let parent_font_size: f32 = if let Some(computed_map) = maps.computed_by_key { 
        computed_map.get(&node).map(|computed_style| computed_style.font_size).unwrap_or(16.0) 
    } else { 
        16.0 
    };
    let space_width: i32 = ((1.0_f32 * args.char_width as f32) * (parent_font_size / 16.0)).round() as i32;
    let line_start_x = inline_x;
    
    for (i, child) in inline_children.iter().enumerate() {
        // Insert a collapsed space between inline items (approximation)
        if i > 0 { 
            inline_x += space_width; 
        }
        
        match maps.kind_by_key.get(child) {
            Some(LayoutNodeKind::InlineText { text }) => {
                let scale = parent_font_size / 16.0;
                let width = ((text.chars().count() as f32 * args.char_width as f32) * scale).round() as i32;
                
                // Wrap to next line if exceeds content width
                if inline_x + width > line_start_x + content_width {
                    child_y_inline += line_height;
                    additional_content_height += line_height;
                    inline_x = line_start_x;
                }
                
                let child_height = (parent_font_size * 1.1).round() as i32;
                let y_offset = ((parent_line_height - child_height) / 2).max(0);
                let y_position = child_y_inline + y_offset;
                
                rects.insert(*child, LayoutRect { 
                    x: inline_x, 
                    y: y_position, 
                    width, 
                    height: child_height 
                });
                
                if min_inline_y.map(|min_y| y_position < min_y).unwrap_or(true) { 
                    min_inline_y = Some(y_position); 
                }
                inline_x += width;
            }
            Some(LayoutNodeKind::Block { .. }) => {
                // Inline element: estimate width from specified width or sum of descendant inline text
                let mut width = 0;
                if let Some(computed_map) = maps.computed_by_key {
                    if let Some(computed_style) = computed_map.get(&child) {
                        match computed_style.width {
                            SizeSpecified::Px(px) => { width = px.round() as i32; }
                            SizeSpecified::Percent(percentage) => { 
                                width = (percentage * content_width as f32).round() as i32; 
                            }
                            SizeSpecified::Auto => {}
                        }
                    }
                }
                if width == 0 {
                    let child_font_size: f32 = if let Some(computed_map) = maps.computed_by_key { 
                        computed_map.get(&child).map(|computed_style| computed_style.font_size).unwrap_or(parent_font_size) 
                    } else { 
                        parent_font_size 
                    };
                    width = measure_descendant_inline_text_width(*child, maps, args.char_width, child_font_size);
                }
                
                // Wrap to next line if exceeds content width
                if inline_x + width > line_start_x + content_width {
                    child_y_inline += line_height;
                    additional_content_height += line_height;
                    inline_x = line_start_x;
                }
                
                let child_font_size: f32 = if let Some(computed_map) = maps.computed_by_key { 
                    computed_map.get(&child).map(|computed_style| computed_style.font_size).unwrap_or(parent_font_size) 
                } else { 
                    parent_font_size 
                };
                let child_height = (child_font_size * 1.1).round() as i32;
                let y_offset = ((parent_line_height - child_height) / 2).max(0);
                let y_position = child_y_inline + y_offset;
                
                rects.insert(*child, LayoutRect { 
                    x: inline_x, 
                    y: y_position, 
                    width, 
                    height: child_height 
                });
                
                if min_inline_y.map(|min_y| y_position < min_y).unwrap_or(true) { 
                    min_inline_y = Some(y_position); 
                }
                inline_x += width;
            }
            _ => {}
        }
    }
    
    // Account for the first line height
    additional_content_height += line_height;
    updated_child_y += line_height;
    
    // Ensure rects exist for all inline element children (safety if skipped elsewhere)
    let inline_y_base = child_y_inline;
    for child_key in inline_children {
        if rects.get(child_key).is_none() {
            // Compute width from specified width or descendant inline text
            let mut width = 0;
            if let Some(computed_map) = maps.computed_by_key {
                if let Some(computed_style) = computed_map.get(child_key) {
                    match computed_style.width {
                        SizeSpecified::Px(px) => { width = px.round() as i32; }
                        SizeSpecified::Percent(percentage) => { 
                            width = (percentage * content_width as f32).round() as i32; 
                        }
                        SizeSpecified::Auto => {}
                    }
                }
            }
            if width == 0 { 
                width = measure_descendant_inline_text_width(*child_key, maps, args.char_width, parent_font_size); 
            }
            
            // Height from parent line-height
            let height = if let Some(computed_map) = maps.computed_by_key {
                if let Some(parent_computed_style) = computed_map.get(&node) {
                    (parent_computed_style.line_height * parent_computed_style.font_size).round() as i32
                } else { 
                    args.line_height 
                }
            } else { 
                args.line_height 
            };
            
            rects.insert(*child_key, LayoutRect { 
                x: inline_x, 
                y: inline_y_base, 
                width, 
                height 
            });
        }
    }
    
    (additional_content_height, updated_child_y)
}

/// Layout block children with margin collapsing
/// Returns (additional_content_height, updated_child_y)
fn layout_block_children(
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
            } else { 
                (0, 0) 
            }
        } else { 
            (0, 0) 
        };

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
        let child_args = ComputeGeomArgs { 
            viewport_width: content_width, 
            body_margin: 0, 
            line_height: args.line_height, 
            char_width: args.char_width, 
            v_gap: args.v_gap 
        };
        let _top_passed = current_y;
        *y_cursor = current_y;
        let (child_consumed, child_content_height) = layout_node(*child, depth + 1, maps, rects, child_args, y_cursor);
        content_height += child_content_height;
        
        let next_y = if let Some(rect) = rects.get(child) {
            rect.y + child_consumed
        } else {
            current_y + child_consumed
        };
        current_y = next_y;
        prev_bottom_margin = child_margin_bottom;
    }

    (content_height, current_y)
}

/// Calculate final dimensions and positioning for a layout node
/// Returns (final_top, final_width, final_height)
fn calculate_final_dimensions_and_position(
    node: NodeKey,
    maps: &LayoutMaps,
    rects: &HashMap<NodeKey, LayoutRect>,
    styles: &LayoutStyles,
    content_height: i32,
    border_width: i32,
    top: i32,
    padding_top: i32,
    padding_bottom: i32,
    is_html: bool,
    is_body: bool,
) -> (i32, i32, i32) {
    // Height resolution
    let mut used_height = content_height;
    if let Some(height_size) = styles.height_spec { 
        if let SizeSpecified::Px(px) = height_size { 
            used_height = px.round() as i32; 
        } 
    }
    
    // Border-box dimensions
    let mut border_height = if is_html || is_body { 
        used_height 
    } else { 
        (used_height + padding_top + padding_bottom).max(0) 
    };

    let mut out_top = top;
    
    // Adjust body box to wrap children (approximate margin-collapsing with first/last child)
    if is_body {
        let mut min_y = i32::MAX;
        let mut max_yh = 0;
        if let Some(children) = maps.children_by_key.get(&node) {
            for child in children {
                if matches!(maps.kind_by_key.get(child), Some(LayoutNodeKind::Block { .. })) {
                    if let Some(rect) = rects.get(child) {
                        if rect.y < min_y { min_y = rect.y; }
                        if rect.y + rect.height > max_yh { max_yh = rect.y + rect.height; }
                    }
                }
            }
        }
        if min_y != i32::MAX && max_yh >= min_y {
            out_top = min_y;
            border_height = max_yh - out_top;
        } else {
            // Fallback: derive from first block child's top margin if available
            if let Some(children) = maps.children_by_key.get(&node) {
                if let Some(first_block) = children.iter().find(|c| matches!(maps.kind_by_key.get(c), Some(LayoutNodeKind::Block { .. }))) {
                    if let Some(computed_map) = maps.computed_by_key {
                        if let Some(computed_style) = computed_map.get(first_block) {
                            let margin_top = computed_style.margin.top.round() as i32;
                            if margin_top > 0 {
                                out_top = margin_top;
                                border_height = (max_yh - out_top).max(0);
                            }
                        }
                    }
                }
            }
        }
    } else {
        // For regular blocks, align the parent's top to the minimum y of its block children (if any),
        // so collapsed top margins are reflected in getBoundingClientRect.
        if let Some(children) = maps.children_by_key.get(&node) {
            let mut min_y = i32::MAX;
            for child in children {
                if matches!(maps.kind_by_key.get(child), Some(LayoutNodeKind::Block { .. })) {
                    if let Some(rect) = rects.get(child) {
                        if rect.y < min_y { min_y = rect.y; }
                    }
                }
            }
            if min_y != i32::MAX { 
                out_top = min_y; 
            } else { 
                out_top = top; 
            }
        } else {
            out_top = top;
        }
    }

    (out_top, border_width, border_height)
}

/// Calculate the width of descendant inline text elements
fn measure_descendant_inline_text_width(node: NodeKey, maps: &LayoutMaps, char_width: i32, base_font_size: f32) -> i32 {
    let mut sum = 0;
    // Use the node's own computed font size if available; otherwise inherit base_font_size
    let mut current_font_size = base_font_size;
    if let Some(comp_map) = maps.computed_by_key {
        if let Some(cs) = comp_map.get(&node) {
            current_font_size = cs.font_size;
        }
    }
    if let Some(kind) = maps.kind_by_key.get(&node) {
        match kind {
            LayoutNodeKind::InlineText { text } => {
                let scale = current_font_size / 16.0;
                sum += ((text.chars().count() as f32 * char_width as f32) * scale).round() as i32;
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

/// Recursively layout nodes, building rects and advancing y_cursor.
pub fn layout_node(
    node: NodeKey,
    depth: usize,
    maps: &LayoutMaps,
    rects: &mut HashMap<NodeKey, LayoutRect>,
    args: ComputeGeomArgs,
    y_cursor: &mut i32,
) -> (i32, i32) {
    match maps.kind_by_key.get(&node) {
        Some(LayoutNodeKind::Block { tag }) => {
            let tag_lowercase = tag.to_lowercase();
            
            // Extract computed styles for this node
            let styles = extract_layout_styles(node, maps);
            
            // Early returns for non-rendering cases
            if styles.display_none { 
                return (0, 0); 
            }
            if is_non_rendering_tag(&tag_lowercase) { 
                return (0, 0); 
            }
            let is_html = tag_lowercase == "html";
            let is_body = tag_lowercase == "body";

            // For inline elements, don't create a box; just descend into children
            if styles.display_inline {
                let mut consumed = 0;
                let mut content_height = 0;
                let mut min_x = i32::MAX;
                let mut min_y = i32::MAX;
                let mut max_xh = 0;
                let mut max_yh = 0;
                if let Some(children) = maps.children_by_key.get(&node) {
                    for child in children {
                        let (c, h) = layout_node(*child, depth + 1, maps, rects, args, y_cursor);
                        consumed += c; content_height += h;
                        if let Some(r) = rects.get(child) {
                            if r.x < min_x { min_x = r.x; }
                            if r.y < min_y { min_y = r.y; }
                            if r.x + r.width > max_xh { max_xh = r.x + r.width; }
                            if r.y + r.height > max_yh { max_yh = r.y + r.height; }
                        }
                    }
                }
                if min_x != i32::MAX && min_y != i32::MAX {
                    rects.insert(node, LayoutRect { x: min_x, y: min_y, width: (max_xh - min_x).max(0), height: (max_yh - min_y).max(0) });
                }
                return (consumed, content_height);
            }

            // Determine available width inside body content area
            let container_content_width = args.viewport_width - args.body_margin * 2;
            // Margins for non-root blocks (use descriptive names instead of abbreviations)
            let margin_left = if is_html || is_body { 0 } else { styles.margin.left.round() as i32 };
            let margin_right = if is_html || is_body { 0 } else { styles.margin.right.round() as i32 };
            let padding_top = if is_html || is_body { 0 } else { styles.padding.top.round() as i32 };
            let padding_right = if is_html || is_body { 0 } else { styles.padding.right.round() as i32 };
            let padding_bottom = if is_html || is_body { 0 } else { styles.padding.bottom.round() as i32 };
            let padding_left = if is_html || is_body { 0 } else { styles.padding.left.round() as i32 };

            let base_width_for_percent = if is_html || is_body { container_content_width } else { (container_content_width - margin_left - margin_right).max(0) };

            // Resolve width with border-box semantics (CSS reset sets box-sizing: border-box)
            let content_width: i32;
            let border_width: i32;
            if is_html || is_body {
                content_width = container_content_width;
                border_width = container_content_width;
            } else {
                match styles.width_spec {
                    Some(SizeSpecified::Px(px)) => {
                        border_width = px.round() as i32;
                        content_width = (border_width - padding_left - padding_right).max(0);
                    }
                    Some(SizeSpecified::Percent(p)) => {
                        let border_width_from_percent = (p * base_width_for_percent as f32).round() as i32;
                        border_width = border_width_from_percent;
                        content_width = (border_width - padding_left - padding_right).max(0);
                    }
                    _ => {
                        // Auto → fill available content box (available width minus padding)
                        content_width = (base_width_for_percent - padding_left - padding_right).max(0);
                        border_width = (content_width + padding_left + padding_right).max(0);
                    }
                }
            }

            // Positioning
            let top = *y_cursor;
            let x_position = if is_html || is_body { 0 } else { margin_left };

            // Compute children layout. Children start at content box top
            let mut content_height = 0;
            let mut child_y = top + if is_html || is_body { 0 } else { padding_top };
            // Track collapsed delta for parent's top adjustment (first child's top margin collapsing with parent)
            let mut _first_collapsed_delta: i32 = 0;
            if let Some(children) = maps.children_by_key.get(&node) {
                // For the html element, only lay out the body element (ignore others)
                if is_html {
                    if let Some(body_key) = children.iter().find_map(|c| match maps.kind_by_key.get(c) {
                        Some(LayoutNodeKind::Block { tag }) if tag.eq_ignore_ascii_case("body") => Some(*c),
                        _ => None,
                    }) {
                        *y_cursor = child_y;
                        let (_child_consumed, child_content_h) = layout_node(body_key, depth + 1, maps, rects, args, y_cursor);
                        content_height += child_content_h;
                    }
                } else {
                    // For other blocks, determine if there are any rendering block children
                    let mut block_children: Vec<NodeKey> = Vec::new();
                    let mut inline_children: Vec<NodeKey> = Vec::new();
                    let mut sep_has_space: Vec<bool> = Vec::new();
                    let mut space_pending = false;
                    for child in children {
                        match maps.kind_by_key.get(child) {
                            Some(LayoutNodeKind::Block { tag }) => {
                                let tl = tag.to_lowercase();
                                let non_render = matches!(tl.as_str(), "head" | "meta" | "title" | "link" | "style" | "script" | "base");
                                if non_render { continue; }
                                // Decide inline vs block using computed display if available
                                let mut computed_inline: Option<bool> = None;
                                let mut display_none = false;
                                if let Some(comp_map) = maps.computed_by_key {
                                    if let Some(cs) = comp_map.get(child) {
                                        if cs.display == Display::None { display_none = true; }
                                        else { computed_inline = Some(cs.display == Display::Inline); }
                                    }
                                }
                                if display_none { continue; }
                                let child_inline = if let Some(ci) = computed_inline { ci } else { false };
                                if child_inline {
                                    inline_children.push(*child);
                                    sep_has_space.push(space_pending);
                                    space_pending = false;
                                } else {
                                    block_children.push(*child);
                                    // block children break inline run; reset pending space
                                    space_pending = false;
                                }
                            }
                            Some(LayoutNodeKind::InlineText { text }) => {
                                if text.trim().is_empty() {
                                    // whitespace collapses; mark pending space between inline content
                                    space_pending = true;
                                } else {
                                    inline_children.push(*child);
                                    sep_has_space.push(space_pending);
                                    space_pending = false;
                                }
                            }
                            _ => {}
                        }
                    }
                    // Layout inline children if any exist
                    if !inline_children.is_empty() {
                        let (additional_height, updated_y) = layout_inline_children(
                            &inline_children,
                            node,
                            maps,
                            rects,
                            args,
                            x_position,
                            child_y,
                            content_width,
                            padding_left,
                            is_html,
                            is_body,
                        );
                        content_height += additional_height;
                        child_y = updated_y;
                    }
                    // Layout block children if any exist
                    if !block_children.is_empty() {
                        let (additional_height, _updated_y) = layout_block_children(
                            &block_children,
                            node,
                            maps,
                            rects,
                            args,
                            content_width,
                            child_y,
                            &styles,
                            is_html,
                            is_body,
                            depth,
                            y_cursor,
                        );
                        content_height += additional_height;
                    }
                }
            }

            // Calculate final dimensions and positioning
            let (out_top, final_width, border_height) = calculate_final_dimensions_and_position(
                node,
                maps,
                rects,
                &styles,
                content_height,
                border_width,
                top,
                padding_top,
                padding_bottom,
                is_html,
                is_body,
            );

            let used_height = if let Some(height_size) = styles.height_spec { 
                if let SizeSpecified::Px(px) = height_size { 
                    px.round() as i32 
                } else { 
                    content_height 
                }
            } else { 
                content_height 
            };

            rects.insert(node, LayoutRect { x: x_position, y: out_top, width: final_width, height: border_height });
            let consumed = border_height + args.v_gap;
            *y_cursor = out_top + border_height + args.v_gap;
            (consumed, used_height)
        }
        Some(LayoutNodeKind::InlineText { .. }) => {
            // Outside of inline formatting context, do not allocate geometry for stray text nodes.
            // Inline text is handled inside the parent block's inline flow.
            (0, 0)
        }
        Some(LayoutNodeKind::Document) | None => {
            // No geometry for document; descend into children
            let mut consumed = 0;
            let mut content_height = 0;
            if let Some(children) = maps.children_by_key.get(&node) {
                for child in children {
                    let (c, h) = layout_node(*child, depth + 1, maps, rects, args, y_cursor);
                    consumed += c;
                    content_height += h;
                }
            }
            (consumed, content_height)
        }
    }
}

/// Perform a simple block layout pass over the mirrored DOM using only the public API.
///
/// This implementation intentionally uses only the public Layouter API (snapshot)
/// and does not mutate internal state. It simulates a block formatting context:
/// - The root establishes a viewport of fixed width.
/// - Block elements are stacked vertically in document order.
/// - InlineText becomes an anonymous block with a single line of fixed line-height.
/// The function returns the number of laid-out boxes (excluding the root document node).
pub fn compute_simple_layout(layouter: &Layouter) -> usize {
    let snapshot = layouter.snapshot();
    if snapshot.is_empty() {
        return 0;
    }

    // Build temporary lookup maps from the snapshot
    let mut kind_by_key = HashMap::new();
    let mut children_by_key = HashMap::new();
    for (key, kind, children) in snapshot.into_iter() {
        kind_by_key.insert(key.clone(), kind);
        children_by_key.insert(key, children);
    }

    // Ensure we have the root
    let root = NodeKey::ROOT;
    let Some(root_children) = children_by_key.get(&root) else {
        return 0;
    };

    // Viewport and layout parameters
    let viewport_width: i32 = 800; // placeholder viewport width
    let padding: i32 = 8;
    let content_width = viewport_width - padding * 2;
    let line_height: i32 = 16;
    let char_width: i32 = 8;
    let v_gap: i32 = 4; // gap between stacked blocks

    // Traverse children of root and compute stacked layout
    let mut y_cursor: i32 = padding;
    let mut laid_out_boxes: usize = 0;

    // DFS using a stack to process blocks recursively in-order
    fn layout_node(
        node: NodeKey,
        kind_by_key: &HashMap<NodeKey, LayoutNodeKind>,
        children_by_key: &HashMap<NodeKey, Vec<NodeKey>>,
        content_width: i32,
        line_height: i32,
        char_width: i32,
        v_gap: i32,
        y_cursor: &mut i32,
        laid_out_boxes: &mut usize,
    ) {
        match kind_by_key.get(&node) {
            Some(LayoutNodeKind::Block { .. }) => {
                // Enter block: position at current y, full content width
                *laid_out_boxes += 1;
                *y_cursor += v_gap; // top gap
                if let Some(children) = children_by_key.get(&node) {
                    for child in children {
                        layout_node(
                            child.clone(),
                            kind_by_key,
                            children_by_key,
                            content_width,
                            line_height,
                            char_width,
                            v_gap,
                            y_cursor,
                            laid_out_boxes,
                        );
                    }
                }
                *y_cursor += v_gap; // bottom gap
            }
            Some(LayoutNodeKind::InlineText { text }) => {
                // Anonymous block: single line; width approximated by text length
                let _approx_width = (text.chars().count() as i32 * char_width).min(content_width);
                let _height = line_height;
                *laid_out_boxes += 1;
                *y_cursor += _height + v_gap;
            }
            Some(LayoutNodeKind::Document) | None => {
                // Skip: layout starts below document root
            }
        }
    }

    for child in root_children.iter() {
        layout_node(
            child.clone(),
            &kind_by_key,
            &children_by_key,
            content_width,
            line_height,
            char_width,
            v_gap,
            &mut y_cursor,
            &mut laid_out_boxes,
        );
    }

    laid_out_boxes
}

/// A simple rectangle for layout geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Compute per-node geometry (x, y, width, height) for a simple stacked layout.
/// Returns a map from NodeKey to its LayoutRect. Geometry excludes artificial vertical gaps.
pub fn compute_layout_geometry(layouter: &Layouter) -> HashMap<NodeKey, LayoutRect> {
    let snapshot = layouter.snapshot();
    let mut rects: HashMap<NodeKey, LayoutRect> = HashMap::new();
    if snapshot.is_empty() {
        return rects;
    }

    // Build lookup maps
    let mut kind_by_key = HashMap::new();
    let mut children_by_key: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();
    for (key, kind, children) in snapshot.into_iter() {
        kind_by_key.insert(key.clone(), kind);
        children_by_key.insert(key, children);
    }

    // Attributes per node (for inline style parsing)
    let root = NodeKey::ROOT;

    // Layout parameters aligned with test/Chromium expectations (with reset: margins/padding = 0)
    let args = ComputeGeomArgs {
        // Align with Chromium's content width (accounts for vertical scrollbar ~16px on 800px window)
        viewport_width: 784,
        body_margin: 0,
        // Approximate single-line height to match Chromium snapshot rounding
        line_height: 18,
        char_width: 9,
        v_gap: 0,
    };

    let maps = LayoutMaps {
        kind_by_key: &kind_by_key,
        children_by_key: &children_by_key,
        computed_by_key: Some(layouter.computed_styles()),
    };

    let mut y_cursor = 0;
    // Start from root's children
    if let Some(children) = children_by_key.get(&root) {
        for child in children {
            layout_node(*child, 0, &maps, &mut rects, args, &mut y_cursor);
        }
    }

    rects
}
