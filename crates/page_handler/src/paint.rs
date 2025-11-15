//! Display list generation from layout and computed styles.

use crate::stacking;
use css::style_types::{BorderStyle, ComputedStyle, Overflow};
use css_core::{LayoutNodeKind, LayoutRect};
use js::NodeKey;
use renderer::{DisplayItem, DisplayList};
use std::collections::HashMap;

/// Type alias for a list of child `NodeKey`s.
type NodeChildren = Vec<NodeKey>;
/// Type alias for a node map entry: (kind, children).
type NodeMapEntry = (LayoutNodeKind, NodeChildren);

/// Context for painting nodes.
struct PaintContext<'ctx> {
    /// Layout rectangles for all nodes.
    rects: &'ctx HashMap<NodeKey, LayoutRect>,
    /// Computed styles for all nodes.
    styles: &'ctx HashMap<NodeKey, ComputedStyle>,
    /// Map from node key to kind and children.
    node_map: &'ctx HashMap<NodeKey, NodeMapEntry>,
}

/// Generate a display list from layout geometry and computed styles.
///
/// This function traverses all layout rectangles and emits display items
/// in correct CSS paint order with proper clipping for overflow.
pub fn build_display_list(
    rects: &HashMap<NodeKey, LayoutRect>,
    styles: &HashMap<NodeKey, ComputedStyle>,
    snapshot: &[(NodeKey, LayoutNodeKind, NodeChildren)],
) -> DisplayList {
    let mut items = Vec::new();

    // Build a map from NodeKey to (kind, children) for easier lookup
    let mut node_map: HashMap<NodeKey, NodeMapEntry> = HashMap::new();
    for (key, kind, children) in snapshot {
        node_map.insert(*key, (kind.clone(), children.clone()));
    }

    // Build a parent map for efficient parent lookups
    let mut parent_map: HashMap<NodeKey, NodeKey> = HashMap::new();
    for (parent_key, _, children) in snapshot {
        for child_key in children {
            parent_map.insert(*child_key, *parent_key);
        }
    }

    // Find layout roots: nodes with rects whose parent either doesn't have a rect or doesn't exist
    // The snapshot includes non-layout nodes (like document root NodeKey(0)) without rects.
    let mut layout_roots = Vec::new();
    for key in rects.keys() {
        let has_parent_with_rect = parent_map
            .get(key)
            .is_some_and(|parent| rects.contains_key(parent));

        if !has_parent_with_rect {
            layout_roots.push(*key);
        }
    }

    let ctx = PaintContext {
        rects,
        styles,
        node_map: &node_map,
    };

    // Paint from each layout root
    for root in layout_roots {
        paint_node_recursive(root, &ctx, &mut items);
    }

    DisplayList::from_items(items)
}

/// Recursively paint a node and its descendants in CSS paint order.
fn paint_node_recursive(key: NodeKey, ctx: &PaintContext<'_>, items: &mut Vec<DisplayItem>) {
    let Some(rect) = ctx.rects.get(&key) else {
        return;
    };
    let Some(style) = ctx.styles.get(&key) else {
        return;
    };

    // Check if this element creates a stacking context (opacity or z-index)
    let stacking_boundary = stacking::creates_stacking_context(style);

    // If this creates a stacking context, wrap everything in Begin/End markers
    if let Some(boundary) = stacking_boundary {
        stacking::emit_stacking_context(boundary, items, |items_inner| {
            paint_node_content(key, rect, style, ctx, items_inner);
        });
    } else {
        paint_node_content(key, rect, style, ctx, items);
    }
}

/// Paint a node's content (background, borders, children) in CSS paint order
fn paint_node_content(
    key: NodeKey,
    rect: &LayoutRect,
    style: &ComputedStyle,
    ctx: &PaintContext<'_>,
    items: &mut Vec<DisplayItem>,
) {
    // CSS 2.2 §E.2 and §9.9.1: Paint order
    // 1. background and borders of stacking context root
    // 2. descendants with negative z-index
    // 3. in-flow non-positioned descendants
    // 4. positioned descendants with z-index: auto/0
    // 5. descendants with positive z-index

    // Check if this element has overflow clipping
    let needs_clip = matches!(
        style.overflow,
        Overflow::Hidden | Overflow::Clip | Overflow::Scroll | Overflow::Auto
    );

    // Begin clipping region if needed
    if needs_clip {
        items.push(DisplayItem::BeginClip {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        });
    }

    // Step 1: Paint background and borders
    paint_background(rect, style, items);
    paint_borders(rect, style, items);

    // Steps 2-5: Paint children in stacking order
    if let Some((_, children)) = ctx.node_map.get(&key) {
        // Sort children by CSS stacking order
        let sorted_children = stacking::sort_children_by_stacking_order(children, ctx.styles);

        // Paint each child in order
        for stacking_child in sorted_children {
            paint_node_recursive(stacking_child.key, ctx, items);
        }
    }

    // End clipping region
    if needs_clip {
        items.push(DisplayItem::EndClip);
    }
}

/// Paint the background of a node.
fn paint_background(rect: &LayoutRect, style: &ComputedStyle, items: &mut Vec<DisplayItem>) {
    let background = &style.background_color;
    if background.alpha > 0 {
        items.push(DisplayItem::Rect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
            color: [
                f32::from(background.red) / 255.0,
                f32::from(background.green) / 255.0,
                f32::from(background.blue) / 255.0,
                f32::from(background.alpha) / 255.0,
            ],
        });
    }
}

/// Paint the borders of a node.
fn paint_borders(rect: &LayoutRect, style: &ComputedStyle, items: &mut Vec<DisplayItem>) {
    // Only paint borders if border-style is 'solid'
    if !matches!(style.border_style, BorderStyle::Solid) {
        return;
    }

    let border = &style.border_width;
    let color = &style.border_color;

    // Convert border color to normalized RGBA
    let border_rgba = [
        f32::from(color.red) / 255.0,
        f32::from(color.green) / 255.0,
        f32::from(color.blue) / 255.0,
        f32::from(color.alpha) / 255.0,
    ];

    // Paint each border edge as a rectangle
    // Top border
    if border.top > 0.0 {
        items.push(DisplayItem::Rect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: border.top,
            color: border_rgba,
        });
    }

    // Right border
    if border.right > 0.0 {
        items.push(DisplayItem::Rect {
            x: rect.x + rect.width - border.right,
            y: rect.y,
            width: border.right,
            height: rect.height,
            color: border_rgba,
        });
    }

    // Bottom border
    if border.bottom > 0.0 {
        items.push(DisplayItem::Rect {
            x: rect.x,
            y: rect.y + rect.height - border.bottom,
            width: rect.width,
            height: border.bottom,
            color: border_rgba,
        });
    }

    // Left border
    if border.left > 0.0 {
        items.push(DisplayItem::Rect {
            x: rect.x,
            y: rect.y,
            width: border.left,
            height: rect.height,
            color: border_rgba,
        });
    }
}
