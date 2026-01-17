//! Style computation and cascade resolution for CSS properties.
//!
//! This module coordinates CSS property parsing, cascade resolution, and computed style
//! generation. It maintains a DOM mirror for selector matching and applies UA and author
//! stylesheets according to CSS cascade rules.

mod cascade;
mod parsers;
pub mod ua_stylesheet;

pub use cascade::StyleComputer;

use std::collections::HashMap;

use crate::style_model;

/// Build a computed style from inline declarations with sensible defaults.
/// Inherits font-size, font-family, font-weight, and color from `parent_style` if provided.
pub fn build_computed_from_inline(
    decls: &HashMap<String, String>,
    parent_style: Option<&style_model::ComputedStyle>,
) -> style_model::ComputedStyle {
    // Start with parent's inheritable properties or defaults
    let inherited_font_size = parent_style.map_or(16.0, |parent| parent.font_size);
    let inherited_color = parent_style.map_or(
        style_model::Rgba {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 255,
        },
        |parent| parent.color,
    );
    let inherited_font_weight = parent_style.map_or(400, |parent| parent.font_weight);
    let inherited_font_family = parent_style.and_then(|parent| parent.font_family.clone());
    let inherited_line_height = parent_style.and_then(|parent| parent.line_height);

    let mut computed = style_model::ComputedStyle {
        font_size: inherited_font_size,
        color: inherited_color,
        font_weight: inherited_font_weight,
        font_family: inherited_font_family,
        line_height: inherited_line_height,
        flex_shrink: 1.0, // CSS spec default for flex-shrink
        ..Default::default()
    };

    parsers::layout::apply_layout_keywords(&mut computed, decls);
    parsers::dimensions::apply_dimensions(&mut computed, decls);
    // Typography must be parsed before edges to resolve em units in margins/padding
    parsers::typography::apply_typography(&mut computed, decls);
    parsers::edges::apply_edges_and_borders(&mut computed, decls);
    parsers::colors::apply_colors(&mut computed, decls);
    // Borders may depend on color defaults (currentColor). Finalize after colors.
    parsers::edges::finalize_borders_after_colors(&mut computed);
    parsers::flex::apply_flex_scalars(&mut computed, decls);
    parsers::flex::apply_flex_alignment(&mut computed, decls);
    parsers::gaps::apply_gaps(&mut computed, decls);
    parsers::grid::apply_grid_properties(&mut computed, decls);
    parsers::layout::apply_offsets(&mut computed, decls);
    parsers::colors::apply_effects(&mut computed, decls);

    computed
}

/// Parse a CSS length in pixels; accepts unitless as px for tests. Returns None for auto/none.
pub fn parse_px(input: &str) -> Option<f32> {
    let trimmed = input.trim();
    if trimmed.eq_ignore_ascii_case("auto") || trimmed.eq_ignore_ascii_case("none") {
        return None;
    }
    if let Some(px_suffix_str) = trimmed.strip_suffix("px") {
        return px_suffix_str.trim().parse::<f32>().ok();
    }
    trimmed.parse::<f32>().ok()
}

/// Parse a font-size value with support for em units.
/// Returns the computed pixel value or None if the value cannot be parsed.
///
/// Supported units:
/// - px: absolute pixels
/// - em: relative to parent font size (multiplier × `parent_font_size`)
/// - unitless: treated as pixels for backward compatibility
pub fn parse_font_size(input: &str, parent_font_size: f32) -> Option<f32> {
    let trimmed = input.trim();
    if trimmed.eq_ignore_ascii_case("auto") || trimmed.eq_ignore_ascii_case("none") {
        return None;
    }

    // Handle em units
    if let Some(em_suffix_str) = trimmed.strip_suffix("em") {
        if let Ok(em_value) = em_suffix_str.trim().parse::<f32>() {
            return Some(em_value * parent_font_size);
        }
        return None;
    }

    // Handle px units
    if let Some(px_suffix_str) = trimmed.strip_suffix("px") {
        return px_suffix_str.trim().parse::<f32>().ok();
    }

    // Handle unitless (treat as px for backward compatibility)
    trimmed.parse::<f32>().ok()
}

/// Parse an integer value (used for z-index).
pub fn parse_int(input: &str) -> Option<i32> {
    input.trim().parse::<i32>().ok()
}

/// Normalize a CSS font-family value to match browser behavior.
///
/// Converts single quotes to double quotes for consistency with Chromium.
///
/// # Example
/// `'Courier New', Courier, monospace` → `"Courier New", Courier, monospace`
pub fn normalize_font_family(value: &str) -> String {
    value.replace('\'', "\"")
}
