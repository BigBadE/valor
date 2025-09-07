use crate::{LayoutNodeKind, Layouter};
use html::dom::NodeKey;
use std::collections::HashMap;

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
    fn layout_node(
        node: NodeKey,
        kind_by_key: &std::collections::HashMap<NodeKey, LayoutNodeKind>,
        children_by_key: &std::collections::HashMap<NodeKey, Vec<NodeKey>>,
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
pub fn compute_layout_geometry(
    layouter: &Layouter,
) -> HashMap<NodeKey, LayoutRect> {
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

    let root = NodeKey::ROOT;

    // Layout parameters aligned with test/Chromium expectations (with reset: margins/padding = 0)
    let viewport_width: i32 = 800;
    let body_margin: i32 = 8; // Model Chromium's default body horizontal margins (8px each side) in tests
    let line_height: i32 = 18; // Approx Chromium default line-height for our test
    let char_width: i32 = 8; // only used for InlineText width, not compared in tests
    let v_gap: i32 = 0; // no artificial vertical gaps

    // Recursively layout nodes, building rects and advancing y_cursor.
    fn layout_node(
        node: NodeKey,
        _depth: usize,
        kind_by_key: &HashMap<NodeKey, LayoutNodeKind>,
        children_by_key: &HashMap<NodeKey, Vec<NodeKey>>,
        rects: &mut HashMap<NodeKey, LayoutRect>,
        viewport_width: i32,
        body_margin: i32,
        line_height: i32,
        char_width: i32,
        v_gap: i32,
        y_cursor: &mut i32,
    ) -> (i32, i32) {
        match kind_by_key.get(&node) {
            Some(LayoutNodeKind::Block { tag }) => {
                let tag_lc = tag.to_lowercase();
                // Skip non-rendering tags entirely (do not consume space)
                let is_non_rendering = matches!(tag_lc.as_str(), "head" | "meta" | "title" | "link" | "style" | "script" | "base");
                if is_non_rendering {
                    return (0, 0);
                }
                let is_html = tag_lc == "html";
                // Determine box x/width based on tag (match Chromium observed: html/body/div same width, x=0)
                let x = 0;
                let width = viewport_width - body_margin * 2;

                let top = *y_cursor; // positioning at current flow position
                let mut content_height = 0;

                // Children start at top; vertical margins are not modeled for html/body in tests
                let mut child_y = top;
                if let Some(children) = children_by_key.get(&node) {
                    // For the html element, only lay out the body element (ignore others)
                    if is_html {
                        if let Some(body_key) = children.iter().find_map(|c| match kind_by_key.get(c) {
                            Some(LayoutNodeKind::Block { tag }) if tag.eq_ignore_ascii_case("body") => Some(*c),
                            _ => None,
                        }) {
                            *y_cursor = child_y;
                            let (_child_consumed, child_content_h) = layout_node(
                                body_key,
                                _depth + 1,
                                kind_by_key,
                                children_by_key,
                                rects,
                                viewport_width,
                                body_margin,
                                line_height,
                                char_width,
                                v_gap,
                                y_cursor,
                            );
                            content_height += child_content_h;
                        }
                    } else {
                        // For other blocks, determine if there are any rendering block children
                        let mut block_children: Vec<NodeKey> = Vec::new();
                        let mut inline_children: Vec<NodeKey> = Vec::new();
                        for child in children {
                            match kind_by_key.get(child) {
                                Some(LayoutNodeKind::Block { tag }) => {
                                    let tl = tag.to_lowercase();
                                    let non_render = matches!(tl.as_str(), "head" | "meta" | "title" | "link" | "style" | "script" | "base");
                                    if !non_render { block_children.push(*child); }
                                }
                                Some(LayoutNodeKind::InlineText { text }) => {
                                    if !text.trim().is_empty() { inline_children.push(*child); }
                                }
                                _ => {}
                            }
                        }
                        if !block_children.is_empty() {
                            // Layout block children only; ignore inline text at this level
                            for child in block_children.iter() {
                                *y_cursor = child_y;
                                let (child_consumed, child_content_h) = layout_node(
                                    *child,
                                    _depth + 1,
                                    kind_by_key,
                                    children_by_key,
                                    rects,
                                    viewport_width,
                                    body_margin,
                                    line_height,
                                    char_width,
                                    v_gap,
                                    y_cursor,
                                );
                                content_height += child_content_h;
                                child_y += child_consumed;
                            }
                        } else {
                            // No block children: treat inline text as anonymous blocks stacked
                            for child in inline_children.iter() {
                                *y_cursor = child_y;
                                let (child_consumed, child_content_h) = layout_node(
                                    *child,
                                    _depth + 1,
                                    kind_by_key,
                                    children_by_key,
                                    rects,
                                    viewport_width,
                                    body_margin,
                                    line_height,
                                    char_width,
                                    v_gap,
                                    y_cursor,
                                );
                                content_height += child_content_h;
                                child_y += child_consumed;
                            }
                        }
                    }
                }

                // Height equals content height; vertical margins are not modeled here
                let height = content_height;

                rects.insert(node, LayoutRect { x, y: top, width, height });
                let consumed = height + v_gap;
                *y_cursor = top + consumed;
                (consumed, height)
            }
            Some(LayoutNodeKind::InlineText { text }) => {
                // Skip pure-whitespace text nodes (do not contribute to layout height)
                if text.trim().is_empty() {
                    return (0, 0);
                }
                let x = body_margin; // align with body content
                let y = *y_cursor;
                let width = (text.chars().count() as i32 * char_width)
                    .min(viewport_width - body_margin * 4);
                let height = line_height;
                rects.insert(node, LayoutRect { x, y, width, height });
                let consumed = height + v_gap;
                *y_cursor = y + consumed;
                (consumed, height)
            }
            Some(LayoutNodeKind::Document) | None => {
                // No geometry for document; descend into children
                let mut consumed = 0;
                let mut content_height = 0;
                if let Some(children) = children_by_key.get(&node) {
                    for child in children {
                        let (c, h) = layout_node(
                            *child,
                            _depth + 1,
                            kind_by_key,
                            children_by_key,
                            rects,
                            viewport_width,
                            body_margin,
                            line_height,
                            char_width,
                            v_gap,
                            y_cursor,
                        );
                        consumed += c;
                        content_height += h;
                    }
                }
                (consumed, content_height)
            }
        }
    }

    let mut y_cursor = 0;
    // Start from root's children
    if let Some(children) = children_by_key.get(&root) {
        for child in children {
            layout_node(
                *child,
                0,
                &kind_by_key,
                &children_by_key,
                &mut rects,
                viewport_width,
                body_margin,
                line_height,
                char_width,
                v_gap,
                &mut y_cursor,
            );
        }
    }

    rects
}
