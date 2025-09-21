//! CSS Box Model Module Level 3 — Box dimensions, margins, borders, padding.
//! Spec: <https://www.w3.org/TR/css-box-3/>

use style_engine::ComputedStyle;

/// Box edges used by layout in pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BoxSides {
    pub margin_top: i32,
    pub margin_right: i32,
    pub margin_bottom: i32,
    pub margin_left: i32,

    pub padding_top: i32,
    pub padding_right: i32,
    pub padding_bottom: i32,
    pub padding_left: i32,

    pub border_top: i32,
    pub border_right: i32,
    pub border_bottom: i32,
    pub border_left: i32,
}

/// Resolve margin/padding/border widths from `ComputedStyle` into integer pixels.
///
/// Padding and border widths are clamped to be non-negative. Margins can be negative.
/// Spec: CSS 2.2 §8.1 (box model) and CSS Box Sizing L3.
#[inline]
pub const fn compute_box_sides(style: &ComputedStyle) -> BoxSides {
    BoxSides {
        margin_top: style.margin.top as i32,
        margin_right: style.margin.right as i32,
        margin_bottom: style.margin.bottom as i32,
        margin_left: style.margin.left as i32,

        padding_top: style.padding.top.max(0.0f32) as i32,
        padding_right: style.padding.right.max(0.0f32) as i32,
        padding_bottom: style.padding.bottom.max(0.0f32) as i32,
        padding_left: style.padding.left.max(0.0f32) as i32,

        border_top: style.border_width.top.max(0.0f32) as i32,
        border_right: style.border_width.right.max(0.0f32) as i32,
        border_bottom: style.border_width.bottom.max(0.0f32) as i32,
        border_left: style.border_width.left.max(0.0f32) as i32,
    }
}
