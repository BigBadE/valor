//! Color and visual effects property parsers.

use std::collections::HashMap;

use crate::style_model;
use css_color::parse_css_color;

/// Apply color-related properties from declarations to the computed style.
pub fn apply_colors(computed: &mut style_model::ComputedStyle, decls: &HashMap<String, String>) {
    if let Some(value) = decls.get("color")
        && let Some((red8, green8, blue8, alpha8)) = parse_css_color(value)
    {
        computed.color = style_model::Rgba {
            red: red8,
            green: green8,
            blue: blue8,
            alpha: alpha8,
        };
    }
    if let Some(value) = decls.get("background-color")
        && let Some((red8, green8, blue8, alpha8)) = parse_css_color(value)
    {
        computed.background_color = style_model::Rgba {
            red: red8,
            green: green8,
            blue: blue8,
            alpha: alpha8,
        };
    }
    // background shorthand: color-only
    if let Some(value) = decls.get("background")
        && computed.background_color.alpha == 0
    {
        // First, try parsing the entire value as a single color (e.g., "rgb(32, 64, 96)").
        if let Some((red8, green8, blue8, alpha8)) = parse_css_color(value) {
            computed.background_color = style_model::Rgba {
                red: red8,
                green: green8,
                blue: blue8,
                alpha: alpha8,
            };
        } else {
            // Fallback: tokenize and look for a color token among other background tokens
            for token_text in value
                .split(|character: char| character.is_ascii_whitespace())
                .filter(|text| !text.is_empty())
            {
                if let Some((red8, green8, blue8, alpha8)) = parse_css_color(token_text) {
                    computed.background_color = style_model::Rgba {
                        red: red8,
                        green: green8,
                        blue: blue8,
                        alpha: alpha8,
                    };
                    break;
                }
            }
        }
    }
    if let Some(value) = decls.get("border-color")
        && let Some((red8, green8, blue8, alpha8)) = parse_css_color(value)
    {
        computed.border_color = style_model::Rgba {
            red: red8,
            green: green8,
            blue: blue8,
            alpha: alpha8,
        };
    }
    // Default border color to currentColor if unspecified and style is not None
    if computed.border_color.alpha == 0
        && !matches!(computed.border_style, style_model::BorderStyle::None)
    {
        computed.border_color = computed.color;
    }
}

/// Parse visual effects such as `opacity`.
pub fn apply_effects(computed: &mut style_model::ComputedStyle, decls: &HashMap<String, String>) {
    if let Some(raw) = decls.get("opacity") {
        let trimmed = raw.trim();
        let mut parsed: Option<f32> = None;
        if let Some(percent_str) = trimmed.strip_suffix('%')
            && let Ok(percent_value) = percent_str.trim().parse::<f32>()
        {
            parsed = Some((percent_value / 100.0).clamp(0.0, 1.0));
        } else if let Ok(number) = trimmed.parse::<f32>() {
            parsed = Some(number.clamp(0.0, 1.0));
        }
        if let Some(alpha) = parsed {
            // Store None for 1.0 to keep default fast path; otherwise Some(alpha)
            if alpha >= 1.0 {
                computed.opacity = None;
            } else if alpha <= 0.0 {
                computed.opacity = Some(0.0);
            } else {
                computed.opacity = Some(alpha);
            }
        }
    }
}
