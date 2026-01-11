//! Font and text property parsers.

use std::collections::HashMap;

use crate::style_model;

use super::super::{normalize_font_family, parse_font_size};

/// Parse a font-size/line-height pair from a string like "14px/1.4" or "2em/1.4".
fn parse_font_size_line_height(size_part: &str, computed: &mut style_model::ComputedStyle) {
    // Store parent font size before we modify computed.font_size
    let parent_font_size = computed.font_size;

    if let Some((size_str, line_str)) = size_part.split_once('/') {
        // Has line-height: "14px/1.4" or "2em/1.4"
        if let Some(pixels) = parse_font_size(size_str, parent_font_size) {
            computed.font_size = pixels;
        }
        // Parse line-height
        let trimmed_line = line_str.trim();
        if let Ok(number) = trimmed_line.parse::<f32>() {
            // Unitless number multiplies font-size
            computed.line_height = Some(number * computed.font_size);
        } else if let Some(px_str) = trimmed_line.strip_suffix("px")
            && let Ok(pixel_value) = px_str.trim().parse::<f32>()
        {
            computed.line_height = Some(pixel_value);
        }
    } else {
        // No line-height, just font-size
        if let Some(pixels) = parse_font_size(size_part, parent_font_size) {
            computed.font_size = pixels;
        }
    }
}

/// Parse font-size and font-family.
pub fn apply_typography(
    computed: &mut style_model::ComputedStyle,
    decls: &HashMap<String, String>,
) {
    // Handle font: shorthand (e.g., "14px/1.4 sans-serif")
    // Simplified parser: assumes format is [font-size]/[line-height] [font-family]
    // or [font-size] [font-family]
    if let Some(font_value) = decls.get("font") {
        let parts: Vec<&str> = font_value.split_whitespace().collect();
        if !parts.is_empty() {
            // First part contains font-size and optionally line-height (size/line-height)
            if let Some(size_part) = parts.first() {
                parse_font_size_line_height(size_part, computed);
            }
            // Remaining parts are font-family
            if parts.len() > 1 {
                let family = parts[1..].join(" ");
                computed.font_family = Some(family);
            }
        }
    }
    // Longhands override shorthand
    // When parsing font-size with em units, use the current computed.font_size
    // (which is the inherited parent font size) as the base for em calculation
    if let Some(value) = decls.get("font-size") {
        let parent_font_size = computed.font_size;
        if let Some(pixels) = parse_font_size(value, parent_font_size) {
            computed.font_size = pixels;
        }
    }
    if let Some(value) = decls.get("font-family") {
        let trimmed = value.trim();
        // CSS 'inherit' keyword means keep the parent's value (already set in computed)
        // Don't overwrite if the value is literally "inherit"
        if !trimmed.eq_ignore_ascii_case("inherit") {
            // Normalize font-family value: remove surrounding quotes from font names
            // CSS allows 'Courier New', "Arial", or unquoted names
            // We store the normalized form for consistent matching
            let normalized = normalize_font_family(value);
            computed.font_family = Some(normalized);
        }
        // If value is "inherit", font_family keeps its inherited value from parent
    }
    // Parse font-weight: numeric values (100-900), keywords (normal=400, bold=700)
    if let Some(value) = decls.get("font-weight") {
        let trimmed = value.trim();
        if trimmed.eq_ignore_ascii_case("normal") {
            computed.font_weight = 400;
        } else if trimmed.eq_ignore_ascii_case("bold") {
            computed.font_weight = 700;
        } else if let Ok(weight) = trimmed.parse::<u16>() {
            // Clamp to valid range 100-900
            computed.font_weight = weight.clamp(100, 900);
        }
    }
    // Parse text-align: left, right, center, justify (other values like start/end not supported yet)
    if let Some(value) = decls.get("text-align") {
        let trimmed = value.trim();
        if trimmed.eq_ignore_ascii_case("left") {
            computed.text_align = style_model::TextAlign::Left;
        } else if trimmed.eq_ignore_ascii_case("right") {
            computed.text_align = style_model::TextAlign::Right;
        } else if trimmed.eq_ignore_ascii_case("center") {
            computed.text_align = style_model::TextAlign::Center;
        } else if trimmed.eq_ignore_ascii_case("justify") {
            computed.text_align = style_model::TextAlign::Justify;
        }
    }
    // Compute line-height: 'normal' -> None; number -> number * font-size; percentage -> resolved;
    // length (px) -> as-is. Other units not yet supported in this minimal engine.
    if let Some(raw) = decls.get("line-height") {
        let trimmed = raw.trim();
        if trimmed.eq_ignore_ascii_case("normal") {
            computed.line_height = None;
        } else if let Some(percent_str) = trimmed.strip_suffix('%') {
            if let Ok(percent_value) = percent_str.trim().parse::<f32>() {
                computed.line_height = Some(computed.font_size * (percent_value / 100.0));
            }
        } else if let Some(px_str) = trimmed.strip_suffix("px") {
            if let Ok(pixel_value) = px_str.trim().parse::<f32>() {
                computed.line_height = Some(pixel_value);
            }
        } else if let Ok(number) = trimmed.parse::<f32>() {
            // Unitless number multiplies element's font-size
            computed.line_height = Some(number * computed.font_size);
        }
    }
}
