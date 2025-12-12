//! Layout serialization logic for Valor's internal layout representation.

use anyhow::{Result, anyhow};
use css::style_types::{
    AlignItems, BorderWidths, BoxSizing, ComputedStyle, Display, Edges, Overflow, Position,
};
use css_core::LayoutRect;
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
}

/// Serialize a layout box's rect as JSON.
fn serialize_rect(rect: &LayoutRect) -> JsonValue {
    json!({
        "x": f64::from(rect.x),
        "y": f64::from(rect.y),
        "width": f64::from(rect.width),
        "height": f64::from(rect.height)
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
fn serialize_style(computed: &ComputedStyle) -> JsonValue {
    let display_str = match computed.display {
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

    // For flex containers, align-items defaults to stretch
    // For non-flex containers, Chrome returns normal, but we model it as stretch
    // Since Stretch is our default, we output stretch for all cases
    let align_items_str = match computed.align_items {
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

    let flex_basis_str = computed
        .flex_basis
        .map_or_else(|| "auto".to_string(), |val| format!("{val}px"));

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

/// Helper to serialize a block child if it's a Block layout node.
///
/// # Errors
///
/// Returns an error if recursive serialization fails.
fn serialize_block_child(
    child_key: NodeKey,
    ctx: &SerializationContext<'_>,
    child_json: &mut Vec<JsonValue>,
) -> Result<()> {
    // Check if this child is a Block node before serializing
    if let Some((_, child_kind, _)) = ctx.snapshot.iter().find(|(key, _, _)| *key == child_key)
        && matches!(child_kind, LayoutNodeKind::Block { .. })
    {
        serialize_element_recursive(child_key, ctx, child_json)?;
    }
    Ok(())
}

/// Serialize a text node to JSON.
fn serialize_text_node(text: &str, rect: &LayoutRect, computed: &ComputedStyle) -> JsonValue {
    json!({
        "type": "text",
        "text": text,
        "rect": serialize_rect(rect),
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
    // Only count Block layout nodes as children, not InlineText
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
        "rect": serialize_rect(rect),
        "style": serialize_style(computed),
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
        return Ok(());
    };

    // Get layout rect
    let Some(rect) = ctx.rects.get(&key) else {
        return Ok(());
    };

    // Get computed style
    let Some(computed) = ctx.styles.get(&key) else {
        return Ok(());
    };

    // Skip nodes with display:none
    if computed.display == Display::None {
        return Ok(());
    }

    match kind {
        LayoutNodeKind::InlineText { text } => {
            parent_children.push(serialize_text_node(text, rect, computed));
        }
        LayoutNodeKind::Block { tag } => {
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
    // Get layout snapshot
    let snapshot = page.layouter_snapshot();

    // Ensure layout is computed and get rects
    page.ensure_layout_now();
    let rects = page.layouter_geometry_mut();

    // Get computed styles
    let styles = page.computed_styles_snapshot()?;

    // Get attributes
    let attrs = page.layouter_attrs_map();

    // Create serialization context
    let ctx = SerializationContext {
        snapshot: &snapshot,
        rects: &rects,
        styles: &styles,
        attrs: &attrs,
    };

    // Find body element in the snapshot
    let body_key = snapshot
        .iter()
        .find(|(_, kind, _)| matches!(kind, LayoutNodeKind::Block { tag } if tag == "body"))
        .map(|(key, _, _)| *key)
        .ok_or_else(|| anyhow!("No body element found"))?;

    let mut root_children = Vec::new();
    serialize_element_recursive(body_key, &ctx, &mut root_children)?;

    let layout = root_children.into_iter().next().unwrap_or(Value::Null);

    Ok(json!({
        "layout": layout,
        "asserts": []
    }))
}
