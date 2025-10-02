use crate::display::{push_border_items, push_text_item, stacking_boundary_for, z_key_for_child};
use crate::snapshots::IRect;
use css::style_types::{ComputedStyle, Overflow};
use css_core::{LayoutNodeKind, LayoutRect};
use js::NodeKey;
use log::debug;
use renderer::{DisplayItem, DisplayList};
use std::collections::HashMap;

/// Walker context bundling borrowed maps for display list generation.
pub struct WalkCtx<'context> {
    /// Maps node keys to their layout kind.
    pub kind_map: &'context HashMap<NodeKey, LayoutNodeKind>,
    /// Maps parent node keys to their children.
    pub children_map: &'context HashMap<NodeKey, Vec<NodeKey>>,
    /// Maps node keys to their computed layout rectangles.
    pub rects: &'context HashMap<NodeKey, LayoutRect>,
    /// Primary map of computed styles from the CSS engine.
    pub computed_map: &'context HashMap<NodeKey, ComputedStyle>,
    /// Fallback computed styles for nodes missing primary styles.
    pub computed_fallback: &'context HashMap<NodeKey, ComputedStyle>,
    /// Optional robust computed styles (third-tier fallback).
    pub computed_robust: Option<&'context HashMap<NodeKey, ComputedStyle>>,
    /// Maps child node keys to their parent for upward traversal.
    pub parent_map: &'context HashMap<NodeKey, NodeKey>,
}

/// Finds nearest ancestor with a computed style (for inline text inheritance).
fn nearest_style<'style>(
    start: NodeKey,
    parent_map: &HashMap<NodeKey, NodeKey>,
    computed_map: &'style HashMap<NodeKey, ComputedStyle>,
    computed_fallback: &'style HashMap<NodeKey, ComputedStyle>,
    computed_robust: Option<&'style HashMap<NodeKey, ComputedStyle>>,
) -> Option<&'style ComputedStyle> {
    let mut current = Some(start);
    while let Some(node) = current {
        if let Some(computed_style) = computed_robust
            .and_then(|map| map.get(&node))
            .or_else(|| computed_fallback.get(&node))
            .or_else(|| computed_map.get(&node))
        {
            return Some(computed_style);
        }
        current = parent_map.get(&node).copied();
    }
    None
}

/// Finds nearest ancestor with a layout rect (inline text nodes lack rects).
fn nearest_rect(
    start: NodeKey,
    parent_map: &HashMap<NodeKey, NodeKey>,
    rects: &HashMap<NodeKey, LayoutRect>,
) -> Option<LayoutRect> {
    let mut current = Some(start);
    while let Some(node) = current {
        if let Some(rect) = rects.get(&node) {
            return Some(*rect);
        }
        current = parent_map.get(&node).copied();
    }
    None
}

/// Orders children by z-index stacking buckets for correct paint order.
fn order_children(
    children: &[NodeKey],
    parent_map: &HashMap<NodeKey, NodeKey>,
    computed_map: &HashMap<NodeKey, ComputedStyle>,
    computed_fallback: &HashMap<NodeKey, ComputedStyle>,
    computed_robust: Option<&HashMap<NodeKey, ComputedStyle>>,
) -> Vec<NodeKey> {
    let mut ordered: Vec<NodeKey> = children.to_vec();
    ordered.sort_by_key(|child| {
        z_key_for_child(
            *child,
            parent_map,
            computed_map,
            computed_fallback,
            computed_robust,
        )
    });
    ordered
}

/// Processes children in z-index paint order.
fn process_children(list: &mut DisplayList, node: NodeKey, ctx: &WalkCtx<'_>) {
    if let Some(children) = ctx.children_map.get(&node) {
        let ordered = order_children(
            children,
            ctx.parent_map,
            ctx.computed_map,
            ctx.computed_fallback,
            ctx.computed_robust,
        );
        for child in ordered {
            recurse(list, child, ctx);
        }
    }
}

/// Applies overflow clipping to a display list if the style requires it.
fn apply_overflow_clip(
    list: &mut DisplayList,
    rect: &LayoutRect,
    style_for_node: Option<&ComputedStyle>,
) -> bool {
    if let Some(computed_style) = style_for_node
        && matches!(
            computed_style.overflow,
            Overflow::Hidden | Overflow::Clip | Overflow::Auto | Overflow::Scroll
        )
    {
        let pad_left = computed_style.padding.left.max(0.0);
        let pad_top = computed_style.padding.top.max(0.0);
        let pad_right = computed_style.padding.right.max(0.0);
        let pad_bottom = computed_style.padding.bottom.max(0.0);
        let border_left = computed_style.border_width.left.max(0.0);
        let border_top = computed_style.border_width.top.max(0.0);
        let border_right = computed_style.border_width.right.max(0.0);
        let border_bottom = computed_style.border_width.bottom.max(0.0);
        let clip_x = rect.x + border_left + pad_left;
        let clip_y = rect.y + border_top + pad_top;
        let clip_width =
            (rect.width - (border_left + pad_left + pad_right + border_right)).max(0.0);
        let clip_height =
            (rect.height - (border_top + pad_top + pad_bottom + border_bottom)).max(0.0);
        list.push(DisplayItem::BeginClip {
            x: clip_x,
            y: clip_y,
            width: clip_width,
            height: clip_height,
        });
        true
    } else {
        false
    }
}

/// Processes a block node with a rect (background, borders, clip, stacking context).
fn process_block_with_rect(
    list: &mut DisplayList,
    node: NodeKey,
    rect: &LayoutRect,
    computed_style_opt: Option<&ComputedStyle>,
    ctx: &WalkCtx<'_>,
) {
    let mut opened_ctx = false;
    let boundary_opt = computed_style_opt.and_then(stacking_boundary_for);
    if let Some(boundary) = boundary_opt {
        list.push(DisplayItem::BeginStackingContext { boundary });
        opened_ctx = true;
    }
    let fill_rgba_opt = computed_style_opt.map(|computed_style| {
        let background = computed_style.background_color;
        [
            f32::from(background.red) / 255.0,
            f32::from(background.green) / 255.0,
            f32::from(background.blue) / 255.0,
            f32::from(background.alpha) / 255.0,
        ]
    });
    if let Some(fill_rgba) = fill_rgba_opt.filter(|rgba| rgba[3] > 0.0) {
        list.push(DisplayItem::Rect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
            color: fill_rgba,
        });
    }
    if let Some(computed_style) = computed_style_opt {
        push_border_items(list, rect, computed_style);
    }
    let style_for_node = computed_style_opt.or_else(|| {
        nearest_style(
            node,
            ctx.parent_map,
            ctx.computed_map,
            ctx.computed_fallback,
            ctx.computed_robust,
        )
    });
    let opened_clip = apply_overflow_clip(list, rect, style_for_node);
    process_children(list, node, ctx);
    if opened_clip {
        list.push(DisplayItem::EndClip);
    }
    if opened_ctx {
        list.push(DisplayItem::EndStackingContext);
    }
}

/// Processes a block node without a rect.
fn process_block_without_rect(
    list: &mut DisplayList,
    node: NodeKey,
    computed_style_opt: Option<&ComputedStyle>,
    ctx: &WalkCtx<'_>,
) {
    let style_for_node_ctx = computed_style_opt.or_else(|| {
        nearest_style(
            node,
            ctx.parent_map,
            ctx.computed_map,
            ctx.computed_fallback,
            ctx.computed_robust,
        )
    });
    let mut opened_ctx = false;
    let boundary_opt = style_for_node_ctx.and_then(stacking_boundary_for);
    if let Some(boundary) = boundary_opt {
        list.push(DisplayItem::BeginStackingContext { boundary });
        opened_ctx = true;
    }
    process_children(list, node, ctx);
    if opened_ctx {
        list.push(DisplayItem::EndStackingContext);
    }
}

/// Recursive tree walker that emits display items for each node.
fn recurse(list: &mut DisplayList, node: NodeKey, ctx: &WalkCtx<'_>) {
    let Some(kind) = ctx.kind_map.get(&node) else {
        return;
    };
    match kind {
        LayoutNodeKind::Document => {
            if let Some(children) = ctx.children_map.get(&node) {
                for &child in children {
                    recurse(list, child, ctx);
                }
            }
        }
        LayoutNodeKind::Block { .. } => {
            let rect_opt = ctx.rects.get(&node);
            let computed_style_opt = ctx
                .computed_robust
                .as_ref()
                .and_then(|map| map.get(&node))
                .or_else(|| ctx.computed_fallback.get(&node))
                .or_else(|| ctx.computed_map.get(&node));
            if let Some(rect) = rect_opt {
                process_block_with_rect(list, node, rect, computed_style_opt, ctx);
            } else {
                process_block_without_rect(list, node, computed_style_opt, ctx);
            }
        }
        LayoutNodeKind::InlineText { text } => {
            if text.trim().is_empty() {
                return;
            }
            if let Some(rect) = nearest_rect(node, ctx.parent_map, ctx.rects) {
                let (font_size, color_rgb) = nearest_style(
                    node,
                    ctx.parent_map,
                    ctx.computed_map,
                    ctx.computed_fallback,
                    ctx.computed_robust,
                )
                .map_or((16.0, [0.0, 0.0, 0.0]), |computed_style| {
                    let text_color = computed_style.color;
                    (
                        computed_style.font_size,
                        [
                            f32::from(text_color.red) / 255.0,
                            f32::from(text_color.green) / 255.0,
                            f32::from(text_color.blue) / 255.0,
                        ],
                    )
                });
                push_text_item(list, &rect, text, font_size, color_rgb);
            }
        }
    }
}

/// Renders selection overlay as semi-transparent blue rectangles.
fn render_selection_overlay(
    list: &mut DisplayList,
    rects: &HashMap<NodeKey, LayoutRect>,
    selection_overlay: IRect,
) {
    let (x0_coord, y0_coord, x1_coord, y1_coord) = selection_overlay;
    let selection_x = x0_coord.min(x1_coord) as f32;
    let selection_y = y0_coord.min(y1_coord) as f32;
    let selection_width = (x0_coord.max(x1_coord) - selection_x.round() as i32).max(0i32) as f32;
    let selection_height = (y0_coord.max(y1_coord) - selection_y.round() as i32).max(0i32) as f32;
    let selection = LayoutRect {
        x: selection_x,
        y: selection_y,
        width: selection_width,
        height: selection_height,
    };
    #[allow(
        clippy::iter_over_hash_type,
        reason = "HashMap iteration order doesn't matter for intersection checks"
    )]
    for rect in rects.values() {
        let intersect_x = rect.x.max(selection.x);
        let intersect_y = rect.y.max(selection.y);
        let intersect_right = (rect.x + rect.width).min(selection.x + selection.width);
        let intersect_bottom = (rect.y + rect.height).min(selection.y + selection.height);
        let intersect_width = (intersect_right - intersect_x).max(0.0);
        let intersect_height = (intersect_bottom - intersect_y).max(0.0);
        if intersect_width > 0.0 && intersect_height > 0.0 {
            list.push(DisplayItem::Rect {
                x: intersect_x,
                y: intersect_y,
                width: intersect_width,
                height: intersect_height,
                color: [0.2, 0.5, 1.0, 0.35],
            });
        }
    }
}

/// Renders focus ring as a 4-sided border around the focused element.
fn render_focus_ring(
    list: &mut DisplayList,
    rects: &HashMap<NodeKey, LayoutRect>,
    focused_node: NodeKey,
) {
    if let Some(focused_rect) = rects.get(&focused_node) {
        let focus_x = focused_rect.x;
        let focus_y = focused_rect.y;
        let focus_width = focused_rect.width;
        let focus_height = focused_rect.height;
        let focus_color = [0.2, 0.4, 1.0, 1.0];
        let focus_thickness = 2.0f32;
        list.push(DisplayItem::Rect {
            x: focus_x,
            y: focus_y,
            width: focus_width,
            height: focus_thickness,
            color: focus_color,
        });
        list.push(DisplayItem::Rect {
            x: focus_x,
            y: focus_y + focus_height - focus_thickness,
            width: focus_width,
            height: focus_thickness,
            color: focus_color,
        });
        list.push(DisplayItem::Rect {
            x: focus_x,
            y: focus_y,
            width: focus_thickness,
            height: focus_height,
            color: focus_color,
        });
        list.push(DisplayItem::Rect {
            x: focus_x + focus_width - focus_thickness,
            y: focus_y,
            width: focus_thickness,
            height: focus_height,
            color: focus_color,
        });
    }
}

/// Builds the display tree by recursively traversing the layout tree.
pub fn build_tree(
    list: &mut DisplayList,
    kind_map: &HashMap<NodeKey, LayoutNodeKind>,
    children_map: &HashMap<NodeKey, Vec<NodeKey>>,
    rects: &HashMap<NodeKey, LayoutRect>,
    computed_map: &HashMap<NodeKey, ComputedStyle>,
    computed_fallback: &HashMap<NodeKey, ComputedStyle>,
    computed_robust: Option<&HashMap<NodeKey, ComputedStyle>>,
    parent_map: &HashMap<NodeKey, NodeKey>,
) {
    let ctx = WalkCtx {
        kind_map,
        children_map,
        rects,
        computed_map,
        computed_fallback,
        computed_robust,
        parent_map,
    };
    recurse(list, NodeKey::ROOT, &ctx);
    debug!(
        "[DL DEBUG] build_retained produced items={}",
        list.items.len()
    );
}

/// Adds selection overlay to the display list if present.
pub fn add_selection_overlay(
    list: &mut DisplayList,
    rects: &HashMap<NodeKey, LayoutRect>,
    selection_overlay: Option<IRect>,
) {
    if let Some(overlay) = selection_overlay {
        render_selection_overlay(list, rects, overlay);
    }
}

/// Adds focus ring to the display list if a node is focused.
pub fn add_focus_ring(
    list: &mut DisplayList,
    rects: &HashMap<NodeKey, LayoutRect>,
    focused_node: Option<NodeKey>,
) {
    if let Some(focused) = focused_node {
        render_focus_ring(list, rects, focused);
    }
}

/// Adds HUD (heads-up display) with performance metrics if enabled.
pub fn add_hud(
    list: &mut DisplayList,
    hud_enabled: bool,
    spillover_deferred: u64,
    last_style_restyled_nodes: u64,
) {
    if hud_enabled {
        let hud = format!("restyled:{last_style_restyled_nodes} spill:{spillover_deferred}");
        list.push(DisplayItem::Text {
            x: 6.0,
            y: 14.0,
            text: hud,
            color: [0.1, 0.1, 0.1],
            font_size: 12.0,
            bounds: None,
        });
    }
}
