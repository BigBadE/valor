//! Margin, padding, and border property parsers.

use std::collections::HashMap;

use crate::style_model;
use css_color::parse_css_color;

use super::super::{parse_font_size, parse_px};

/// Denotes a single border side for per-side shorthand parsing.
#[derive(Clone, Copy)]
enum BorderSide {
    /// Top border side.
    Top,
    /// Right border side.
    Right,
    /// Bottom border side.
    Bottom,
    /// Left border side.
    Left,
}

/// Parse 4 edge values with longhand names like "{prefix}-top" in pixels.
/// Supports px, em, and unitless values. Em values are resolved against font_size.
fn parse_edges(
    prefix: &str,
    decls: &HashMap<String, String>,
    font_size: f32,
) -> style_model::Edges {
    // Helper to parse a single edge value (px, em, or unitless)
    let parse_edge_value = |value: &str| -> Option<f32> {
        // Try parse_font_size which handles px, em, and unitless
        parse_font_size(value, font_size)
    };

    // Start from shorthand if present
    let mut edges = decls
        .get(prefix)
        .map_or_else(style_model::Edges::default, |shorthand| {
            let numbers: Vec<f32> = shorthand
                .split(|character: char| character.is_ascii_whitespace())
                .filter(|segment| !segment.is_empty())
                .filter_map(|seg| parse_edge_value(seg))
                .collect();
            match *numbers.as_slice() {
                [one] => style_model::Edges {
                    top: one,
                    right: one,
                    bottom: one,
                    left: one,
                },
                [top, right] => style_model::Edges {
                    top,
                    right,
                    bottom: top,
                    left: right,
                },
                [top, right, bottom] => style_model::Edges {
                    top,
                    right,
                    bottom,
                    left: right,
                },
                [top, right, bottom, left] => style_model::Edges {
                    top,
                    right,
                    bottom,
                    left,
                },
                _ => style_model::Edges::default(),
            }
        });
    // Longhands override shorthand sides if present
    if let Some(value) = decls.get(&format!("{prefix}-top"))
        && let Some(pixels) = parse_edge_value(value)
    {
        edges.top = pixels;
    }
    if let Some(value) = decls.get(&format!("{prefix}-right"))
        && let Some(pixels) = parse_edge_value(value)
    {
        edges.right = pixels;
    }
    if let Some(value) = decls.get(&format!("{prefix}-bottom"))
        && let Some(pixels) = parse_edge_value(value)
    {
        edges.bottom = pixels;
    }
    if let Some(value) = decls.get(&format!("{prefix}-left"))
        && let Some(pixels) = parse_edge_value(value)
    {
        edges.left = pixels;
    }
    edges
}

/// Parse margin-left/right 'auto' from shorthand and longhands and set flags + zero px values.
fn apply_margin_auto_flags(
    computed: &mut style_model::ComputedStyle,
    decls: &HashMap<String, String>,
) {
    computed.margin_left_auto = false;
    computed.margin_right_auto = false;
    // Shorthand handling (TRBL mapping)
    if let Some(shorthand) = decls.get("margin") {
        let tokens: Vec<&str> = shorthand
            .split(|character: char| character.is_ascii_whitespace())
            .filter(|segment| !segment.is_empty())
            .collect();
        let right_token: Option<&str> = match tokens.len() {
            1 => tokens.first().copied(),
            2..=4 => tokens.get(1).copied(),
            _ => None,
        };
        let left_token: Option<&str> = match tokens.len() {
            1 => tokens.first().copied(),
            2 | 3 => tokens.get(1).copied(),
            4 => tokens.get(3).copied(),
            _ => None,
        };
        if let Some(tok) = left_token
            && tok.eq_ignore_ascii_case("auto")
        {
            computed.margin_left_auto = true;
            computed.margin.left = 0.0;
        }
        if let Some(tok) = right_token
            && tok.eq_ignore_ascii_case("auto")
        {
            computed.margin_right_auto = true;
            computed.margin.right = 0.0;
        }
    }
    // Longhands override
    if let Some(value) = decls.get("margin-left")
        && value.trim().eq_ignore_ascii_case("auto")
    {
        computed.margin_left_auto = true;
        computed.margin.left = 0.0;
    }
    if let Some(value) = decls.get("margin-right")
        && value.trim().eq_ignore_ascii_case("auto")
    {
        computed.margin_right_auto = true;
        computed.margin.right = 0.0;
    }
}

/// Parse and apply per-side `border-<side>` shorthand tokens.
///
/// Accepts `<width> <style> <color>` in any order. Only the targeted side's width/style/color
/// are updated. Spec parsing model reference: CSS2.2 border shorthand tokenization (mirrored
/// from `border`).
fn apply_border_side_shorthand_tokens(
    value: &str,
    computed: &mut style_model::ComputedStyle,
    side: BorderSide,
) {
    let mut width_opt: Option<f32> = None;
    let mut style_opt: Option<style_model::BorderStyle> = None;
    let mut color_opt: Option<style_model::Rgba> = None;
    for token_text in value
        .split(|character: char| character.is_ascii_whitespace())
        .filter(|text| !text.is_empty())
    {
        if width_opt.is_none()
            && let Some(px_value) = parse_px(token_text)
        {
            width_opt = Some(px_value);
            continue;
        }
        if style_opt.is_none() {
            if token_text.eq_ignore_ascii_case("solid") {
                style_opt = Some(style_model::BorderStyle::Solid);
                continue;
            }
            if token_text.eq_ignore_ascii_case("none") {
                style_opt = Some(style_model::BorderStyle::None);
                continue;
            }
        }
        if color_opt.is_none()
            && let Some((red8, green8, blue8, alpha8)) = parse_css_color(token_text)
        {
            color_opt = Some(style_model::Rgba {
                red: red8,
                green: green8,
                blue: blue8,
                alpha: alpha8,
            });
        }
    }
    if let Some(px_value) = width_opt {
        match side {
            BorderSide::Top => computed.border_width.top = px_value,
            BorderSide::Right => computed.border_width.right = px_value,
            BorderSide::Bottom => computed.border_width.bottom = px_value,
            BorderSide::Left => computed.border_width.left = px_value,
        }
    } else if matches!(style_opt, Some(style_model::BorderStyle::Solid)) {
        // Width omitted but style specified on per-side shorthand -> default medium for that side only.
        let medium = 3.0f32;
        match side {
            BorderSide::Top => computed.border_width.top = medium,
            BorderSide::Right => computed.border_width.right = medium,
            BorderSide::Bottom => computed.border_width.bottom = medium,
            BorderSide::Left => computed.border_width.left = medium,
        }
    } else if matches!(style_opt, Some(style_model::BorderStyle::None)) {
        // border-*: none should set border width to 0 for that side
        match side {
            BorderSide::Top => computed.border_width.top = 0.0,
            BorderSide::Right => computed.border_width.right = 0.0,
            BorderSide::Bottom => computed.border_width.bottom = 0.0,
            BorderSide::Left => computed.border_width.left = 0.0,
        }
    }
    if let Some(style_value) = style_opt {
        // Update overall border-style; tests and our layouter only consider solid/none, and
        // side-specific border-style resolution is simplified here to global for parity.
        computed.border_style = style_value;
    }
    if let Some(color_value) = color_opt {
        // Update overall border color; side-specific colors are not modeled in this minimal engine.
        computed.border_color = color_value;
    }
}

/// Parse and apply `border` shorthand tokens: <width> <style> <color> in any order.
fn apply_border_shorthand_tokens(value: &str, computed: &mut style_model::ComputedStyle) {
    let mut width_opt: Option<f32> = None;
    let mut style_opt: Option<style_model::BorderStyle> = None;
    let mut color_opt: Option<style_model::Rgba> = None;
    for token_text in value
        .split(|character: char| character.is_ascii_whitespace())
        .filter(|text| !text.is_empty())
    {
        if width_opt.is_none()
            && let Some(px_value) = parse_px(token_text)
        {
            width_opt = Some(px_value);
            continue;
        }
        if style_opt.is_none() {
            if token_text.eq_ignore_ascii_case("solid") {
                style_opt = Some(style_model::BorderStyle::Solid);
                continue;
            }
            if token_text.eq_ignore_ascii_case("none") {
                style_opt = Some(style_model::BorderStyle::None);
                continue;
            }
        }
        if color_opt.is_none()
            && let Some((red8, green8, blue8, alpha8)) = parse_css_color(token_text)
        {
            color_opt = Some(style_model::Rgba {
                red: red8,
                green: green8,
                blue: blue8,
                alpha: alpha8,
            });
        }
    }
    if let Some(px_value) = width_opt {
        computed.border_width = style_model::BorderWidths {
            top: px_value,
            right: px_value,
            bottom: px_value,
            left: px_value,
        };
    } else if matches!(style_opt, Some(style_model::BorderStyle::Solid)) {
        // Width omitted but style specified on `border` shorthand -> default medium on all sides.
        let medium = 3.0f32;
        computed.border_width = style_model::BorderWidths {
            top: medium,
            right: medium,
            bottom: medium,
            left: medium,
        };
    }
    if let Some(style_value) = style_opt {
        computed.border_style = style_value;
    }
    if let Some(color_value) = color_opt {
        computed.border_color = color_value;
    }
}

/// Apply `border-style` longhand when present.
fn apply_border_style_longhand(
    decls: &HashMap<String, String>,
    computed: &mut style_model::ComputedStyle,
) {
    if let Some(value) = decls.get("border-style") {
        computed.border_style = if value.eq_ignore_ascii_case("solid") {
            style_model::BorderStyle::Solid
        } else {
            style_model::BorderStyle::None
        };
    }
}

/// Parse margins, paddings, and border subset.
pub fn apply_edges_and_borders(
    computed: &mut style_model::ComputedStyle,
    decls: &HashMap<String, String>,
) {
    // Use current font_size for em unit resolution
    let font_size = computed.font_size;

    computed.margin = parse_edges("margin", decls, font_size);
    // Margin auto flags from shorthand and longhands
    apply_margin_auto_flags(computed, decls);
    computed.padding = parse_edges("padding", decls, font_size);
    // 1) Border widths
    // Prefer explicit 'border-width' (and per-side longhands) when present.
    let border_widths_from_bw = parse_edges("border-width", decls, font_size);
    let has_bw_longhands = decls.contains_key("border-width")
        || decls.contains_key("border-top-width")
        || decls.contains_key("border-right-width")
        || decls.contains_key("border-bottom-width")
        || decls.contains_key("border-left-width");
    if has_bw_longhands {
        computed.border_width = style_model::BorderWidths {
            top: border_widths_from_bw.top,
            right: border_widths_from_bw.right,
            bottom: border_widths_from_bw.bottom,
            left: border_widths_from_bw.left,
        };
    } else {
        // Fallback: numeric-only shorthand widths for 'border' when author uses 'border: 1px 2px ...'
        let border_widths_tmp = parse_edges("border", decls, font_size);
        computed.border_width = style_model::BorderWidths {
            top: border_widths_tmp.top,
            right: border_widths_tmp.right,
            bottom: border_widths_tmp.bottom,
            left: border_widths_tmp.left,
        };
    }

    // 2) Full border shorthand tokens (all sides)
    if let Some(value) = decls.get("border") {
        apply_border_shorthand_tokens(value, computed);
    }
    // 2b) Per-side border shorthands: border-top/right/bottom/left
    // CSS permits per-side shorthands with tokens in any order: <width> <style> <color>.
    // Parse each side independently and override only that side's width/style/color.
    if let Some(value) = decls.get("border-top") {
        apply_border_side_shorthand_tokens(value, computed, BorderSide::Top);
    }
    if let Some(value) = decls.get("border-right") {
        apply_border_side_shorthand_tokens(value, computed, BorderSide::Right);
    }
    if let Some(value) = decls.get("border-bottom") {
        apply_border_side_shorthand_tokens(value, computed, BorderSide::Bottom);
    }
    if let Some(value) = decls.get("border-left") {
        apply_border_side_shorthand_tokens(value, computed, BorderSide::Left);
    }
    // 3) Longhand border-style (all sides or per-side)
    apply_border_style_longhand(decls, computed);
    // If author specified 'border-style' longhand as solid but omitted widths on some sides,
    // assign default medium width (3px) only to sides that are still zero/unspecified.
    if decls.contains_key("border-style")
        && matches!(computed.border_style, style_model::BorderStyle::Solid)
    {
        let medium = 3.0f32;
        if computed.border_width.top <= 0.0 {
            computed.border_width.top = medium;
        }
        if computed.border_width.right <= 0.0 {
            computed.border_width.right = medium;
        }
        if computed.border_width.bottom <= 0.0 {
            computed.border_width.bottom = medium;
        }
        if computed.border_width.left <= 0.0 {
            computed.border_width.left = medium;
        }
    }
}

/// Finalize border after colors are known: if widths exist, style is None, and color is present,
/// promote style to Solid; then ensure default widths for solid.
pub fn finalize_borders_after_colors(computed: &mut style_model::ComputedStyle) {
    let any_width = computed.border_width.top > 0.0
        || computed.border_width.right > 0.0
        || computed.border_width.bottom > 0.0
        || computed.border_width.left > 0.0;
    if any_width
        && matches!(computed.border_style, style_model::BorderStyle::None)
        && computed.border_color.alpha > 0
    {
        computed.border_style = style_model::BorderStyle::Solid;
    }
}
