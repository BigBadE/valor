//! Specified → Computed → Used values
//!
//! - Specified values come directly from the cascade (strings/keywords/lengths).
//! - Computed values are resolved where possible without layout context (e.g., display keyword, absolute px).
//! - Used values are resolved later with layout context (percentages, box-sizing, min/max). The StyleEngine
//!   currently produces ComputedStyle only; a future resolver will turn these into used values for layout.
use std::default::Default;
use std::collections::HashMap;

/// The CSS `display` property for computed style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    /// Element is not rendered and does not generate a box.
    None,
    /// Element generates a block-level box.
    Block,
    /// Element generates one or more inline-level boxes.
    Inline,
    /// Element establishes a flex formatting context (block-level).
    Flex,
    /// Element establishes an inline-level flex formatting context.
    InlineFlex,
}

impl Default for Display {
    fn default() -> Self { Display::Inline }
}

/// Positioning scheme for layout (computed layer only; layout effects later).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

impl Default for Position {
    fn default() -> Self { Position::Static }
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

/// Border line style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderStyle {
    None,
    Solid,
    Dashed,
    Dotted,
}

impl Default for BorderStyle { fn default() -> Self { BorderStyle::None } }

/// Cross-axis alignment for flex containers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignItems {
    Stretch,
    FlexStart,
    FlexEnd,
    Center,
    Baseline,
}

impl Default for AlignItems { fn default() -> Self { AlignItems::Stretch } }

/// Overflow/clipping behavior for boxes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overflow {
    Visible,
    Hidden,
    Scroll,
    Auto,
}

impl Default for Overflow { fn default() -> Self { Overflow::Visible } }

/// Font style keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

impl Default for FontStyle { fn default() -> Self { FontStyle::Normal } }

/// The minimal set of computed style properties used by early layout code.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    /// Computed `display`.
    pub display: Display,
    /// Computed positioning scheme.
    pub position: Position,
    /// Computed `margin` edges.
    pub margin: Edges,
    /// Computed `padding` edges.
    pub padding: Edges,
    /// Computed border widths per side.
    pub border_width: Edges,
    /// Computed border style (uniform for now).
    pub border_style: BorderStyle,
    /// Computed border color (uniform for now).
    pub border_color: ColorRGBA,
    /// Computed background color (alpha may be 0 for transparent).
    pub background_color: ColorRGBA,
    /// Computed text `color`.
    pub color: ColorRGBA,
    /// Computed `font-size` in CSS pixels.
    pub font_size: f32,     // px
    /// Computed `line-height` as a unitless multiplier of font-size for now.
    pub line_height: f32,   // unitless multiplier of font-size for now
    /// Computed font weight as numeric 100..900 steps (400 normal, 700 bold).
    pub font_weight: u16,
    /// Computed font style keyword.
    pub font_style: FontStyle,
    /// Primary font family name (as specified; no resolution yet).
    pub font_family: String,
    /// Computed `width` value.
    pub width: SizeSpecified,
    /// Computed `height` value.
    pub height: SizeSpecified,
    /// Computed offsets for positioned elements (None/Auto when unspecified).
    pub top: Option<SizeSpecified>,
    pub right: Option<SizeSpecified>,
    pub bottom: Option<SizeSpecified>,
    pub left: Option<SizeSpecified>,
    /// Computed overflow behavior (applies to both axes for now).
    pub overflow: Overflow,
    /// Computed `min-width` value.
    pub min_width: Option<SizeSpecified>,
    /// Computed `max-width` value.
    pub max_width: Option<SizeSpecified>,
    /// Computed `min-height` value.
    pub min_height: Option<SizeSpecified>,
    /// Computed `max-height` value.
    pub max_height: Option<SizeSpecified>,
    /// Flex formatting context properties applied on items.
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: SizeSpecified,
    /// Cross-axis alignment for container.
    pub align_items: AlignItems,
    /// Resolved CSS custom properties (computed), inherited by default.
    pub custom_properties: HashMap<String, String>,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        Self {
            display: Display::default(),
            position: Position::default(),
            margin: Edges::default(),
            padding: Edges::default(),
            border_width: Edges::default(),
            border_style: BorderStyle::default(),
            border_color: ColorRGBA::default(),
            background_color: ColorRGBA { red: 0, green: 0, blue: 0, alpha: 0 },
            color: ColorRGBA::default(),
            font_size: 16.0,
            line_height: 1.2,
            font_weight: 400,
            font_style: FontStyle::default(),
            font_family: String::new(),
            width: SizeSpecified::default(),
            height: SizeSpecified::default(),
            top: None,
            right: None,
            bottom: None,
            left: None,
            overflow: Overflow::default(),
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: SizeSpecified::Auto,
            align_items: AlignItems::Stretch,
            custom_properties: HashMap::new(),
        }
    }
}
