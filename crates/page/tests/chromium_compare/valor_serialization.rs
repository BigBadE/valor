use lightningcss::properties::PropertyId;
use rewrite_core::{Database, NodeId, Subpixel};
use rewrite_html::{DomTree, NodeData};
use rewrite_renderer::{ComputedBox, LayoutState};
use serde_json::{Value as JsonValue, json};
use std::sync::Mutex;

/// Serialize Valor's layout to JSON matching Chrome's extraction format.
pub fn serialize_valor_layout(
    tree: &DomTree,
    db: &Database,
    layout: &Mutex<LayoutState>,
) -> Result<JsonValue, String> {
    // Find <body> node
    let body_id = find_body(tree)?;

    let ctx = SerCtx {
        tree,
        db,
        layout,
        body_x: Subpixel::ZERO,
        body_y: Subpixel::ZERO,
    };

    // Get body offset to make coordinates relative
    let body_box = get_node_layout(body_id, &ctx);
    let body_x = body_box.x.unwrap_or(Subpixel::ZERO);
    let body_y = body_box.y.unwrap_or(Subpixel::ZERO);

    let ctx = SerCtx {
        tree,
        db,
        layout,
        body_x,
        body_y,
    };

    let body_json = serialize_element(body_id, &ctx);
    Ok(json!({
        "layout": body_json,
        "asserts": []
    }))
}

struct SerCtx<'ctx> {
    tree: &'ctx DomTree,
    db: &'ctx Database,
    layout: &'ctx Mutex<LayoutState>,
    body_x: Subpixel,
    body_y: Subpixel,
}

/// Read a node's cached layout values. These were resolved eagerly
/// during `on_property_change` / `on_node_created`.
fn get_node_layout(node_id: NodeId, ctx: &SerCtx<'_>) -> ComputedBox {
    ctx.layout.lock().expect("lock poisoned").get_node(node_id)
}

fn find_body(tree: &DomTree) -> Result<NodeId, String> {
    for idx in 0..tree.nodes.count() {
        if let NodeData::Element { tag, .. } = &tree.nodes[idx] {
            if tree.interner.resolve(tag) == "body" {
                return Ok(NodeId(idx as u32));
            }
        }
    }
    Err("No <body> element found".to_string())
}

fn serialize_element(node_id: NodeId, ctx: &SerCtx<'_>) -> JsonValue {
    let node_data = &ctx.tree.nodes[node_id.0 as usize];

    let NodeData::Element { tag, attributes } = node_data else {
        return JsonValue::Null;
    };

    let tag_str = ctx.tree.interner.resolve(tag).to_lowercase();

    // Skip style and script elements
    if tag_str == "style" || tag_str == "script" {
        return JsonValue::Null;
    }

    let computed = get_node_layout(node_id, ctx);

    let rect = json!({
        "x": (computed.x.unwrap_or(Subpixel::ZERO) - ctx.body_x).to_f64(),
        "y": (computed.y.unwrap_or(Subpixel::ZERO) - ctx.body_y).to_f64(),
        "width": computed.width.unwrap_or(Subpixel::ZERO).to_f64(),
        "height": computed.height.unwrap_or(Subpixel::ZERO).to_f64()
    });

    // Extract id and attrs
    let id = ctx
        .tree
        .interner
        .get("id")
        .and_then(|key| attributes.get(&key))
        .map_or_else(String::new, |val| val.to_string());

    let mut attrs_json = serde_json::Map::new();
    if let Some(type_key) = ctx.tree.interner.get("type") {
        if let Some(type_val) = attributes.get(&type_key) {
            attrs_json.insert("type".to_string(), json!(type_val.to_string()));
        }
    }
    if let Some(checked_key) = ctx.tree.interner.get("checked") {
        if attributes.contains_key(&checked_key) {
            attrs_json.insert("checked".to_string(), json!("true"));
        }
    }

    let style = serialize_style(node_id, ctx);

    // Check display:none
    if let Some(display_str) = style.get("display").and_then(|val| val.as_str()) {
        if display_str == "none" {
            return JsonValue::Null;
        }
    }

    let is_form_control = matches!(tag_str.as_str(), "input" | "textarea" | "select" | "button");

    let children = if is_form_control {
        Vec::new()
    } else {
        serialize_children(node_id, ctx)
    };

    json!({
        "type": "element",
        "tag": tag_str,
        "id": id,
        "attrs": attrs_json,
        "rect": rect,
        "style": style,
        "children": children
    })
}

fn serialize_children(node_id: NodeId, ctx: &SerCtx<'_>) -> Vec<JsonValue> {
    // Collect and reverse because DomTree children iterate in reverse DOM order
    let child_ids: Vec<NodeId> = ctx
        .tree
        .children(node_id)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    let mut children = Vec::new();
    for child_id in child_ids {
        let child_data = &ctx.tree.nodes[child_id.0 as usize];
        match child_data {
            NodeData::Element { tag, .. } => {
                let tag_str = ctx.tree.interner.resolve(tag).to_lowercase();
                if tag_str == "style" || tag_str == "script" {
                    continue;
                }
                let child_json = serialize_element(child_id, ctx);
                if !child_json.is_null() {
                    children.push(child_json);
                }
            }
            NodeData::Text(text) => {
                if text.trim().is_empty() {
                    continue;
                }
                let text_json = serialize_text(child_id, text, node_id, ctx);
                if !text_json.is_null() {
                    children.push(text_json);
                }
            }
            _ => {}
        }
    }
    children
}

/// Check if a text node is at the start/end of its block for Phase II trimming.
fn text_block_boundary(node_id: NodeId, tree: &DomTree) -> (bool, bool) {
    let has_prev = tree
        .prev_siblings(node_id)
        .any(|sib| match tree.get_node(sib) {
            Some(NodeData::Element { .. }) => true,
            Some(NodeData::Text(t)) => !t.trim().is_empty(),
            _ => false,
        });
    let has_next = tree
        .next_siblings(node_id)
        .any(|sib| match tree.get_node(sib) {
            Some(NodeData::Element { .. }) => true,
            Some(NodeData::Text(t)) => !t.trim().is_empty(),
            _ => false,
        });
    let sole_content = !has_prev && !has_next;
    (sole_content, sole_content)
}

fn serialize_text(node_id: NodeId, text: &str, parent_id: NodeId, ctx: &SerCtx<'_>) -> JsonValue {
    let computed = get_node_layout(node_id, ctx);

    let rect = json!({
        "x": (computed.x.unwrap_or(Subpixel::ZERO) - ctx.body_x).to_f64(),
        "y": (computed.y.unwrap_or(Subpixel::ZERO) - ctx.body_y).to_f64(),
        "width": computed.width.unwrap_or(Subpixel::ZERO).to_f64(),
        "height": computed.height.unwrap_or(Subpixel::ZERO).to_f64()
    });

    // CSS Text 3 §4.1.1: collapse whitespace for white-space: normal.
    let (at_start, at_end) = text_block_boundary(node_id, ctx.tree);
    let collapsed = rewrite_text::collapse_whitespace(text, at_start, at_end);

    // Text nodes use parent's style for font info
    let font_size = query_css_string(parent_id, &PropertyId::FontSize, ctx);
    let font_weight = query_css_string(parent_id, &PropertyId::FontWeight, ctx);
    let color = query_css_string(parent_id, &PropertyId::Color, ctx);
    let line_height = query_css_string(parent_id, &PropertyId::LineHeight, ctx);

    json!({
        "type": "text",
        "text": collapsed,
        "rect": rect,
        "style": {
            "fontSize": font_size,
            "fontWeight": font_weight,
            "color": color,
            "lineHeight": line_height
        }
    })
}

fn serialize_style(node_id: NodeId, ctx: &SerCtx<'_>) -> JsonValue {
    let display = query_css_string(node_id, &PropertyId::Display, ctx);
    let box_sizing = query_css_string(
        node_id,
        &PropertyId::BoxSizing(lightningcss::vendor_prefix::VendorPrefix::None),
        ctx,
    );
    let position = query_css_string(node_id, &PropertyId::Position, ctx);
    let overflow = query_css_string(node_id, &PropertyId::OverflowX, ctx);
    let font_size = query_css_string(node_id, &PropertyId::FontSize, ctx);
    let font_weight = query_css_string(node_id, &PropertyId::FontWeight, ctx);
    let font_family = query_css_string(node_id, &PropertyId::FontFamily, ctx);
    let color = query_css_string(node_id, &PropertyId::Color, ctx);
    let line_height = query_css_string(node_id, &PropertyId::LineHeight, ctx);
    let opacity = query_css_string(node_id, &PropertyId::Opacity, ctx);
    let z_index = query_css_string(node_id, &PropertyId::ZIndex, ctx);

    let flex_grow = query_css_number(
        node_id,
        &PropertyId::FlexGrow(lightningcss::vendor_prefix::VendorPrefix::None),
        ctx,
    );
    let flex_shrink = query_css_number(
        node_id,
        &PropertyId::FlexShrink(lightningcss::vendor_prefix::VendorPrefix::None),
        ctx,
    );
    let flex_basis = query_css_string(
        node_id,
        &PropertyId::FlexBasis(lightningcss::vendor_prefix::VendorPrefix::None),
        ctx,
    );
    let align_items = query_css_string(
        node_id,
        &PropertyId::AlignItems(lightningcss::vendor_prefix::VendorPrefix::None),
        ctx,
    );

    json!({
        "display": if display.is_empty() { "block".to_string() } else { display },
        "boxSizing": if box_sizing.is_empty() { "content-box".to_string() } else { box_sizing },
        "flexBasis": if flex_basis.is_empty() { "auto".to_string() } else { flex_basis },
        "flexGrow": flex_grow,
        "flexShrink": flex_shrink,
        "margin": serialize_edges(node_id, "margin", ctx),
        "padding": serialize_edges(node_id, "padding", ctx),
        "borderWidth": serialize_border_edges(node_id, ctx),
        "alignItems": if align_items.is_empty() { "normal".to_string() } else { align_items },
        "overflow": if overflow.is_empty() { "visible".to_string() } else { overflow },
        "position": if position.is_empty() { "static".to_string() } else { position },
        "fontSize": if font_size.is_empty() { "16px".to_string() } else { font_size },
        "fontWeight": if font_weight.is_empty() { "400".to_string() } else { font_weight },
        "fontFamily": font_family,
        "color": if color.is_empty() { "rgb(0, 0, 0)".to_string() } else { color },
        "lineHeight": if line_height.is_empty() { "normal".to_string() } else { line_height },
        "zIndex": if z_index.is_empty() { "auto".to_string() } else { z_index },
        "opacity": if opacity.is_empty() { "1".to_string() } else { opacity }
    })
}

fn serialize_edges(node_id: NodeId, prefix: &str, ctx: &SerCtx<'_>) -> JsonValue {
    let props: [PropertyId<'static>; 4] = match prefix {
        "margin" => [
            PropertyId::MarginTop,
            PropertyId::MarginRight,
            PropertyId::MarginBottom,
            PropertyId::MarginLeft,
        ],
        "padding" => [
            PropertyId::PaddingTop,
            PropertyId::PaddingRight,
            PropertyId::PaddingBottom,
            PropertyId::PaddingLeft,
        ],
        _ => return json!({"top": "0px", "right": "0px", "bottom": "0px", "left": "0px"}),
    };

    let [top, right, bottom, left] = props.map(|p| resolve_px(node_id, &p, ctx));

    json!({
        "top": top,
        "right": right,
        "bottom": bottom,
        "left": left
    })
}

fn serialize_border_edges(node_id: NodeId, ctx: &SerCtx<'_>) -> JsonValue {
    let top = resolve_px(node_id, &PropertyId::BorderTopWidth, ctx);
    let right = resolve_px(node_id, &PropertyId::BorderRightWidth, ctx);
    let bottom = resolve_px(node_id, &PropertyId::BorderBottomWidth, ctx);
    let left = resolve_px(node_id, &PropertyId::BorderLeftWidth, ctx);

    json!({
        "top": top,
        "right": right,
        "bottom": bottom,
        "left": left
    })
}

/// Read a cached box-model property value as a px string.
fn resolve_px(node_id: NodeId, prop_id: &PropertyId<'static>, ctx: &SerCtx<'_>) -> String {
    let val = ctx
        .layout
        .lock()
        .expect("lock poisoned")
        .get_property(node_id, prop_id)
        .unwrap_or(Subpixel::ZERO);
    format!("{}px", val.to_f64())
}

/// Query a CSS property and format it as a string.
fn query_css_string(node_id: NodeId, prop_id: &PropertyId<'static>, ctx: &SerCtx<'_>) -> String {
    let Some(prop) = ctx.db.get_property(node_id, prop_id.clone()) else {
        return String::new();
    };
    format_property(&prop)
}

/// Query a CSS property as a numeric value (for flexGrow/flexShrink).
fn query_css_number(node_id: NodeId, prop_id: &PropertyId<'static>, ctx: &SerCtx<'_>) -> f64 {
    let Some(prop) = ctx.db.get_property(node_id, prop_id.clone()) else {
        // Default: flexGrow=0, flexShrink=1
        return match prop_id {
            PropertyId::FlexShrink(_) => 1.0,
            _ => 0.0,
        };
    };

    match &prop {
        lightningcss::properties::Property::FlexGrow(val, _) => f64::from(*val),
        lightningcss::properties::Property::FlexShrink(val, _) => f64::from(*val),
        _ => 0.0,
    }
}

/// Format a lightningcss Property to a Chrome-compatible string.
fn format_property(prop: &lightningcss::properties::Property<'static>) -> String {
    use lightningcss::properties::Property;

    match prop {
        Property::Display(display) => format_display(display),

        Property::BoxSizing(bs, _) => match bs {
            lightningcss::properties::size::BoxSizing::BorderBox => "border-box".to_string(),
            lightningcss::properties::size::BoxSizing::ContentBox => "content-box".to_string(),
        },

        Property::Position(pos) => match pos {
            lightningcss::properties::position::Position::Static => "static".to_string(),
            lightningcss::properties::position::Position::Relative => "relative".to_string(),
            lightningcss::properties::position::Position::Absolute => "absolute".to_string(),
            lightningcss::properties::position::Position::Fixed => "fixed".to_string(),
            lightningcss::properties::position::Position::Sticky(_) => "sticky".to_string(),
        },

        Property::Overflow(overflow) => format_overflow_keyword(&overflow.x),
        Property::OverflowX(kw) | Property::OverflowY(kw) => format_overflow_keyword(kw),

        Property::FontSize(size) => format_length_or_percentage(size),
        Property::FontWeight(weight) => format_font_weight(weight),
        Property::FontFamily(families) => families
            .iter()
            .map(format_font_family)
            .collect::<Vec<_>>()
            .join(", "),

        Property::Color(color) => format_color(color),

        Property::LineHeight(lh) => match lh {
            lightningcss::properties::font::LineHeight::Normal => "normal".to_string(),
            lightningcss::properties::font::LineHeight::Number(num) => format!("{num}"),
            lightningcss::properties::font::LineHeight::Length(lp) => {
                format_length_or_percentage_lp(lp)
            }
        },

        Property::Opacity(opacity) => format!("{}", opacity.0),

        Property::ZIndex(zi) => match zi {
            lightningcss::properties::position::ZIndex::Auto => "auto".to_string(),
            lightningcss::properties::position::ZIndex::Integer(val) => format!("{val}"),
        },

        Property::MarginTop(val)
        | Property::MarginRight(val)
        | Property::MarginBottom(val)
        | Property::MarginLeft(val) => format_length_percentage_or_auto(val),

        Property::PaddingTop(val)
        | Property::PaddingRight(val)
        | Property::PaddingBottom(val)
        | Property::PaddingLeft(val) => format_length_percentage_or_auto(val),

        Property::BorderTopWidth(val)
        | Property::BorderRightWidth(val)
        | Property::BorderBottomWidth(val)
        | Property::BorderLeftWidth(val) => format_border_width(val),

        Property::FlexBasis(basis, _) => format_length_percentage_or_auto(basis),

        Property::AlignItems(align, _) => format_align_items(align),

        _ => format!("{prop:?}"),
    }
}

fn format_display(display: &lightningcss::properties::display::Display) -> String {
    use lightningcss::properties::display::*;

    match display {
        Display::Keyword(kw) => match kw {
            DisplayKeyword::None => "none".to_string(),
            _ => "block".to_string(),
        },
        Display::Pair(pair) => match (&pair.outside, &pair.inside) {
            (DisplayOutside::Block, DisplayInside::Flow) => "block".to_string(),
            (DisplayOutside::Inline, DisplayInside::Flow) => "inline".to_string(),
            (DisplayOutside::Block, DisplayInside::FlowRoot) => "flow-root".to_string(),
            (DisplayOutside::Inline, DisplayInside::FlowRoot) => "inline-block".to_string(),
            (_, DisplayInside::Flex(_)) => {
                if matches!(pair.outside, DisplayOutside::Inline) {
                    "inline-flex".to_string()
                } else {
                    "flex".to_string()
                }
            }
            (_, DisplayInside::Grid) => {
                if matches!(pair.outside, DisplayOutside::Inline) {
                    "inline-grid".to_string()
                } else {
                    "grid".to_string()
                }
            }
            (_, DisplayInside::Table) => "table".to_string(),
            _ => "block".to_string(),
        },
    }
}

fn format_overflow_keyword(kw: &lightningcss::properties::overflow::OverflowKeyword) -> String {
    use lightningcss::properties::overflow::OverflowKeyword;
    match kw {
        OverflowKeyword::Visible => "visible".to_string(),
        OverflowKeyword::Hidden => "hidden".to_string(),
        OverflowKeyword::Scroll => "scroll".to_string(),
        OverflowKeyword::Auto => "auto".to_string(),
        OverflowKeyword::Clip => "clip".to_string(),
    }
}

fn format_color(color: &lightningcss::values::color::CssColor) -> String {
    use lightningcss::values::color::CssColor;
    match color {
        CssColor::RGBA(rgba) => {
            format!("rgb({}, {}, {})", rgba.red, rgba.green, rgba.blue)
        }
        _ => format!("{color:?}"),
    }
}

fn format_length_or_percentage(size: &lightningcss::properties::font::FontSize) -> String {
    use lightningcss::properties::font::FontSize;
    match size {
        FontSize::Length(lp) => format_length_or_percentage_lp(lp),
        _ => format!("{size:?}"),
    }
}

fn format_length_or_percentage_lp(lp: &lightningcss::values::length::LengthPercentage) -> String {
    use lightningcss::values::length::LengthPercentage;
    match lp {
        LengthPercentage::Dimension(len) => format_length_value(len),
        LengthPercentage::Percentage(pct) => format!("{}%", pct.0 * 100.0),
        _ => "0px".to_string(),
    }
}

fn format_length_value(len: &lightningcss::values::length::LengthValue) -> String {
    use lightningcss::values::length::LengthValue;
    match len {
        LengthValue::Px(px) => format!("{px}px"),
        LengthValue::Em(em) => format!("{em}em"),
        LengthValue::Rem(rem) => format!("{rem}rem"),
        _ => format!("{len:?}"),
    }
}

fn format_length_percentage_or_auto(
    val: &lightningcss::values::length::LengthPercentageOrAuto,
) -> String {
    use lightningcss::values::length::LengthPercentageOrAuto;
    match val {
        LengthPercentageOrAuto::Auto => "auto".to_string(),
        LengthPercentageOrAuto::LengthPercentage(lp) => format_length_or_percentage_lp(lp),
    }
}

fn format_border_width(val: &lightningcss::properties::border::BorderSideWidth) -> String {
    use lightningcss::properties::border::BorderSideWidth;
    match val {
        BorderSideWidth::Length(len) => format_border_length(len),
        BorderSideWidth::Thin => "1px".to_string(),
        BorderSideWidth::Medium => "3px".to_string(),
        BorderSideWidth::Thick => "5px".to_string(),
    }
}

fn format_border_length(len: &lightningcss::values::length::Length) -> String {
    use lightningcss::values::length::Length;
    match len {
        Length::Value(lv) => format_length_value(lv),
        Length::Calc(_) => "0px".to_string(),
    }
}

fn format_font_weight(weight: &lightningcss::properties::font::FontWeight) -> String {
    use lightningcss::properties::font::{AbsoluteFontWeight, FontWeight};
    match weight {
        FontWeight::Absolute(abs) => match abs {
            AbsoluteFontWeight::Weight(n) => format!("{n}"),
            AbsoluteFontWeight::Normal => "400".to_string(),
            AbsoluteFontWeight::Bold => "700".to_string(),
        },
        FontWeight::Bolder => "bolder".to_string(),
        FontWeight::Lighter => "lighter".to_string(),
    }
}

fn format_font_family(ff: &lightningcss::properties::font::FontFamily<'_>) -> String {
    use lightningcss::properties::font::FontFamily;
    match ff {
        FontFamily::Generic(g) => format!("{g:?}").to_lowercase(),
        FontFamily::FamilyName(name) => format!("{name:?}"),
    }
}

fn format_align_items(align: &lightningcss::properties::align::AlignItems) -> String {
    use lightningcss::properties::align::*;
    match align {
        AlignItems::Normal => "normal".to_string(),
        AlignItems::Stretch => "stretch".to_string(),
        AlignItems::BaselinePosition(_) => "baseline".to_string(),
        AlignItems::SelfPosition { value, overflow: _ } => match value {
            SelfPosition::Center => "center".to_string(),
            SelfPosition::Start => "start".to_string(),
            SelfPosition::End => "end".to_string(),
            SelfPosition::SelfStart => "self-start".to_string(),
            SelfPosition::SelfEnd => "self-end".to_string(),
            SelfPosition::FlexStart => "flex-start".to_string(),
            SelfPosition::FlexEnd => "flex-end".to_string(),
        },
    }
}
