//! Text rendering types.

use rewrite_core::{Color, Keyword};

/// Shaped text ready for rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapedText {
    /// Positioned glyphs.
    pub glyphs: Vec<PositionedGlyph>,
    /// Font ID.
    pub font_id: u64,
    /// Font size in pixels.
    pub font_size: f32,
    /// Text color.
    pub color: Color,
    /// Font features.
    pub features: Vec<FontFeature>,
    /// Font variations (for variable fonts).
    pub variations: Vec<FontVariation>,
}

/// A glyph positioned for rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionedGlyph {
    /// Glyph ID in the font.
    pub glyph_id: u32,
    /// X offset from text origin.
    pub x: f32,
    /// Y offset from text origin.
    pub y: f32,
    /// Advance width.
    pub advance: f32,
}

/// Font feature (e.g., ligatures, small-caps).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFeature {
    /// Feature tag (e.g., "liga", "smcp").
    pub tag: [u8; 4],
    /// Feature value (usually 0 or 1).
    pub value: u32,
}

/// Font variation axis (for variable fonts).
#[derive(Debug, Clone, PartialEq)]
pub struct FontVariation {
    /// Variation tag (e.g., "wght", "wdth").
    pub tag: [u8; 4],
    /// Variation value.
    pub value: f32,
}

/// Text decoration (underline, overline, line-through).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextDecoration {
    /// Decoration type (Underline, Overline, LineThrough).
    pub kind: Keyword,
    /// Decoration style (Solid, Double, Dotted, Dashed, Wavy).
    pub style: Keyword,
    /// Decoration color.
    pub color: Color,
    /// Thickness in pixels.
    pub thickness: f32,
    /// Vertical offset from baseline.
    pub offset: f32,
}

/// Text shadow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextShadow {
    /// Horizontal offset.
    pub offset_x: f32,
    /// Vertical offset.
    pub offset_y: f32,
    /// Blur radius.
    pub blur_radius: f32,
    /// Shadow color.
    pub color: Color,
}
