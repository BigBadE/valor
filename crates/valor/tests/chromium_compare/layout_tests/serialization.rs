//! Layout serialization logic for Valor's internal layout representation.

use anyhow::{Result, anyhow};
use css::style_types::{
    AlignItems, BorderWidths, BoxSizing, ComputedStyle, Display, Edges, Overflow, Position,
};
use css_core::LayoutRect;
use css_display::used_display_for_child;
use css_text::measurement::measure_text;
use js::NodeKey;
use page_handler::HtmlPage;
use page_handler::utilities::snapshots::LayoutNodeKind;
use serde_json::{Map as JsonMap, Value as JsonValue, Value, json};
use std::collections::HashMap;

/// Type alias for layout snapshot data structure.
type LayoutSnapshot = [(NodeKey, LayoutNodeKind, Vec<NodeKey>)];

/// Context for serialization operations, bundling commonly passed parameters.
struct SerializationContext<'context> {
    snapshot: &'context LayoutSnapshot,
    rects: &'context HashMap<NodeKey, LayoutRect>,
    styles: &'context HashMap<NodeKey, ComputedStyle>,
    attrs: &'context HashMap<NodeKey, HashMap<String, String>>,
    /// Offset to subtract from all coordinates (body element's position)
    body_offset_x: f32,
    body_offset_y: f32,
}

/// Find the parent key for a given node key in the snapshot.
fn find_parent_key(snapshot: &LayoutSnapshot, key: NodeKey) -> Option<NodeKey> {
    snapshot
        .iter()
        .find(|(_, _, children)| children.contains(&key))
        .map(|(parent_key, _, _)| *parent_key)
}

/// Serialize a layout box's rect as JSON.
/// Chrome rounds dimensions to whole pixels for text boxes, so we do the same.
/// Coordinates are adjusted relative to the body element's position.
fn serialize_rect(rect: &LayoutRect, body_offset_x: f32, body_offset_y: f32) -> JsonValue {
    json!({
        "x": f64::from(rect.x - body_offset_x),
        "y": f64::from(rect.y - body_offset_y),
        "width": f64::from(rect.width),
        "height": f64::from(rect.height.round())
    })
}

/// Serialize style edges (margin/padding/border) as JSON.
fn serialize_edges(edges: &Edges) -> JsonValue {
    json!({
        "top": format!("{top}px", top = edges.top),
        "right": format!("{right}px", right = edges.right),
        "bottom": format!("{bottom}px", bottom = edges.bottom),
        "left": format!("{left}px", left = edges.left)
    })
}

/// Serialize border widths as JSON.
fn serialize_border_widths(widths: &BorderWidths) -> JsonValue {
    json!({
        "top": format!("{top}px", top = widths.top),
        "right": format!("{right}px", right = widths.right),
        "bottom": format!("{bottom}px", bottom = widths.bottom),
        "left": format!("{left}px", left = widths.left)
    })
}

/// Serialize computed style to JSON for comparison with Chromium.
///
/// `parent_style` is used to compute the used display value (blockification for flex/grid items).
/// `is_root` should be true for the root element (body).
fn serialize_style(
    computed: &ComputedStyle,
    parent_style: Option<&ComputedStyle>,
    is_root: bool,
) -> JsonValue {
    // Compute the used display value with blockification rules applied
    let used_display = used_display_for_child(computed, parent_style, is_root);

    let display_str = match used_display {
        Display::Block => "block",
        Display::Flex => "flex",
        Display::InlineFlex => "inline-flex",
        Display::Grid => "grid",
        Display::InlineGrid => "inline-grid",
        Display::None => "none",
        Display::Contents => "contents",
        Display::InlineBlock => "inline-block",
        Display::Inline => "inline",
    };

    let box_sizing_str = match computed.box_sizing {
        BoxSizing::BorderBox => "border-box",
        BoxSizing::ContentBox => "content-box",
    };

    // For flex containers, align-items defaults to normal (which behaves like stretch)
    // Chrome serializes "normal" for the initial value, "stretch" when explicitly set
    let align_items_str = match computed.align_items {
        AlignItems::Normal => "normal",
        AlignItems::Stretch => "stretch",
        AlignItems::Center => "center",
        AlignItems::FlexStart => "flex-start",
        AlignItems::FlexEnd => "flex-end",
    };

    let overflow_str = match computed.overflow {
        Overflow::Visible => "visible",
        Overflow::Hidden => "hidden",
        Overflow::Auto => "auto",
        Overflow::Scroll => "scroll",
        Overflow::Clip => "clip",
    };

    let position_str = match computed.position {
        Position::Static => "static",
        Position::Relative => "relative",
        Position::Absolute => "absolute",
        Position::Fixed => "fixed",
    };

    let flex_basis_str = if let Some(percent) = computed.flex_basis_percent {
        // Percentage flex-basis (from flex shorthand or explicit percentage)
        format!("{}%", (percent * 100.0).round() as i32)
    } else if let Some(val) = computed.flex_basis {
        // Pixel flex-basis
        format!("{val}px")
    } else {
        // Auto (default)
        "auto".to_string()
    };

    json!({
        "display": display_str,
        "boxSizing": box_sizing_str,
        "flexBasis": flex_basis_str,
        "flexGrow": computed.flex_grow,
        "flexShrink": computed.flex_shrink,
        "margin": serialize_edges(&computed.margin),
        "padding": serialize_edges(&computed.padding),
        "borderWidth": serialize_border_widths(&computed.border_width),
        "alignItems": align_items_str,
        "overflow": overflow_str,
        "position": position_str,
        "fontSize": format!("{font_size}px", font_size = computed.font_size),
        "fontWeight": computed.font_weight.to_string(),
        "fontFamily": computed.font_family.clone().unwrap_or_default(),
        "color": format!(
            "rgb({red}, {green}, {blue})",
            red = computed.color.red,
            green = computed.color.green,
            blue = computed.color.blue
        ),
        "lineHeight": computed
            .line_height
            .map_or_else(|| "normal".to_string(), |val| format!("{val}px")),
        "zIndex": computed
            .z_index
            .map_or_else(|| "auto".to_string(), |val| val.to_string()),
        "opacity": computed.opacity.unwrap_or(1.0).to_string()
    })
}

/// Helper to serialize a child node (both Block and `InlineText` nodes).
///
/// # Errors
///
/// Returns an error if recursive serialization fails.
fn serialize_block_child(
    child_key: NodeKey,
    ctx: &SerializationContext<'_>,
    child_json: &mut Vec<JsonValue>,
) -> Result<()> {
    // Serialize all children (Block and InlineText nodes)
    // serialize_element_recursive handles both types correctly
    serialize_element_recursive(child_key, ctx, child_json)?;
    Ok(())
}

/// Serialize a text node to JSON.
/// Uses the layout rect height which already accounts for text wrapping.
/// For wrapped text, the height includes all lines.
fn serialize_text_node(
    text: &str,
    rect: &LayoutRect,
    computed: &ComputedStyle,
    _parent_width: f32,
    body_offset_x: f32,
    body_offset_y: f32,
) -> JsonValue {
    // The rect passed in from layout already has the correct height accounting for:
    // - Single line: glyph_height (shaped bounds)
    // - Wrapped text: total_height (line_height × line_count)
    //
    // Chrome's getBoundingClientRect() for text nodes returns the ink bounds,
    // which for wrapped text is the total height of all lines. Our layout engine
    // computes this correctly in text.rs, so we just use rect.height directly.
    //
    // Floor the height to match Chrome's pixel rounding behavior.
    let text_height = rect.height.floor();

    // Chrome's Range.getBoundingClientRect() returns the CONTENT AREA height
    // (font ascent + descent), NOT the line-height or ink bounds.
    // This is the em-box height from the font metrics.
    let metrics = measure_text(text, computed);
    let single_line_height = metrics.height; // CSS line-height
    let is_single_line = rect.height <= single_line_height * 1.5;

    let final_height = if is_single_line {
        // Single line: use font content area (ascent + descent)
        let content_height = metrics.shaped_ascent + metrics.shaped_descent;
        content_height.floor()
    } else {
        // Wrapped text: use layout-computed height (line_height × line_count)
        text_height
    };

    // Create rect with appropriate height, adjusted relative to body element
    let text_rect = json!({
        "x": f64::from(rect.x - body_offset_x),
        "y": f64::from(rect.y - body_offset_y),
        "width": f64::from(rect.width),
        "height": f64::from(final_height)
    });

    json!({
        "type": "text",
        "text": text,
        "rect": text_rect,
        "style": {
            "fontSize": format!("{font_size}px", font_size = computed.font_size),
            "fontWeight": computed.font_weight.to_string(),
            "color": format!(
                "rgb({red}, {green}, {blue})",
                red = computed.color.red,
                green = computed.color.green,
                blue = computed.color.blue
            ),
            "lineHeight": computed
                .line_height
                .map_or_else(|| "normal".to_string(), |val| format!("{val}px"))
        }
    })
}

/// Extract element attributes for serialization.
fn extract_element_attrs(
    key: NodeKey,
    attrs: &HashMap<NodeKey, HashMap<String, String>>,
) -> JsonMap<String, JsonValue> {
    let mut attrs_map = JsonMap::new();
    if let Some(node_attrs) = attrs.get(&key) {
        if let Some(type_val) = node_attrs.get("type") {
            attrs_map.insert("type".to_string(), JsonValue::String(type_val.clone()));
        }
        if node_attrs.get("checked").is_some() {
            attrs_map.insert("checked".to_string(), JsonValue::String("true".to_string()));
        }
    }
    attrs_map
}

/// Serialize a block element's children.
///
/// # Errors
///
/// Returns an error if child serialization fails.
fn serialize_block_children(
    children: &[NodeKey],
    is_form_control: bool,
    ctx: &SerializationContext<'_>,
) -> Result<Vec<JsonValue>> {
    let mut child_json = Vec::new();

    // Serialize children recursively (unless it's a form control)
    // Include both Block and InlineText nodes to match Chrome output
    if !is_form_control {
        for child_key in children {
            serialize_block_child(*child_key, ctx, &mut child_json)?;
        }
    }

    Ok(child_json)
}

/// Serialize a block element to JSON.
///
/// # Errors
///
/// Returns an error if child serialization fails.
fn serialize_block_element(
    key: NodeKey,
    tag: &str,
    children: &[NodeKey],
    ctx: &SerializationContext<'_>,
    parent_children: &mut Vec<JsonValue>,
) -> Result<()> {
    // Look up rect and computed style from context
    let Some(rect) = ctx.rects.get(&key) else {
        return Ok(());
    };
    let Some(computed) = ctx.styles.get(&key) else {
        return Ok(());
    };

    // Find parent style for used display computation
    let parent_key = find_parent_key(ctx.snapshot, key);
    let parent_style = parent_key.and_then(|parent| ctx.styles.get(&parent));

    // Body element is the root for display blockification purposes
    let is_root = tag == "body";

    let attrs_map = extract_element_attrs(key, ctx.attrs);
    let is_form_control = matches!(tag, "input" | "textarea" | "select" | "button");
    let child_json = serialize_block_children(children, is_form_control, ctx)?;

    let id = ctx
        .attrs
        .get(&key)
        .and_then(|attr| attr.get("id"))
        .cloned()
        .unwrap_or_default();

    parent_children.push(json!({
        "type": "element",
        "tag": tag,
        "id": id,
        "attrs": attrs_map,
        "rect": serialize_rect(rect, ctx.body_offset_x, ctx.body_offset_y),
        "style": serialize_style(computed, parent_style, is_root),
        "children": child_json
    }));

    Ok(())
}

/// Serialize an element's layout and children recursively.
///
/// # Errors
///
/// Returns an error if serialization of child elements fails.
fn serialize_element_recursive(
    key: NodeKey,
    ctx: &SerializationContext<'_>,
    parent_children: &mut Vec<JsonValue>,
) -> Result<()> {
    // Find node in snapshot
    let node_info = ctx
        .snapshot
        .iter()
        .find(|(node_key, _, _)| *node_key == key);
    let Some((_, kind, children)) = node_info else {
        log::warn!(
            "serialize_element_recursive: node {:?} not found in snapshot",
            key
        );
        return Ok(());
    };

    match kind {
        LayoutNodeKind::InlineText { text } => {
            // Text nodes need rect and their parent's computed style for font info
            let Some(rect) = ctx.rects.get(&key) else {
                return Ok(());
            };

            // Skip whitespace-only text nodes
            // (Chrome filters these out from layout output in block formatting contexts)
            if text.trim().is_empty() {
                return Ok(());
            }

            // Text nodes don't have their own computed styles - they inherit from parent
            // Find parent and use its computed style for font/color information
            let parent_key = ctx
                .snapshot
                .iter()
                .find(|(_, _, child_keys)| child_keys.contains(&key))
                .map(|(parent_key, _, _)| *parent_key);

            let computed = parent_key
                .and_then(|parent| ctx.styles.get(&parent))
                .or_else(|| {
                    // Fallback: try to get style from the text node itself
                    // (in case the engine set it, though it normally doesn't)
                    ctx.styles.get(&key)
                });

            let Some(computed) = computed else {
                return Ok(());
            };

            // Get parent width for text wrapping calculation
            let parent_width = parent_key
                .and_then(|parent| ctx.rects.get(&parent))
                .map_or(800.0, |parent_rect| parent_rect.width); // Fallback to viewport width

            parent_children.push(serialize_text_node(
                text,
                rect,
                computed,
                parent_width,
                ctx.body_offset_x,
                ctx.body_offset_y,
            ));
        }
        LayoutNodeKind::Block { tag } => {
            // Block elements need rect and computed style
            let Some(_rect) = ctx.rects.get(&key) else {
                log::warn!(
                    "serialize_element_recursive: no rect for block {:?} tag={}",
                    key,
                    tag
                );
                return Ok(());
            };
            let Some(computed) = ctx.styles.get(&key) else {
                log::warn!(
                    "serialize_element_recursive: no computed style for block {:?} tag={}",
                    key,
                    tag
                );
                return Ok(());
            };

            // Skip nodes with display:none
            if computed.display == Display::None {
                return Ok(());
            }

            serialize_block_element(key, tag, children, ctx, parent_children)?;
        }
        LayoutNodeKind::Document => {
            // Recursively serialize children for document node
            for child_key in children {
                serialize_element_recursive(*child_key, ctx, parent_children)?;
            }
        }
    }

    Ok(())
}

/// Serialize Valor's layout representation to JSON for comparison with Chromium.
///
/// # Errors
///
/// Returns an error if layout serialization fails.
pub fn serialize_valor_layout(page: &mut HtmlPage) -> Result<JsonValue> {
    // Ensure layout is computed first
    page.ensure_layout_now();

    // Get layout snapshot AFTER layout is computed
    let snapshot = page.layouter_snapshot();
    log::info!(
        "serialize_valor_layout: snapshot has {} nodes",
        snapshot.len()
    );
    let rects = page.layouter_geometry_mut();

    // Get computed styles
    let styles = page.computed_styles_snapshot()?;

    // Get attributes
    let attrs = page.layouter_attrs_map();

    // Find body element in the snapshot first to get its offset
    let body_key = snapshot
        .iter()
        .find(|(_, kind, _)| matches!(kind, LayoutNodeKind::Block { tag } if tag == "body"))
        .map(|(key, _, _)| *key)
        .ok_or_else(|| {
            log::error!("No body element found in snapshot. Available elements:");
            for (key, kind, _) in &snapshot {
                match kind {
                    LayoutNodeKind::Block { tag } => log::error!("  - {:?}: {}", key, tag),
                    LayoutNodeKind::InlineText { text } => {
                        log::error!("  - {:?}: text({:?})", key, text)
                    }
                    LayoutNodeKind::Document => log::error!("  - {:?}: Document", key),
                }
            }
            anyhow!("No body element found")
        })?;

    // Get body's position to use as offset (make body coordinates relative to itself at 0,0)
    let body_rect = rects
        .get(&body_key)
        .ok_or_else(|| anyhow!("No rect for body element"))?;
    let body_offset_x = body_rect.x;
    let body_offset_y = body_rect.y;

    log::info!(
        "Body element at ({}, {}) - will adjust all coordinates relative to this",
        body_offset_x,
        body_offset_y
    );

    // Create serialization context with body offset
    let ctx = SerializationContext {
        snapshot: &snapshot,
        rects: &rects,
        styles: &styles,
        attrs: &attrs,
        body_offset_x,
        body_offset_y,
    };

    log::info!("Found body element: {:?}", body_key);
    let mut root_children = Vec::new();
    serialize_element_recursive(body_key, &ctx, &mut root_children)?;
    log::info!("Serialized {} root children", root_children.len());

    let layout = root_children.into_iter().next().unwrap_or(Value::Null);
    log::info!(
        "Final layout is: {}",
        if layout == Value::Null {
            "null"
        } else {
            "non-null"
        }
    );

    Ok(json!({
        "layout": layout,
        "asserts": []
    }))
}
