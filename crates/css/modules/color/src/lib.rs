//! CSS Color Module Level 4 — Color spaces, color values, and opacity.
//! Spec: <https://www.w3.org/TR/css-color-4/>
use csscolorparser::Color;

/// Parse a CSS <color> into 8-bit RGBA channels.
///
/// Supports named colors, hex forms (`#rgb`/`#rgba`/`#rrggbb`/`#rrggbbaa`),
/// and functional notations like `rgb()/rgba()`.
///
/// Spec: <https://www.w3.org/TR/css-color-4/#typedef-color>
/// Spec: <https://www.w3.org/TR/css-color-4/#legacy-color-values>
pub type Rgba8Tuple = (u8, u8, u8, u8);
#[inline]
pub fn parse_css_color(input: &str) -> Option<Rgba8Tuple> {
    let parsed: Color = input.parse().ok()?;
    let channels = parsed.to_rgba8();
    let red = channels[0];
    let green = channels[1];
    let blue = channels[2];
    let alpha = channels[3];
    Some((red, green, blue, alpha))
}
