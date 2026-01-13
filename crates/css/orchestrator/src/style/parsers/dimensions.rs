//! Width, height, min/max dimension parsers.

use std::collections::HashMap;

use crate::style_model;

use super::super::parse_px;

/// Parse a dimension value, returning pixel and percent values.
fn parse_dimension(value: &str) -> (Option<f32>, Option<f32>) {
    let trimmed = value.trim();
    if let Some(percent_str) = trimmed.strip_suffix('%')
        && let Ok(percent_value) = percent_str.trim().parse::<f32>()
    {
        (None, Some((percent_value / 100.0).clamp(0.0, 1.0)))
    } else {
        (parse_px(value), None)
    }
}

/// Parse width/height/min/max and box-sizing.
pub fn apply_dimensions(
    computed: &mut style_model::ComputedStyle,
    decls: &HashMap<String, String>,
) {
    log::trace!(
        "apply_dimensions: decls keys = {:?}",
        decls.keys().collect::<Vec<_>>()
    );

    if let Some(value) = decls.get("width") {
        let (pixels, percent) = parse_dimension(value);
        log::trace!(
            "apply_dimensions: width='{}' -> pixels={:?}, percent={:?}",
            value,
            pixels,
            percent
        );
        computed.width = pixels;
        computed.width_percent = percent;
    }
    if let Some(value) = decls.get("height") {
        let (pixels, percent) = parse_dimension(value);
        computed.height = pixels;
        computed.height_percent = percent;
    }
    if let Some(value) = decls.get("min-width") {
        let (pixels, percent) = parse_dimension(value);
        computed.min_width = pixels;
        computed.min_width_percent = percent.map(|val| val.max(0.0));
    }
    if let Some(value) = decls.get("min-height") {
        let (pixels, percent) = parse_dimension(value);
        computed.min_height = pixels;
        computed.min_height_percent = percent.map(|val| val.max(0.0));
    }
    if let Some(value) = decls.get("max-width") {
        let (pixels, percent) = parse_dimension(value);
        computed.max_width = pixels;
        computed.max_width_percent = percent.map(|val| val.max(0.0));
    }
    if let Some(value) = decls.get("max-height") {
        let (pixels, percent) = parse_dimension(value);
        computed.max_height = pixels;
        computed.max_height_percent = percent.map(|val| val.max(0.0));
    }
    if let Some(value) = decls.get("box-sizing") {
        computed.box_sizing = if value.eq_ignore_ascii_case("border-box") {
            style_model::BoxSizing::BorderBox
        } else {
            style_model::BoxSizing::ContentBox
        };
    }
}
