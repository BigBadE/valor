use std::default::Default;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    None,
    Block,
    Inline,
}

impl Default for Display {
    fn default() -> Self { Display::Inline }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorRGBA {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Default for ColorRGBA {
    fn default() -> Self { Self { r: 0, g: 0, b: 0, a: 255 } }
}

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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizeSpecified {
    Auto,
    Px(f32),
    Percent(f32), // 0.0..=1.0
}

impl Default for SizeSpecified {
    fn default() -> Self { SizeSpecified::Auto }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    pub display: Display,
    pub margin: Edges,
    pub padding: Edges,
    pub color: ColorRGBA,
    pub font_size: f32,     // px
    pub line_height: f32,   // unitless multiplier of font-size for now
    pub width: SizeSpecified,
    pub height: SizeSpecified,
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
        }
    }
}
