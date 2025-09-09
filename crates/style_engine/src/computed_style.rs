//! Specified → Computed → Used values
//!
//! - Specified values come directly from the cascade (strings/keywords/lengths).
//! - Computed values are resolved where possible without layout context (e.g., display keyword, absolute px).
//! - Used values are resolved later with layout context (percentages, box-sizing, min/max). The StyleEngine
//!   currently produces ComputedStyle only; a future resolver will turn these into used values for layout.
use std::default::Default;

/// The CSS `display` property for computed style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    /// Element is not rendered and does not generate a box.
    None,
    /// Element generates a block-level box.
    Block,
    /// Element generates one or more inline-level boxes.
    Inline,
}

impl Default for Display {
    fn default() -> Self { Display::Inline }
}

/// An RGBA color value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorRGBA {
    /// Red channel (0..=255).
    pub red: u8,
    /// Green channel (0..=255).
    pub green: u8,
    /// Blue channel (0..=255).
    pub blue: u8,
    /// Alpha channel (0..=255), 255 = fully opaque.
    pub alpha: u8,
}

impl Default for ColorRGBA {
    fn default() -> Self { Self { red: 0, green: 0, blue: 0, alpha: 255 } }
}

/// A set of four edge values (top, right, bottom, left) used for margin and padding.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Default for Edges {
    fn default() -> Self { Self { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 } }
}

/// A specified size value for width/height before resolution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizeSpecified {
    /// `auto` value.
    Auto,
    /// Pixel value.
    Px(f32),
    /// Percentage (0.0..=1.0) of the containing block.
    Percent(f32), // 0.0..=1.0
}

impl Default for SizeSpecified {
    fn default() -> Self { SizeSpecified::Auto }
}

/// The minimal set of computed style properties used by early layout code.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    /// Computed `display`.
    pub display: Display,
    /// Computed `margin` edges.
    pub margin: Edges,
    /// Computed `padding` edges.
    pub padding: Edges,
    /// Computed text `color`.
    pub color: ColorRGBA,
    /// Computed `font-size` in CSS pixels.
    pub font_size: f32,     // px
    /// Computed `line-height` as a unitless multiplier of font-size for now.
    pub line_height: f32,   // unitless multiplier of font-size for now
    /// Computed `width` value.
    pub width: SizeSpecified,
    /// Computed `height` value.
    pub height: SizeSpecified,
    /// Computed `min-width` value.
    pub min_width: Option<SizeSpecified>,
    /// Computed `max-width` value.
    pub max_width: Option<SizeSpecified>,
    /// Computed `min-height` value.
    pub min_height: Option<SizeSpecified>,
    /// Computed `max-height` value.
    pub max_height: Option<SizeSpecified>,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            display: Display::default(),
            margin: Edges::default(),
            padding: Edges::default(),
            color: ColorRGBA::default(),
            font_size: 16.0,
            line_height: 1.2,
            width: SizeSpecified::default(),
            height: SizeSpecified::default(),
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
        }
    }
}
