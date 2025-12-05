#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rgba {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WritingMode {
    #[default]
    HorizontalTb,
    VerticalRl,
    VerticalLr,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BorderWidths {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BorderStyle {
    #[default]
    None,
    Solid,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Overflow {
    #[default]
    Visible,
    Hidden,
    Clip,
    Auto,
    Scroll,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Position {
    #[default]
    Static,
    Relative,
    Absolute,
    Fixed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Float {
    #[default]
    None,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Clear {
    #[default]
    None,
    Left,
    Right,
    Both,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Display {
    #[default]
    Inline,
    Block,
    InlineBlock,
    Flex,
    InlineFlex,
    None,
    Contents,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BoxSizing {
    #[default]
    ContentBox,
    BorderBox,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlexDirection {
    #[default]
    Row,
    Column,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlexWrap {
    #[default]
    NoWrap,
    Wrap,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlignItems {
    #[default]
    Stretch,
    FlexStart,
    Center,
    FlexEnd,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlignContent {
    #[default]
    Stretch,
    FlexStart,
    Center,
    FlexEnd,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum JustifyContent {
    #[default]
    FlexStart,
    Center,
    FlexEnd,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComputedStyle {
    pub color: Rgba,
    pub background_color: Rgba,
    pub border_width: BorderWidths,
    pub border_style: BorderStyle,
    pub border_color: Rgba,
    pub font_size: f32,
    /// Computed line-height in pixels when specified; None represents 'normal'.
    pub line_height: Option<f32>,
    pub overflow: Overflow,
    pub position: Position,
    /// Optional opacity multiplier in [0.0, 1.0]. None represents 1.0 (fully opaque).
    pub opacity: Option<f32>,
    /// Floating behavior (CSS 2.2 §9.5). Exposed for blockification and clearance logic.
    pub float: Float,
    /// Clearance behavior (CSS 2.2 §9.5). Exposed for clearance and BFC decisions.
    pub clear: Clear,
    pub z_index: Option<i32>,
    // Display and box model
    pub display: Display,
    /// Writing mode used for axis mapping in layout algorithms.
    pub writing_mode: WritingMode,
    pub box_sizing: BoxSizing,
    pub margin: Edges,
    pub padding: Edges,
    // Dimensions (px; None means auto/none)
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub min_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    /// Optional height as a percentage fraction (0.0..=1.0) of containing block.
    pub height_percent: Option<f32>,
    /// Optional min-height as a percentage fraction (0.0..=1.0).
    pub min_height_percent: Option<f32>,
    /// Optional max-height as a percentage fraction (0.0..=1.0).
    pub max_height_percent: Option<f32>,
    /// Whether margin-left was specified as 'auto' (used in horizontal width solving).
    pub margin_left_auto: bool,
    /// Whether margin-right was specified as 'auto' (used in horizontal width solving).
    pub margin_right_auto: bool,
    // Positional offsets (px) for positioned layout
    pub top: Option<f32>,
    pub left: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
    /// Optional positional offsets as percentages of the containing block (0.0..=1.0).
    /// When present, these are resolved during layout and take precedence over px fields above.
    pub top_percent: Option<f32>,
    pub left_percent: Option<f32>,
    pub right_percent: Option<f32>,
    pub bottom_percent: Option<f32>,
    // Typography
    pub font_family: Option<String>,
    /// Font weight (100-900, with 400=normal, 700=bold). Defaults to 400.
    pub font_weight: u16,
    // Flexbox
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: Option<f32>,
    pub flex_direction: FlexDirection,
    pub flex_wrap: FlexWrap,
    pub align_items: AlignItems,
    pub justify_content: JustifyContent,
    pub align_content: AlignContent,
    /// Gap between adjacent items along rows (cross axis for column direction), in px.
    pub row_gap: f32,
    /// Gap between adjacent items along columns (main axis for row direction), in px.
    pub column_gap: f32,
    /// Optional row-gap percentage (0.0..=1.0). When present, resolved during layout.
    pub row_gap_percent: Option<f32>,
    /// Optional column-gap percentage (0.0..=1.0). When present, resolved during layout.
    pub column_gap_percent: Option<f32>,
}
