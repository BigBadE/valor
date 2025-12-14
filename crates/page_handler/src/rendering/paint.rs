//! Display list generation from layout and computed styles.

use crate::rendering::stacking;
use crate::utilities::snapshots::LayoutNodeKind;
use css::style_types::{BorderStyle, ComputedStyle, Overflow};
use css_core::LayoutRect;
use css_text::measurement::measure_text;
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
    /// Element attributes by node (for form controls, etc.).
    attrs: &'ctx HashMap<NodeKey, HashMap<String, String>>,
}

/// Generate a display list from layout geometry and computed styles.
///
/// This function traverses all layout rectangles and emits display items
/// in correct CSS paint order with proper clipping for overflow.
pub fn build_display_list(
    rects: &HashMap<NodeKey, LayoutRect>,
    styles: &HashMap<NodeKey, ComputedStyle>,
    snapshot: &[(NodeKey, LayoutNodeKind, NodeChildren)],
    attrs: &HashMap<NodeKey, HashMap<String, String>>,
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
        attrs,
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

    // Paint form control markers (checkboxes, radios) after borders
    if let Some((LayoutNodeKind::Block { tag }, _)) = ctx.node_map.get(&key) {
        paint_form_controls(
            &FormControlParams {
                key,
                tag,
                rect,
                style,
            },
            ctx,
            items,
        );
    }

    // Steps 2-5: Paint children in stacking order
    if let Some((_, children)) = ctx.node_map.get(&key) {
        paint_children_in_stacking_order(children, style, ctx, items);
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

/// Paint children in CSS stacking order, combining consecutive text nodes.
fn paint_children_in_stacking_order(
    children: &[NodeKey],
    style: &ComputedStyle,
    ctx: &PaintContext<'_>,
    items: &mut Vec<DisplayItem>,
) {
    // Sort children by CSS stacking order
    let sorted_children = stacking::sort_children_by_stacking_order(children, ctx.styles);

    // Paint each child in order, combining consecutive text nodes
    let mut skip_until_idx = 0;
    for (idx, stacking_child) in sorted_children.iter().enumerate() {
        if idx < skip_until_idx {
            continue; // Skip already-painted continuation text nodes
        }

        // Check if this child is an inline text node
        if let Some((LayoutNodeKind::InlineText { text }, _)) =
            ctx.node_map.get(&stacking_child.key)
        {
            // Combine consecutive text nodes
            let mut combined_text = text.clone();
            let mut last_text_idx = idx;

            // Look ahead for consecutive text siblings
            for (next_idx, next_child) in sorted_children[(idx + 1)..].iter().enumerate() {
                let Some((LayoutNodeKind::InlineText { text: next_text }, _)) =
                    ctx.node_map.get(&next_child.key)
                else {
                    break; // Stop at first non-text node
                };

                combined_text.push_str(next_text);
                last_text_idx = idx + 1 + next_idx;
            }

            skip_until_idx = last_text_idx + 1;

            // Paint combined text if it has a layout rect
            if let Some(text_rect) = ctx.rects.get(&stacking_child.key) {
                paint_text(&combined_text, text_rect, style, items);
            }
        } else {
            // Recursively paint non-text children
            paint_node_recursive(stacking_child.key, ctx, items);
        }
    }
}

/// Paint text content.
fn paint_text(text: &str, rect: &LayoutRect, style: &ComputedStyle, items: &mut Vec<DisplayItem>) {
    // Skip empty or whitespace-only text
    if text.trim().is_empty() {
        return;
    }

    // Extract text color from computed style
    let text_color = [
        f32::from(style.color.red) / 255.0,
        f32::from(style.color.green) / 255.0,
        f32::from(style.color.blue) / 255.0,
    ];

    // Get font size, weight, and family from computed style
    let font_size = style.font_size;
    let font_weight = style.font_weight;
    let font_family = style.font_family.clone();

    // Calculate line height using actual font metrics (MUST match css_text::measurement logic!)
    // For line-height: normal, measure_text gets font metrics directly (no shaping).
    // This ensures we use the EXACT same calculation as layout.
    let line_height = style
        .line_height
        .unwrap_or_else(|| measure_text("M", style).height);

    // Create text display item
    // Note: Glyphon positions text from the top-left corner, not the baseline

    items.push(DisplayItem::Text {
        x: rect.x,
        y: rect.y,
        text: text.to_string(),
        color: text_color,
        font_size,
        font_weight,
        font_family,
        line_height,
        // IMPORTANT: Round UP the bounds to avoid text wrapping due to fractional pixel truncation.
        // If text measures at 252.04px and we truncate to 252px, glyphon will wrap the text.
        // Using ceil() ensures we always have enough space.
        bounds: Some((
            rect.x.floor() as i32,
            rect.y.floor() as i32,
            (rect.x + rect.width).ceil() as i32,
            (rect.y + rect.height).ceil() as i32,
        )),
    });
}

struct FormControlParams<'params> {
    key: NodeKey,
    tag: &'params str,
    rect: &'params LayoutRect,
    style: &'params ComputedStyle,
}

/// Paint form control markers (checkbox check, radio dot).
fn paint_form_controls(
    params: &FormControlParams<'_>,
    ctx: &PaintContext<'_>,
    items: &mut Vec<DisplayItem>,
) {
    let FormControlParams {
        key,
        tag,
        rect,
        style,
    } = params;
    // Only handle input elements
    if *tag != "input" {
        return;
    }

    // Get the input type attribute
    let Some(attrs) = ctx.attrs.get(key) else {
        return;
    };
    let Some(input_type) = attrs.get("type") else {
        return;
    };

    // Check if the input is checked (for checkbox/radio)
    let is_checked = attrs.get("checked").is_some();

    match input_type.as_str() {
        "checkbox" => {
            if is_checked {
                paint_checkbox_mark(rect, style, items);
            }
        }
        "radio" => {
            if is_checked {
                paint_radio_dot(rect, style, items);
            }
        }
        _ => {}
    }
}

/// Paint a checkmark in a checkbox.
fn paint_checkbox_mark(rect: &LayoutRect, style: &ComputedStyle, items: &mut Vec<DisplayItem>) {
    // Use the border color or text color for the checkmark
    let color = if style.border_color.alpha > 0 {
        [
            f32::from(style.border_color.red) / 255.0,
            f32::from(style.border_color.green) / 255.0,
            f32::from(style.border_color.blue) / 255.0,
            f32::from(style.border_color.alpha) / 255.0,
        ]
    } else {
        [
            f32::from(style.color.red) / 255.0,
            f32::from(style.color.green) / 255.0,
            f32::from(style.color.blue) / 255.0,
            1.0,
        ]
    };

    // Draw a simple checkmark using two rectangles to form an "L" shape
    // The checkmark will be centered in the checkbox
    let center_x = rect.x + rect.width / 2.0;
    let center_y = rect.y + rect.height / 2.0;
    let size = rect.width.min(rect.height) * 0.6;

    // Short arm of the checkmark (bottom-left to center)
    let short_width = size * 0.4;
    let short_height = size * 0.15;
    items.push(DisplayItem::Rect {
        x: center_x - size * 0.3,
        y: center_y,
        width: short_width,
        height: short_height,
        color,
    });

    // Long arm of the checkmark (center to top-right)
    let long_width = size * 0.15;
    let long_height = size * 0.7;
    items.push(DisplayItem::Rect {
        x: center_x + size * 0.1,
        y: center_y - size * 0.5,
        width: long_width,
        height: long_height,
        color,
    });
}

/// Paint a dot in a radio button.
fn paint_radio_dot(rect: &LayoutRect, style: &ComputedStyle, items: &mut Vec<DisplayItem>) {
    // Use the border color or text color for the dot
    let color = if style.border_color.alpha > 0 {
        [
            f32::from(style.border_color.red) / 255.0,
            f32::from(style.border_color.green) / 255.0,
            f32::from(style.border_color.blue) / 255.0,
            f32::from(style.border_color.alpha) / 255.0,
        ]
    } else {
        [
            f32::from(style.color.red) / 255.0,
            f32::from(style.color.green) / 255.0,
            f32::from(style.color.blue) / 255.0,
            1.0,
        ]
    };

    // Draw a filled circle as a square (approximation)
    // The dot will be centered in the radio button
    let center_x = rect.x + rect.width / 2.0;
    let center_y = rect.y + rect.height / 2.0;
    let size = rect.width.min(rect.height) * 0.5;

    items.push(DisplayItem::Rect {
        x: center_x - size / 2.0,
        y: center_y - size / 2.0,
        width: size,
        height: size,
        color,
    });
}
