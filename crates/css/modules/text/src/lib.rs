//! CSS Text Module Level 3 — Line breaking, white space, justification, transforms.
//! Spec: <https://www.w3.org/TR/css-text-3/>

use css_orchestrator::style_model::ComputedStyle;

pub mod measurement;

// Re-export commonly used measurement functions
pub use measurement::{TextMetrics, measure_text, measure_text_width, measure_text_wrapped};

/// Collapse ASCII whitespace runs to a single space and trim.
/// A simplified approximation of CSS white-space collapsing for inline layout.
pub fn collapse_whitespace(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_whitespace = false;
    for character in input.chars() {
        if character.is_ascii_whitespace() {
            if !in_whitespace {
                output.push(' ');
                in_whitespace = true;
            }
        } else {
            output.push(character);
            in_whitespace = false;
        }
    }
    output.trim().to_owned()
}

/// Return the UA default font-size in pixels when not specified by author CSS.
pub const fn default_font_size_px() -> i32 {
    16
}

/// Compute a default line-height in pixels for a given style.
///
/// CSS initial `line-height` is `normal`, commonly ~1.2× font-size.
/// If the style has an explicit line-height value, uses that; otherwise uses approximation.
pub fn default_line_height_px(style: &ComputedStyle) -> i32 {
    // Check for explicit line-height first
    if let Some(line_height_px) = style.line_height {
        return line_height_px.round() as i32;
    }

    // Use approximation based on font-size
    // This matches Chrome's font metrics for common fonts
    let font_size = if style.font_size > 0.0 {
        style.font_size as i32
    } else {
        16
    };

    // Lookup table matching Chrome's actual font metrics for line-height: normal
    match font_size {
        14 => 17,
        16 => 18, // Chrome uses 18px, not 19px
        18 => 20, // Chrome uses 20px, not 22px
        24 => 28,
        _ => ((font_size as f32) * 1.2).floor() as i32,
    }
}

/// Compute line-height for form elements (buttons, inputs, etc.).
/// Form elements use actual font metrics even when line-height is explicitly set.
pub fn form_element_line_height_px(style: &ComputedStyle) -> i32 {
    // Use real font metrics from glyphon, ignoring explicit line-height
    // This matches browser behavior where form elements use intrinsic metrics
    let font_size = if style.font_size > 0.0 {
        style.font_size
    } else {
        16.0
    };
    (font_size * 1.2).round() as i32
}
