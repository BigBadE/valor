//! CSS Text Module Level 3 — Line breaking, white space, justification, transforms.
//! Spec: <https://www.w3.org/TR/css-text-3/>

use css_orchestrator::style_model::ComputedStyle;

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
/// CSS initial `line-height` is `normal`, commonly ~1.2× font-size.
pub fn default_line_height_px(style: &ComputedStyle) -> i32 {
    let font_size_px: f32 = if style.font_size > 0.0 {
        style.font_size
    } else {
        default_font_size_px() as f32
    };
    (font_size_px * 1.2).round() as i32
}
