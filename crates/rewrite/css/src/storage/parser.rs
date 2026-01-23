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
        "auto" => Some(CssKeyword::Auto),
        "none" => Some(CssKeyword::None),
        "normal" => Some(CssKeyword::Normal),
        "initial" => Some(CssKeyword::Initial),
        "inherit" => Some(CssKeyword::Inherit),
        "unset" => Some(CssKeyword::Unset),
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

    if let Some(keyword) = parse_keyword(value) {
        return Some(CssValue::Keyword(keyword));
    }

    if let Some(length) = parse_length(value) {
        return Some(CssValue::Length(length));
    }

    if let Some(num) = parse_number(value) {
        return Some(CssValue::Number(num));
    }

    None
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
