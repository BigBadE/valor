//! Width, height, min/max dimension parsers.

use std::collections::HashMap;

use crate::style_model;

use super::super::parse_px;

/// Parse width/height/min/max and box-sizing.
pub fn apply_dimensions(
    computed: &mut style_model::ComputedStyle,
    decls: &HashMap<String, String>,
) {
    if let Some(value) = decls.get("width") {
        computed.width = parse_px(value);
    }
    if let Some(value) = decls.get("height") {
        let trimmed = value.trim();
        if let Some(percent_str) = trimmed.strip_suffix('%')
            && let Ok(percent_value) = percent_str.trim().parse::<f32>()
        {
            computed.height = None;
            computed.height_percent = Some((percent_value / 100.0).clamp(0.0, 1.0));
        } else {
            computed.height = parse_px(value);
            computed.height_percent = None;
        }
    }
    if let Some(value) = decls.get("min-width") {
        computed.min_width = parse_px(value);
    }
    if let Some(value) = decls.get("min-height") {
        let trimmed = value.trim();
        if let Some(percent_str) = trimmed.strip_suffix('%')
            && let Ok(percent_value) = percent_str.trim().parse::<f32>()
        {
            computed.min_height = None;
            computed.min_height_percent = Some((percent_value / 100.0).max(0.0));
        } else {
            computed.min_height = parse_px(value);
            computed.min_height_percent = None;
        }
    }
    if let Some(value) = decls.get("max-width") {
        computed.max_width = parse_px(value);
    }
    if let Some(value) = decls.get("max-height") {
        let trimmed = value.trim();
        if let Some(percent_str) = trimmed.strip_suffix('%')
            && let Ok(percent_value) = percent_str.trim().parse::<f32>()
        {
            computed.max_height = None;
            computed.max_height_percent = Some((percent_value / 100.0).max(0.0));
        } else {
            computed.max_height = parse_px(value);
            computed.max_height_percent = None;
        }
    }
    if let Some(value) = decls.get("box-sizing") {
        computed.box_sizing = if value.eq_ignore_ascii_case("border-box") {
            style_model::BoxSizing::BorderBox
        } else {
            style_model::BoxSizing::ContentBox
        };
    }
}
