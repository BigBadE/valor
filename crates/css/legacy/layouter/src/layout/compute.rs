//! Core layout traversal and public API entry points.

use std::collections::HashMap;
use js::NodeKey;
use style_engine::SizeSpecified;

use crate::{LayoutNodeKind, Layouter};
use super::args::{ComputeGeomArgs, LayoutMaps};
use super::geometry::LayoutRect;
use super::styles::{LayoutStyles, extract_layout_styles};
use super::inline::layout_inline_children;
use super::block::{layout_block_children, is_non_rendering_tag, measure_descendant_inline_text_width};
use super::flex::layout_flex_children;

/// Recursively layout nodes, building rects and advancing y_cursor.
pub(crate) fn layout_node(
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
            if styles.display_none { return (0, 0); }
            if is_non_rendering_tag(&tag_lowercase) { return (0, 0); }
            let is_html = tag_lowercase == "html";
            let is_body = tag_lowercase == "body";

            // For inline elements, don't create a box; just descend into children
            // Exception: html/body must generate boxes; positioned inline elements also generate boxes per spec
            let is_positioned_current = maps
                .computed_by_key
                .and_then(|m| m.get(&node))
                .map(|cs| !matches!(cs.position, style_engine::Position::Static))
                .unwrap_or(false);
            if styles.display_inline && !is_html && !is_body && !is_positioned_current {
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
            // Honor padding/border on body (but keep html at 0)
            let padding_top = if is_html { 0 } else { styles.padding.top.round() as i32 };
            let padding_right = if is_html { 0 } else { styles.padding.right.round() as i32 };
            let padding_bottom = if is_html { 0 } else { styles.padding.bottom.round() as i32 };
            let padding_left = if is_html { 0 } else { styles.padding.left.round() as i32 };
            let border_top = if is_html { 0 } else { styles.border.top.round() as i32 };
            let border_right = if is_html { 0 } else { styles.border.right.round() as i32 };
            let border_bottom = if is_html { 0 } else { styles.border.bottom.round() as i32 };
            let border_left = if is_html { 0 } else { styles.border.left.round() as i32 };

            let base_width_for_percent = if is_html || is_body { container_content_width } else { (container_content_width - margin_left - margin_right).max(0) };

            // Resolve width with border-box semantics (assume border-box)
            // border_box_width = content + padding + border
            let content_width: i32;
            let border_box_width: i32;
            if is_html {
                content_width = container_content_width;
                border_box_width = container_content_width;
            } else if is_body {
                // For body, subtract padding/border from available width when width:auto
                match styles.width_spec {
                    Some(SizeSpecified::Px(px)) => {
                        border_box_width = px.round() as i32;
                        content_width = (border_box_width - padding_left - padding_right - border_left - border_right).max(0);
                    }
                    Some(SizeSpecified::Percent(p)) => {
                        let bw = (p * container_content_width as f32).round() as i32;
                        border_box_width = bw;
                        content_width = (border_box_width - padding_left - padding_right - border_left - border_right).max(0);
                    }
                    Some(SizeSpecified::Auto) | None => {
                        content_width = (container_content_width - padding_left - padding_right - border_left - border_right).max(0);
                        border_box_width = (content_width + padding_left + padding_right + border_left + border_right).max(0);
                    }
                }
            } else {
                match styles.width_spec {
                    Some(SizeSpecified::Px(px)) => {
                        border_box_width = px.round() as i32;
                        content_width = (border_box_width - padding_left - padding_right - border_left - border_right).max(0);
                    }
                    Some(SizeSpecified::Percent(p)) => {
                        let bw = (p * base_width_for_percent as f32).round() as i32;
                        border_box_width = bw;
                        content_width = (border_box_width - padding_left - padding_right - border_left - border_right).max(0);
                    }
                    Some(SizeSpecified::Auto) | None => {
                        // Auto → fill available content box (available width minus padding+border)
                        content_width = (base_width_for_percent - padding_left - padding_right - border_left - border_right).max(0);
                        border_box_width = (content_width + padding_left + padding_right + border_left + border_right).max(0);
                    }
                }
            }


            // Positioning
            let top = *y_cursor;
            let x_position = if is_html || is_body { 0 } else { margin_left };

            // Compute children layout. Children start at content box top
            let mut content_height = 0;
            let mut child_y = top + if is_html { 0 } else { border_top + padding_top };
            // Flex container path (row)
            if styles.display_flex || styles.display_inline_flex {
                // Resolve container cross size in pixels if explicitly specified
                let container_cross_px: Option<i32> = match styles.height_spec {
                    Some(SizeSpecified::Px(px)) => Some(px.round() as i32),
                    _ => None,
                };
                let (child_content_h, _consumed_h) = layout_flex_children(
                    node,
                    maps,
                    rects,
                    args,
                    x_position + if is_html || is_body { 0 } else { border_left + padding_left },
                    child_y,
                    content_width,
                    container_cross_px,
                );
                content_height += child_content_h;
            } else {
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
                        // For other blocks, process children in DOM order and flush inline runs
                        let mut inline_run: Vec<NodeKey> = Vec::new();
                        let mut positioned_children: Vec<NodeKey> = Vec::new(); // absolute/fixed
                        let mut space_pending = false;
                        for child in children {
                            // Skip non-rendering or display:none
                            let mut display_none = false;
                            let mut child_inline_hint: Option<bool> = None;
                            let mut child_position: Option<style_engine::Position> = None;
                            if let Some(comp_map) = maps.computed_by_key
                                && let Some(cs) = comp_map.get(child)
                            {
                                if cs.display == style_engine::Display::None { display_none = true; }
                                child_inline_hint = Some(cs.display == style_engine::Display::Inline);
                                child_position = Some(cs.position);
                            }
                            if display_none { continue; }

                            // Text node handling
                            if let Some(LayoutNodeKind::InlineText { text }) = maps.kind_by_key.get(child) {
                                if text.trim().is_empty() { space_pending = true; } else { inline_run.push(*child); space_pending = false; }
                                continue;
                            }

                            // Element nodes
                            if let Some(LayoutNodeKind::Block { tag }) = maps.kind_by_key.get(child) {
                                let tl = tag.to_lowercase();
                                if matches!(tl.as_str(), "head" | "meta" | "title" | "link" | "style" | "script" | "base") { continue; }
                                match child_position.unwrap_or(style_engine::Position::Static) {
                                    style_engine::Position::Absolute | style_engine::Position::Fixed => {
                                        positioned_children.push(*child);
                                        // out-of-flow, does not affect inline/block partition or spacing
                                        space_pending = false;
                                    }
                                    _ => {
                                        let child_inline = child_inline_hint.unwrap_or(false);
                                        if child_inline {
                                            inline_run.push(*child);
                                            space_pending = false;
                                        } else {
                                            // Flush any pending inline run before laying out this block
                                            if !inline_run.is_empty() {
                                                let (additional_height, updated_y) = layout_inline_children(
                                                    &inline_run,
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
                                                inline_run.clear();
                                            }
                                            // Lay out the block child
                                            let (_c, h) = layout_node(*child, depth + 1, maps, rects, args, &mut child_y);
                                            content_height += h;
                                            space_pending = false;
                                        }
                                    }
                                }
                            }
                        }
                        // Flush any trailing inline run
                        if !inline_run.is_empty() {
                            let (additional_height, updated_y) = layout_inline_children(
                                &inline_run,
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
                    }
                }
            }

            // Finalize this block's box dimensions and top position
            let (final_top, final_width, final_height) = calculate_final_dimensions_and_position(
                node,
                maps,
                rects,
                &styles,
                content_height,
                border_box_width,
                top,
                padding_top,
                padding_bottom,
                border_top,
                border_bottom,
                is_html,
                is_body,
            );

            rects.insert(node, LayoutRect { x: x_position, y: final_top, width: final_width, height: final_height });

            // After establishing this node's rect, lay out out-of-flow positioned children (absolute/fixed)
            if let Some(children) = maps.children_by_key.get(&node) {
                // Helper: resolve SizeSpecified to pixels given a reference size
                let resolve_len = |spec_opt: &Option<SizeSpecified>, reference: i32| -> i32 {
                    match spec_opt {
                        Some(SizeSpecified::Px(px)) => px.round() as i32,
                        Some(SizeSpecified::Percent(p)) => ((*p) * reference as f32).round() as i32,
                        _ => 0,
                    }
                };
                // Containing block for absolute: if current node is positioned (not static), use it; else viewport
                let node_position = maps
                    .computed_by_key
                    .and_then(|m| m.get(&node))
                    .map(|cs| cs.position)
                    .unwrap_or(style_engine::Position::Static);
                let parent_rect = rects.get(&node).cloned().unwrap_or(LayoutRect { x: x_position, y: final_top, width: final_width, height: final_height });
                let viewport_rect = LayoutRect { x: 0, y: 0, width: args.viewport_width, height: i32::MAX / 4 };

                // Identify positioned children we collected earlier
                // Note: We recompute here to avoid storing extra lists across scopes if needed
                let mut positioned_children: Vec<NodeKey> = Vec::new();
                for child in children {
                    if let Some(comp_map) = maps.computed_by_key
                        && let Some(cs) = comp_map.get(child)
                        && matches!(cs.position, style_engine::Position::Absolute | style_engine::Position::Fixed)
                    {
                        positioned_children.push(*child);
                    }
                }

                for child in positioned_children {
                    let cs_opt = maps.computed_by_key.and_then(|m| m.get(&child));
                    // Establish containing block
                    let (cb_x, cb_y, cb_w, cb_h, _pos_kind) = if let Some(cs) = cs_opt {
                        let cb = if matches!(cs.position, style_engine::Position::Fixed) {
                            viewport_rect
                        } else if !matches!(node_position, style_engine::Position::Static) {
                            parent_rect
                        } else {
                            viewport_rect
                        };
                        (cb.x, cb.y, cb.width, cb.height, cs.position)
                    } else {
                        (parent_rect.x, parent_rect.y, parent_rect.width, parent_rect.height, style_engine::Position::Static)
                    };

                    // Resolve offsets
                    let (mut left_px, top_px, right_px) = if let Some(cs) = cs_opt {
                        let l = resolve_len(&cs.left, cb_w);
                        let t = resolve_len(&cs.top, cb_h);
                        let r = resolve_len(&cs.right, cb_w);
                        (cb_x + l, cb_y + t, r)
                    } else { (cb_x, cb_y, 0) };

                    // Determine size
                    let (mut w, mut h) = (0, 0);
                    if let Some(cs) = cs_opt {
                        // Width: prefer explicit; otherwise derive from left/right when present
                        w = match cs.width {
                            SizeSpecified::Px(px) => px.round() as i32,
                            SizeSpecified::Percent(p) => (p * cb_w as f32).round() as i32,
                            SizeSpecified::Auto => 0,
                        };
                        if w == 0 {
                            let has_left = cs.left.is_some();
                            let has_right = cs.right.is_some();
                            if has_left && has_right {
                                let l = left_px - cb_x;
                                w = (cb_w - l - right_px).max(0);
                            } else if has_left {
                                w = (cb_w - (left_px - cb_x)).max(0);
                            } else if has_right {
                                // align to right edge
                                left_px = cb_x;
                                w = (cb_w - right_px).max(0);
                            } else {
                                // fill containing block
                                left_px = cb_x;
                                w = cb_w.max(0);
                            }
                        }
                        h = match cs.height { SizeSpecified::Px(px) => px.round() as i32, _ => 0 };
                    }
                    if w == 0 {
                        // Approximate using inline text measurement
                        let fs = cs_opt.map(|c| c.font_size).unwrap_or(16.0);
                        w = measure_descendant_inline_text_width(child, maps, args.char_width, fs);
                    }
                    if h == 0 {
                        let fs = cs_opt.map(|c| c.font_size).unwrap_or(16.0);
                        let lh = cs_opt.map(|c| (c.line_height * fs).round() as i32).unwrap_or((1.2 * fs).round() as i32);
                        h = lh.max(1);
                    }
                    rects.insert(child, LayoutRect { x: left_px, y: top_px, width: w.max(0), height: h.max(0) });
                }
            }


            // Advance y_cursor for subsequent siblings
            let consumed = if is_html || is_body { final_height } else { (final_height).max(0) };
            *y_cursor = top + consumed;
            (consumed, content_height)
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

/// Calculate final dimensions and positioning for a layout node.
/// Returns (final_top, final_width, final_height).
#[allow(clippy::too_many_arguments)]
fn calculate_final_dimensions_and_position(
    node: NodeKey,
    maps: &LayoutMaps,
    rects: &HashMap<NodeKey, LayoutRect>,
    styles: &LayoutStyles,
    content_height: i32,
    border_box_width: i32,
    top: i32,
    padding_top: i32,
    padding_bottom: i32,
    border_top: i32,
    border_bottom: i32,
    is_html: bool,
    is_body: bool,
) -> (i32, i32, i32) {
    // Height resolution
    let mut used_height = content_height;
    if let Some(SizeSpecified::Px(px)) = styles.height_spec { used_height = px.round() as i32; }
    
    // Border-box dimensions
    let mut border_height = if is_html || is_body { used_height } else { (used_height + padding_top + padding_bottom + border_top + border_bottom).max(0) };

    let mut out_top = top;
    
    // Adjust body box to wrap children (approximate margin-collapsing with first/last child)
    if is_body {
        let mut min_y = i32::MAX;
        let mut max_yh = 0;
        if let Some(children) = maps.children_by_key.get(&node) {
            for child in children {
                if matches!(maps.kind_by_key.get(child), Some(LayoutNodeKind::Block { .. }))
                    && let Some(rect) = rects.get(child)
                {
                    if rect.y < min_y { min_y = rect.y; }
                    if rect.y + rect.height > max_yh { max_yh = rect.y + rect.height; }
                }
            }
        }
        // If parent has top border/padding, do not shift top; margin collapsing does not escape border-box.
        if (padding_top + border_top) == 0 {
            if min_y != i32::MAX && max_yh >= min_y {
                out_top = min_y;
                border_height = max_yh - out_top;
            } else {
                // Fallback: derive from first block child's top margin if available
                if let Some(children) = maps.children_by_key.get(&node)
                    && let Some(first_block) = children.iter().find(|c| matches!(maps.kind_by_key.get(c), Some(LayoutNodeKind::Block { .. })))
                    && let Some(computed_map) = maps.computed_by_key
                    && let Some(computed_style) = computed_map.get(first_block)
                {
                    let margin_top = computed_style.margin.top.round() as i32;
                    if margin_top > 0 { out_top = margin_top; border_height = (max_yh - out_top).max(0); }
                }
            }
        } else {
            // Fallback: derive from first block child's top margin if available
            out_top = top;
        }
    } else {
        // For regular blocks, keep the box's top as positioned without collapsing to children's min_y.
        // This better matches Chromium's bounding rects for our simplified model.
        out_top = top;
    }

    (out_top, border_box_width, border_height)
}

/// Perform a simple block layout pass over the mirrored DOM using only the public API.
///
/// This implementation intentionally uses only the public Layouter API (snapshot)
/// and does not mutate internal state. It simulates a block formatting context:
/// - The root establishes a viewport of fixed width.
/// - Block elements are stacked vertically in document order.
/// - InlineText becomes an anonymous block with a single line of fixed line-height.
///   The function returns the number of laid-out boxes (excluding the root document node).
pub fn compute_simple_layout(layouter: &Layouter) -> usize {
    let snapshot = layouter.snapshot();
    if snapshot.is_empty() { return 0; }

    // Build temporary lookup maps from the snapshot
    let mut kind_by_key = HashMap::new();
    let mut children_by_key = HashMap::new();
    for (key, kind, children) in snapshot.into_iter() {
        kind_by_key.insert(key, kind);
        children_by_key.insert(key, children);
    }

    // Ensure we have the root
    let root = NodeKey::ROOT;
    let Some(root_children) = children_by_key.get(&root) else { return 0; };

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
    #[allow(clippy::too_many_arguments)]
    fn layout_node_simple(
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
                        layout_node_simple(
                            *child,
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
        layout_node_simple(
            *child,
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

/// Compute per-node geometry (x, y, width, height) for a simple stacked layout.
/// Returns a map from NodeKey to its LayoutRect. Geometry excludes artificial vertical gaps.
pub fn compute_layout_geometry(layouter: &Layouter) -> HashMap<NodeKey, LayoutRect> {
    // Build a LayoutBox tree first (Phase 2 scaffold), then derive NodeKey-based maps
    let box_tree = super::boxes::build_layout_box_tree(layouter);
    let mut rects: HashMap<NodeKey, LayoutRect> = HashMap::new();
    // If no boxes besides the anonymous root, return empty
    if box_tree.boxes.len() <= 1 { return rects; }

    let (kind_by_key, children_by_key) = super::boxes::derive_maps_from_box_tree(&box_tree);

    // Attributes per node (for inline style parsing)
    let root = NodeKey::ROOT;

    // Layout parameters aligned with test/Chromium expectations (with reset: margins/padding = 0)
    // IMPORTANT: Do not subtract scrollbar thickness preemptively. Use the full viewport width;
    // scrollbars (if any) are a painting/scrollport concern and should be gated on actual overflow.
    let attrs_map = layouter.attrs_map();
    let viewport_width = 784;
    let args = ComputeGeomArgs {
        viewport_width,
        body_margin: 0,
        // Approximate single-line height to match Chromium snapshot rounding; use 20px for integral line boxes
        line_height: 20,
        char_width: 9,
        v_gap: 0,
    };

    let maps = LayoutMaps { kind_by_key: &kind_by_key, children_by_key: &children_by_key, computed_by_key: Some(layouter.computed_styles()), attrs_by_key: &attrs_map };

    // Debug: if node with id="inner" exists, log its computed height spec before geometry
    for (k, attrs) in &attrs_map {
        if attrs.get("id").map(|v| v == "inner").unwrap_or(false)
            && let Some(comp_map) = maps.computed_by_key
            && let Some(cs) = comp_map.get(k)
        {
            log::info!("Layouter debug: id=inner height_spec={:?} width_spec={:?}", cs.height, cs.width);
        }
    }

    let mut y_cursor = 0;
    // Start from root's children if present; otherwise, start from box tree top-level DOM nodes
    let start_nodes: Vec<NodeKey> = if let Some(children) = children_by_key.get(&root) {
        children.clone()
    } else {
        box_tree.boxes[box_tree.root.0 as usize]
            .children
            .iter()
            .filter_map(|id| box_tree.boxes[id.0 as usize].dom_node)
            .collect()
    };
    for child in start_nodes.iter() {
        layout_node(*child, 0, &maps, &mut rects, args, &mut y_cursor);
    }

    // Note: We intentionally do not apply overflow:hidden clipping during geometry export.
    // Chromium's getBoundingClientRect for layout comparison retains the child's used size;
    // clipping is a painting concern and should be handled by the renderer, not geometry.

    rects
}
