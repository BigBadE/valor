//! CSS Box Model Module Level 3 — Box dimensions, margins, borders, padding.
//! Spec: <https://www.w3.org/TR/css-box-3/>

pub mod layout_unit;
pub use layout_unit::LayoutUnit;

use css_orchestrator::style_model::ComputedStyle;

/// Box edges used by layout in sub-pixel precision.
///
/// Chromium and other browsers use sub-pixel layout coordinates to avoid
/// cumulative rounding errors. We use `LayoutUnit` (1/64px precision) to match
/// this behavior while using integer arithmetic.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BoxSides {
    pub margin_top: LayoutUnit,
    pub margin_right: LayoutUnit,
    pub margin_bottom: LayoutUnit,
    pub margin_left: LayoutUnit,

    pub padding_top: LayoutUnit,
    pub padding_right: LayoutUnit,
    pub padding_bottom: LayoutUnit,
    pub padding_left: LayoutUnit,

    pub border_top: LayoutUnit,
    pub border_right: LayoutUnit,
    pub border_bottom: LayoutUnit,
    pub border_left: LayoutUnit,
}

/// Resolve margin/padding/border widths from `ComputedStyle` into `LayoutUnit`.
///
/// Padding and border widths are clamped to be non-negative. Margins can be negative.
/// Values are quantized to 1/64px precision to match browser sub-pixel layout.
/// Spec: CSS 2.2 §8.1 (box model) and CSS Box Sizing L3.
pub fn compute_box_sides(style: &ComputedStyle) -> BoxSides {
    BoxSides {
        margin_top: LayoutUnit::from_px(style.margin.top),
        margin_right: LayoutUnit::from_px(style.margin.right),
        margin_bottom: LayoutUnit::from_px(style.margin.bottom),
        margin_left: LayoutUnit::from_px(style.margin.left),

        padding_top: LayoutUnit::from_px(style.padding.top.max(0.0)),
        padding_right: LayoutUnit::from_px(style.padding.right.max(0.0)),
        padding_bottom: LayoutUnit::from_px(style.padding.bottom.max(0.0)),
        padding_left: LayoutUnit::from_px(style.padding.left.max(0.0)),

        border_top: LayoutUnit::from_px(style.border_width.top.max(0.0)),
        border_right: LayoutUnit::from_px(style.border_width.right.max(0.0)),
        border_bottom: LayoutUnit::from_px(style.border_width.bottom.max(0.0)),
        border_left: LayoutUnit::from_px(style.border_width.left.max(0.0)),
    }
}
