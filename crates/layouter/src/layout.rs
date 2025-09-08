use crate::{LayoutNodeKind, Layouter};
use css::parser::parse_declarations;
use css::values::{LengthContext, to_length_in_px_rounded_i32};
use html::dom::NodeKey;
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
    pub attrs_by_key: &'a HashMap<NodeKey, HashMap<String, String>>,
    pub computed_by_key: Option<&'a HashMap<NodeKey, ComputedStyle>>,
}

/// General inline style parser for single length attributes (e.g., width, height, margin-top in the future)
/// Converts the attribute value to px using LengthContext and containing block width as percent base.
pub fn parse_inline_length_attr(
    attrs_by_key: &HashMap<NodeKey, HashMap<String, String>>,
    node: NodeKey,
    attr_name: &str,
    containing_block_width: i32,
) -> Option<i32> {
    let style_text = attrs_by_key.get(&node)?.get("style")?.to_string();
    let decls = parse_declarations(&style_text);
    for d in decls {
        if d.name.eq_ignore_ascii_case(attr_name) {
            let v = d.value.trim();
            let ctx = LengthContext {
                percent_base_px: containing_block_width as f32,
                ..LengthContext::default()
            };
            if let Some(px) = to_length_in_px_rounded_i32(v, &ctx) {
                return Some(px);
            }
        }
    }
    None
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
            let tag_lc = tag.to_lowercase();
            // Read computed style if available
            let mut display_inline = false;
            let mut display_none = false;
            let mut margin = Edges { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 };
            let mut padding = Edges { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 };
            let mut width_spec: Option<SizeSpecified> = None;
            let mut height_spec: Option<SizeSpecified> = None;
            if let Some(comp_map) = maps.computed_by_key {
                if let Some(cs) = comp_map.get(&node) {
                    display_none = cs.display == Display::None;
                    display_inline = cs.display == Display::Inline;
                    margin = cs.margin;
                    padding = cs.padding;
                    width_spec = Some(cs.width);
                    height_spec = Some(cs.height);
                }
            }
            if display_none { return (0, 0); }
            // Skip non-rendering tags entirely (do not consume space)
            let is_non_rendering = matches!(
                tag_lc.as_str(),
                "head" | "meta" | "title" | "link" | "style" | "script" | "base"
            );
            if is_non_rendering { return (0, 0); }
            let is_html = tag_lc == "html";
            let is_body = tag_lc == "body";

            // For inline elements, don't create a box; just descend into children
            if display_inline {
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
            // Margins for non-root blocks
            let ml = if is_html || is_body { 0 } else { margin.left.round() as i32 };
            let mr = if is_html || is_body { 0 } else { margin.right.round() as i32 };
            let pt = if is_html || is_body { 0 } else { padding.top.round() as i32 };
            let pr = if is_html || is_body { 0 } else { padding.right.round() as i32 };
            let pb = if is_html || is_body { 0 } else { padding.bottom.round() as i32 };
            let pl = if is_html || is_body { 0 } else { padding.left.round() as i32 };

            let base_width_for_percent = if is_html || is_body { container_content_width } else { (container_content_width - ml - mr).max(0) };

            let mut content_width = if is_html || is_body { container_content_width } else { base_width_for_percent };

            if !is_html && !is_body {
                let mut used_from_computed = false;
                if let Some(ws) = width_spec {
                    match ws {
                        SizeSpecified::Auto => { /* resolve in fallback */ }
                        SizeSpecified::Px(px) => {
                            // Border-box semantics: content width is specified minus padding
                            content_width = (px.round() as i32 - pl - pr).max(0);
                            used_from_computed = true;
                        }
                        SizeSpecified::Percent(p) => {
                            let bw = (p * base_width_for_percent as f32).round() as i32;
                            content_width = (bw - pl - pr).max(0);
                            used_from_computed = true;
                        }
                    }
                }
                if !used_from_computed {
                    if let Some(px) = parse_inline_length_attr(
                        maps.attrs_by_key,
                        node,
                        "width",
                        base_width_for_percent,
                    ) {
                        // Treat inline parsed width as border-box
                        content_width = (px - pl - pr).max(0);
                    } else {
                        // Auto → fill available border box
                        content_width = (base_width_for_percent - pl - pr).max(0);
                    }
                }
            }

            // Positioning
            let top = if is_html || is_body { *y_cursor } else { *y_cursor + (margin.top.round() as i32) };
            let x = if is_html || is_body { 0 } else { ml };

            // Compute children layout. Children start at content box top
            let mut content_height = 0;
            let mut child_y = top + if is_html || is_body { 0 } else { pt };
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
                    for child in children {
                        match maps.kind_by_key.get(child) {
                            Some(LayoutNodeKind::Block { tag }) => {
                                let tl = tag.to_lowercase();
                                let non_render = matches!(tl.as_str(), "head" | "meta" | "title" | "link" | "style" | "script" | "base");
                                if non_render { continue; }
                                // Decide inline vs block using computed display if available
                                let mut child_inline = false;
                                if let Some(comp_map) = maps.computed_by_key {
                                    if let Some(cs) = comp_map.get(child) {
                                        if cs.display == Display::None { continue; }
                                        child_inline = cs.display == Display::Inline;
                                    } else {
                                        // No computed style entry: attempt inline style attribute fallback
                                        if let Some(style_text) = maps.attrs_by_key.get(child).and_then(|m| m.get("style")) {
                                            for d in parse_declarations(style_text) {
                                                if d.name.eq_ignore_ascii_case("display") {
                                                    let v = d.value.trim().to_ascii_lowercase();
                                                    match v.as_str() {
                                                        "none" => { continue; }
                                                        "inline" => { child_inline = true; }
                                                        "block" => { child_inline = false; }
                                                        _ => {}
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                // Fallback to tag default inline for common inline tags if style info still missing
                                if !child_inline {
                                    let tli = tl.as_str();
                                    if matches!(tli, "span" | "a" | "b" | "i" | "strong" | "em") {
                                        child_inline = true;
                                    }
                                }
                                if child_inline { inline_children.push(*child); } else { block_children.push(*child); }
                            }
                            Some(LayoutNodeKind::InlineText { text }) => {
                                if !text.trim().is_empty() { inline_children.push(*child); }
                            }
                            _ => {}
                        }
                    }
                    if block_children.is_empty() {
                        let mut inline_x = x + if is_html || is_body { 0 } else { pl };
                        let child_y_inline = child_y;
                        let mut line_h = 0;
                        for child in inline_children {
                            match maps.kind_by_key.get(&child) {
                                Some(LayoutNodeKind::InlineText { text }) => {
                                    let w = (text.chars().count() as i32 * args.char_width).max(0);
                                    let h = args.line_height;
                                    rects.insert(child, LayoutRect { x: inline_x, y: child_y_inline, width: w, height: h });
                                    inline_x += w + (args.char_width / 2);
                                    if h > line_h { line_h = h; }
                                }
                                Some(LayoutNodeKind::Block { .. }) => {
                                    // Inline element: estimate width from specified width or sum of immediate text children
                                    let mut w = 0;
                                    if let Some(comp_map) = maps.computed_by_key {
                                        if let Some(cs) = comp_map.get(&child) {
                                            match cs.width {
                                                SizeSpecified::Px(px) => { w = px.round() as i32; }
                                                SizeSpecified::Percent(p) => { w = (p * content_width as f32).round() as i32; }
                                                SizeSpecified::Auto => {}
                                            }
                                        }
                                    }
                                    if w == 0 {
                                        if let Some(grand) = maps.children_by_key.get(&child) {
                                            let mut sum = 0;
                                            for g in grand {
                                                if let Some(LayoutNodeKind::InlineText { text }) = maps.kind_by_key.get(g) {
                                                    sum += (text.chars().count() as i32 * args.char_width).max(0);
                                                }
                                            }
                                            w = sum;
                                        }
                                    }
                                    let h = args.line_height;
                                    rects.insert(child, LayoutRect { x: inline_x, y: child_y_inline, width: w, height: h });
                                    inline_x += w + (args.char_width / 2);
                                    if h > line_h { line_h = h; }
                                }
                                _ => {}
                            }
                        }
                        content_height += line_h;
                        child_y += line_h;
                    }
                    // Then lay out block children (if any)
                    let children = block_children;
                    for child in children {
                        // For block children, pass down the parent content width as viewport_width for percent resolution
                        let child_args = ComputeGeomArgs { viewport_width: content_width, body_margin: 0, line_height: args.line_height, char_width: args.char_width, v_gap: args.v_gap };
                        *y_cursor = child_y;
                        let (child_consumed, child_content_h) = layout_node(child, depth + 1, maps, rects, child_args, y_cursor);
                        content_height += child_content_h;
                        child_y += child_consumed;
                    }
                }
            }

            // Height resolution
            let mut used_height = content_height;
            if let Some(hs) = height_spec { if let SizeSpecified::Px(px) = hs { used_height = px.round() as i32; } }
            // Border-box dimensions
            let border_width = if is_html || is_body { content_width } else { (content_width + pl + pr).max(0) };
            let mut border_height = if is_html || is_body { used_height } else { (used_height + pt + pb).max(0) };

            let mut out_top = top;
            // Adjust body box to wrap children (approximate margin-collapsing with first/last child)
            if is_body {
                let mut min_y = i32::MAX;
                let mut max_yh = 0;
                if let Some(children) = maps.children_by_key.get(&node) {
                    for child in children {
                        if let Some(r) = rects.get(child) {
                            if r.y < min_y { min_y = r.y; }
                            if r.y + r.height > max_yh { max_yh = r.y + r.height; }
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
                            if let Some(comp_map) = maps.computed_by_key {
                                if let Some(cs) = comp_map.get(first_block) {
                                    let mt = cs.margin.top.round() as i32;
                                    if mt > 0 {
                                        out_top = mt;
                                        border_height = (max_yh - out_top).max(0);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            rects.insert(node, LayoutRect { x, y: out_top, width: border_width, height: border_height });
            let consumed = if is_html || is_body { border_height } else { (margin.top.round() as i32) + border_height + (margin.bottom.round() as i32) } + args.v_gap;
            *y_cursor = out_top + border_height + if is_html || is_body { 0 } else { margin.bottom.round() as i32 } + args.v_gap;
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

/// Perform a simple block layout pass over the mirrored DOM.
///
/// This implementation intentionally uses only the public Layouter API (snapshot)
/// and does not mutate internal state. It simulates a block formatting context:
/// - The root establishes a viewport of fixed width.
/// - Block elements are stacked vertically in document order.
/// - InlineText becomes an anonymous block with a single line of fixed line-height.
/// The function returns the number of laid-out boxes (excluding the root document node).
pub fn compute_layout(layouter: &Layouter) -> usize {
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
    let attrs_by_key: HashMap<NodeKey, HashMap<String, String>> = layouter.attrs_map();

    let root = NodeKey::ROOT;

    // Layout parameters aligned with test/Chromium expectations (with reset: margins/padding = 0)
    let args = ComputeGeomArgs {
        viewport_width: 800,
        body_margin: 8,
        line_height: 17,
        char_width: 6,
        v_gap: 0,
    };

    let maps = LayoutMaps {
        kind_by_key: &kind_by_key,
        children_by_key: &children_by_key,
        attrs_by_key: &attrs_by_key,
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
