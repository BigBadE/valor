//! CSS value parsing.

use crate::value::{CssKeyword, CssValue, LengthValue};

/// Parse a CSS length value (px, em, rem, etc.).
pub fn parse_length(value: &str) -> Option<LengthValue> {
    let value = value.trim();

    if let Some(px_str) = value.strip_suffix("px") {
        return px_str.trim().parse::<f32>().ok().map(LengthValue::Px);
    }
    if let Some(em_str) = value.strip_suffix("em") {
        return em_str.trim().parse::<f32>().ok().map(LengthValue::Em);
    }
    if let Some(rem_str) = value.strip_suffix("rem") {
        return rem_str.trim().parse::<f32>().ok().map(LengthValue::Rem);
    }
    if let Some(vw_str) = value.strip_suffix("vw") {
        return vw_str.trim().parse::<f32>().ok().map(LengthValue::Vw);
    }
    if let Some(vh_str) = value.strip_suffix("vh") {
        return vh_str.trim().parse::<f32>().ok().map(LengthValue::Vh);
    }
    if let Some(pct_str) = value.strip_suffix('%') {
        return pct_str
            .trim()
            .parse::<f32>()
            .ok()
            .map(|v| LengthValue::Percent(v / 100.0));
    }

    // Unitless number (treat as px)
    if let Ok(num) = value.parse::<f32>() {
        return Some(LengthValue::Px(num));
    }

    None
}

/// Parse a CSS keyword.
pub fn parse_keyword(value: &str) -> Option<CssKeyword> {
    let value = value.trim().to_lowercase();
    match value.as_str() {
        // Global
        "initial" => Some(CssKeyword::Initial),
        "inherit" => Some(CssKeyword::Inherit),
        "unset" => Some(CssKeyword::Unset),
        "revert" => Some(CssKeyword::Revert),

        // Auto/None
        "auto" => Some(CssKeyword::Auto),
        "none" => Some(CssKeyword::None),
        "normal" => Some(CssKeyword::Normal),

        // Display
        "block" => Some(CssKeyword::Block),
        "inline" => Some(CssKeyword::Inline),
        "inline-block" => Some(CssKeyword::InlineBlock),
        "flex" => Some(CssKeyword::Flex),
        "inline-flex" => Some(CssKeyword::InlineFlex),
        "grid" => Some(CssKeyword::Grid),
        "inline-grid" => Some(CssKeyword::InlineGrid),
        "table" => Some(CssKeyword::Table),
        "table-row" => Some(CssKeyword::TableRow),
        "table-cell" => Some(CssKeyword::TableCell),
        "list-item" => Some(CssKeyword::ListItem),
        "contents" => Some(CssKeyword::Contents),
        "flow-root" => Some(CssKeyword::FlowRoot),

        // Position
        "static" => Some(CssKeyword::Static),
        "relative" => Some(CssKeyword::Relative),
        "absolute" => Some(CssKeyword::Absolute),
        "fixed" => Some(CssKeyword::Fixed),
        "sticky" => Some(CssKeyword::Sticky),

        // Float
        "left" => Some(CssKeyword::Left),
        "right" => Some(CssKeyword::Right),

        // Clear
        "both" => Some(CssKeyword::Both),

        // Overflow
        "visible" => Some(CssKeyword::Visible),
        "hidden" => Some(CssKeyword::Hidden),
        "scroll" => Some(CssKeyword::Scroll),
        "clip" => Some(CssKeyword::Clip),

        // Box Sizing
        "content-box" => Some(CssKeyword::ContentBox),
        "border-box" => Some(CssKeyword::BorderBox),

        // Flex Direction
        "row" => Some(CssKeyword::Row),
        "row-reverse" => Some(CssKeyword::RowReverse),
        "column" => Some(CssKeyword::Column),
        "column-reverse" => Some(CssKeyword::ColumnReverse),

        // Flex Wrap
        "nowrap" => Some(CssKeyword::Nowrap),
        "wrap" => Some(CssKeyword::Wrap),
        "wrap-reverse" => Some(CssKeyword::WrapReverse),

        // Justify Content / Align Items
        "flex-start" => Some(CssKeyword::FlexStart),
        "flex-end" => Some(CssKeyword::FlexEnd),
        "center" => Some(CssKeyword::Center),
        "space-between" => Some(CssKeyword::SpaceBetween),
        "space-around" => Some(CssKeyword::SpaceAround),
        "space-evenly" => Some(CssKeyword::SpaceEvenly),
        "stretch" => Some(CssKeyword::Stretch),
        "baseline" => Some(CssKeyword::Baseline),

        // Text Align
        "start" => Some(CssKeyword::Start),
        "end" => Some(CssKeyword::End),
        "justify" => Some(CssKeyword::Justify),

        // Font Weight
        "thin" => Some(CssKeyword::Thin),
        "extra-light" => Some(CssKeyword::ExtraLight),
        "light" => Some(CssKeyword::Light),
        "regular" => Some(CssKeyword::Regular),
        "medium" => Some(CssKeyword::Medium),
        "semi-bold" => Some(CssKeyword::SemiBold),
        "bold" => Some(CssKeyword::Bold),
        "extra-bold" => Some(CssKeyword::ExtraBold),
        "black" => Some(CssKeyword::Black),

        // Font Style
        "italic" => Some(CssKeyword::Italic),
        "oblique" => Some(CssKeyword::Oblique),

        // Text Transform
        "uppercase" => Some(CssKeyword::Uppercase),
        "lowercase" => Some(CssKeyword::Lowercase),
        "capitalize" => Some(CssKeyword::Capitalize),

        // White Space
        "pre" => Some(CssKeyword::Pre),
        "pre-wrap" => Some(CssKeyword::PreWrap),
        "pre-line" => Some(CssKeyword::PreLine),

        // Word Break
        "break-all" => Some(CssKeyword::BreakAll),
        "keep-all" => Some(CssKeyword::KeepAll),
        "break-word" => Some(CssKeyword::BreakWord),

        // Vertical Align
        "top" => Some(CssKeyword::Top),
        "middle" => Some(CssKeyword::Middle),
        "bottom" => Some(CssKeyword::Bottom),
        "text-top" => Some(CssKeyword::TextTop),
        "text-bottom" => Some(CssKeyword::TextBottom),
        "sub" => Some(CssKeyword::Sub),
        "super" => Some(CssKeyword::Super),

        // Cursor
        "pointer" => Some(CssKeyword::Pointer),
        "default" => Some(CssKeyword::Default),
        "text" => Some(CssKeyword::Text),
        "move" => Some(CssKeyword::Move),
        "not-allowed" => Some(CssKeyword::NotAllowed),
        "grab" => Some(CssKeyword::Grab),
        "grabbing" => Some(CssKeyword::Grabbing),

        // Visibility
        "collapse" => Some(CssKeyword::Collapse),

        // Border Style
        "solid" => Some(CssKeyword::Solid),
        "dashed" => Some(CssKeyword::Dashed),
        "dotted" => Some(CssKeyword::Dotted),
        "double" => Some(CssKeyword::Double),
        "groove" => Some(CssKeyword::Groove),
        "ridge" => Some(CssKeyword::Ridge),
        "inset" => Some(CssKeyword::Inset),
        "outset" => Some(CssKeyword::Outset),

        // Background
        "cover" => Some(CssKeyword::Cover),
        "contain" => Some(CssKeyword::Contain),
        "repeat" => Some(CssKeyword::Repeat),
        "repeat-x" => Some(CssKeyword::RepeatX),
        "repeat-y" => Some(CssKeyword::RepeatY),
        "no-repeat" => Some(CssKeyword::NoRepeat),

        // Object Fit
        "fill" => Some(CssKeyword::Fill),
        "scale-down" => Some(CssKeyword::ScaleDown),

        // Mix Blend Mode
        "multiply" => Some(CssKeyword::Multiply),
        "screen" => Some(CssKeyword::Screen),
        "overlay" => Some(CssKeyword::Overlay),
        "darken" => Some(CssKeyword::Darken),
        "lighten" => Some(CssKeyword::Lighten),
        "color-dodge" => Some(CssKeyword::ColorDodge),
        "color-burn" => Some(CssKeyword::ColorBurn),
        "hard-light" => Some(CssKeyword::HardLight),
        "soft-light" => Some(CssKeyword::SoftLight),
        "difference" => Some(CssKeyword::Difference),
        "exclusion" => Some(CssKeyword::Exclusion),
        "hue" => Some(CssKeyword::Hue),
        "saturation" => Some(CssKeyword::Saturation),
        "color" => Some(CssKeyword::Color),
        "luminosity" => Some(CssKeyword::Luminosity),

        // Writing Mode
        "horizontal-tb" => Some(CssKeyword::HorizontalTb),
        "vertical-rl" => Some(CssKeyword::VerticalRl),
        "vertical-lr" => Some(CssKeyword::VerticalLr),

        // Other
        "transparent" => Some(CssKeyword::Transparent),
        "currentcolor" => Some(CssKeyword::CurrentColor),
        "min" => Some(CssKeyword::Min),
        "max" => Some(CssKeyword::Max),
        "fit-content" => Some(CssKeyword::FitContent),
        "min-content" => Some(CssKeyword::MinContent),
        "max-content" => Some(CssKeyword::MaxContent),

        _ => None,
    }
}

/// Parse a CSS number (unitless).
pub fn parse_number(value: &str) -> Option<f32> {
    value.trim().parse::<f32>().ok()
}

/// Parse a generic CSS property value.
pub fn parse_value(value: &str) -> Option<CssValue> {
    let value = value.trim();

    if value.is_empty() {
        return None;
    }

    if let Some(keyword) = parse_keyword(value) {
        return Some(CssValue::Keyword(keyword));
    }

    if let Some(length) = parse_length(value) {
        return Some(CssValue::Length(length));
    }

    if let Some(num) = parse_number(value) {
        return Some(CssValue::Number(num));
    }

    // Fallback: store as custom value for later interpretation
    Some(CssValue::Custom(value.to_string()))
}

/// Parse edge values (padding, margin, border-width).
pub fn parse_edges(value: &str) -> Option<[CssValue; 4]> {
    let values: Vec<CssValue> = value.split_whitespace().filter_map(parse_value).collect();

    match values.len() {
        1 => {
            let v = values[0].clone();
            Some([v.clone(), v.clone(), v.clone(), v])
        }
        2 => {
            let tb = values[0].clone();
            let lr = values[1].clone();
            Some([tb.clone(), lr.clone(), tb, lr])
        }
        3 => {
            let top = values[0].clone();
            let lr = values[1].clone();
            let bottom = values[2].clone();
            Some([top, lr.clone(), bottom, lr])
        }
        4 => Some([
            values[0].clone(),
            values[1].clone(),
            values[2].clone(),
            values[3].clone(),
        ]),
        _ => None,
    }
}

/// Parse a gap shorthand.
pub fn parse_gap(value: &str) -> Option<[CssValue; 2]> {
    let values: Vec<CssValue> = value.split_whitespace().filter_map(parse_value).collect();

    match values.len() {
        1 => {
            let v = values[0].clone();
            Some([v.clone(), v])
        }
        2 => Some([values[0].clone(), values[1].clone()]),
        _ => None,
    }
}
