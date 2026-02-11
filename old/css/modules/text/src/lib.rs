//! CSS Text Module Level 3 — Line breaking, white space, justification, transforms.
//! Spec: <https://www.w3.org/TR/css-text-3/>

use css_orchestrator::style_model::ComputedStyle;

pub mod measurement;

// Re-export commonly used measurement functions
pub use measurement::{
    TextMetrics, WrappedTextMetrics, map_font_family, measure_text, measure_text_width,
    measure_text_wrapped,
};

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

/// Compute a default line-height in pixels for a given style (spec-compliant).
///
/// CSS initial `line-height` is `normal`, which should use the font's intrinsic metrics.
/// This is an approximation that matches common browser behavior.
pub fn default_line_height_px(style: &ComputedStyle) -> i32 {
    // Use the font size with a multiplier
    let font_size = if style.font_size > 0.0 {
        style.font_size
    } else {
        16.0
    };

    // Match Chrome's line-height values for common font sizes
    // Chrome uses ~1.5x font-size for normal line-height with system fonts
    match font_size.round() as i32 {
        12 => 18, // 12 * 1.5 = 18
        14 => 21, // 14 * 1.5 = 21
        16 => 24, // 16 * 1.5 = 24
        18 => 27, // 18 * 1.5 = 27
        20 => 30, // 20 * 1.5 = 30
        24 => 36, // 24 * 1.5 = 36
        _ => (font_size * 1.5).round() as i32,
    }
}

/// Compute line-height for form elements (buttons, inputs, etc.).
/// Form elements use actual font metrics even when line-height is explicitly set.
pub fn form_element_line_height_px(style: &ComputedStyle) -> i32 {
    // Form elements use the same line-height lookup as text, but inputs may have
    // slightly different intrinsic sizing behavior
    default_line_height_px(style)
}
