//! CSS Value definitions.
//!
//! This module defines all CSS value types that properties can have.
//! Similar values are unified with enums to minimize redundancy.

use crate::{Axis, Axis3D, Edge, Position};

// ============================================================================
// Core Value Types
// ============================================================================

/// Main CSS value enum.
#[derive(Debug, Clone, PartialEq)]
pub enum CssValue {
    // Numeric
    Length(Length),
    Percentage(f32),
    Number(f32),
    Integer(i32),

    // Color
    Color(Color),

    // Keywords
    Keyword(Keyword),

    // Functions
    Calc(Box<Calc>),
    Var(String, Option<Box<CssValue>>), // CSS variable with optional fallback

    // Complex values
    Transform(Vec<Transform>),
    Filter(Vec<Filter>),
    Shadow(Vec<Shadow>),
    Gradient(Gradient),
    TimingFunction(TimingFunction),

    // Lists
    List(Vec<CssValue>),

    // URLs and Strings
    Url(String),
    String(String),
}

// ============================================================================
// Length Units
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    // Absolute units
    Px(f32),
    Pt(f32),
    Pc(f32),
    In(f32),
    Cm(f32),
    Mm(f32),
    Q(f32),

    // Font-relative units
    Em(f32),
    Rem(f32),
    Ex(f32),
    Ch(f32),
    Cap(f32),
    Ic(f32),
    Lh(f32),
    Rlh(f32),

    // Viewport-relative units
    Vw(f32),
    Vh(f32),
    Vmin(f32),
    Vmax(f32),
    Vi(f32),
    Vb(f32),

    // Container-relative units
    Cqw(f32),
    Cqh(f32),
    Cqi(f32),
    Cqb(f32),
    Cqmin(f32),
    Cqmax(f32),
}

// ============================================================================
// Color
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

// ============================================================================
// Keywords (collated by removing duplicates and grouping related values)
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Keyword {
    // ========================================================================
    // Global keywords
    // ========================================================================
    Initial,
    Inherit,
    Unset,
    Revert,
    RevertLayer,

    // ========================================================================
    // Common keywords
    // ========================================================================
    Auto,
    None,
    Normal,

    // ========================================================================
    // Display values
    // ========================================================================
    Block,
    Inline,
    InlineBlock,
    Flex,
    InlineFlex,
    Grid,
    InlineGrid,
    Table,
    TableRow,
    TableRowGroup,
    TableHeaderGroup,
    TableFooterGroup,
    TableColumn,
    TableColumnGroup,
    TableCell,
    TableCaption,
    ListItem,
    Contents,
    FlowRoot,
    Ruby,
    RubyBase,
    RubyText,

    // ========================================================================
    // Position values
    // ========================================================================
    Static,
    Relative,
    Absolute,
    PositionFixed,
    Sticky,

    // ========================================================================
    // Non-directional alignment
    // ========================================================================
    Middle,
    Center,
    Both,

    // ========================================================================
    // Visibility/Display
    // ========================================================================
    Visible,
    Hidden,
    Scroll,
    Clip,
    Collapse,

    // ========================================================================
    // Box model
    // ========================================================================
    ContentBox,
    BorderBox,
    PaddingBox,

    // ========================================================================
    // Flex/Grid
    // ========================================================================
    Direction(Axis), // Row, Column (for flex-direction)
    Reverse(Axis),   // RowReverse, ColumnReverse
    Nowrap,
    Wrap,
    WrapReverse,
    FlexStart,
    FlexEnd,
    SelfStart,
    SelfEnd,
    Stretch,
    Baseline,
    FirstBaseline,
    LastBaseline,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    Space,

    // ========================================================================
    // Text alignment/transform
    // ========================================================================
    Justify,
    JustifyAll,
    MatchParent,
    Uppercase,
    Lowercase,
    Capitalize,
    FullSize,
    FullSizeKana,

    // ========================================================================
    // White space/word breaking
    // ========================================================================
    Pre,
    PreWrap,
    PreLine,
    BreakAll,
    KeepAll,
    BreakWord,

    // ========================================================================
    // Text decoration (collated line styles)
    // ========================================================================
    Underline,
    Overline,
    LineThrough,
    Blink,

    // ========================================================================
    // Line styles (used by border, outline, text-decoration)
    // ========================================================================
    Solid,
    Double,
    Dotted,
    Dashed,
    Wavy,
    Groove,
    Ridge,
    Inset,
    Outset,

    // ========================================================================
    // Font properties (collated weights/styles/variants)
    // ========================================================================
    // Weight
    Thin,
    ExtraLight,
    Light,
    Regular,
    Medium,
    SemiBold,
    Bold,
    ExtraBold,
    Black,
    Lighter,
    Bolder,

    // Style
    Italic,
    Oblique,

    // Variant
    SmallCaps,
    AllSmallCaps,
    PetiteCaps,
    AllPetiteCaps,
    Unicase,
    TitlingCaps,

    // Stretch
    UltraCondensed,
    ExtraCondensed,
    Condensed,
    SemiCondensed,
    SemiExpanded,
    Expanded,
    ExtraExpanded,
    UltraExpanded,

    // ========================================================================
    // Vertical alignment
    // ========================================================================
    TextEdge(Edge), // TextTop, TextBottom
    Sub,
    Super,

    // ========================================================================
    // Cursor values
    // ========================================================================
    Pointer,
    Default,
    TextCursor,
    Move,
    NotAllowed,
    Grab,
    Grabbing,
    Crosshair,
    Help,
    Wait,
    Progress,
    Copy,
    Alias,
    ContextMenu,
    Cell,
    NoDrop,
    AxisResize(Axis), // ColResize, RowResize unified
    AllScroll,
    ZoomIn,
    ZoomOut,

    // ========================================================================
    // Background properties
    // ========================================================================
    Cover,
    Contain,
    Repeat,
    RepeatAxis(Axis), // RepeatX, RepeatY
    NoRepeat,
    Round,
    BackgroundFixed,
    Local,

    // ========================================================================
    // List style
    // ========================================================================
    Disc,
    Circle,
    Square,
    Decimal,
    DecimalLeadingZero,
    LowerRoman,
    UpperRoman,
    LowerGreek,
    LowerLatin,
    UpperLatin,
    LowerAlpha,
    UpperAlpha,
    Inside,
    Outside,

    // ========================================================================
    // Table
    // ========================================================================
    Separate,
    Show,
    Hide,

    // ========================================================================
    // Object fit
    // ========================================================================
    Fill,
    ScaleDown,

    // ========================================================================
    // Blend modes
    // ========================================================================
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,

    // ========================================================================
    // Writing modes/direction
    // ========================================================================
    HorizontalTb,
    VerticalRl,
    VerticalLr,
    SidewaysRl,
    SidewaysLr,
    Ltr,
    Rtl,

    // Unicode bidi
    Embed,
    BidiOverride,
    Isolate,
    IsolateOverride,
    Plaintext,

    // ========================================================================
    // Sizing keywords
    // ========================================================================
    MinContent,
    MaxContent,
    FitContent,
    Min,
    Max,

    // ========================================================================
    // Color keywords
    // ========================================================================
    Transparent,
    CurrentColor,

    // ========================================================================
    // Interactive
    // ========================================================================
    All,
    VisiblePainted,
    VisibleFill,
    VisibleStroke,
    Painted,

    // ========================================================================
    // Transform
    // ========================================================================
    Flat,
    Preserve3d,

    // ========================================================================
    // Animation/Transition
    // ========================================================================
    Forwards,
    Backwards,
    Alternate,
    AlternateReverse,
    Infinite,
    Running,
    Paused,
    Ease,
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,

    // ========================================================================
    // Grid
    // ========================================================================
    Dense,
    MasonryAuto,

    // ========================================================================
    // Scroll
    // ========================================================================
    Smooth,
    Mandatory,
    Proximity,

    // ========================================================================
    // Contain
    // ========================================================================
    Size,
    Layout,
    Style,
    Paint,
    Strict,
    Content,

    // ========================================================================
    // Other
    // ========================================================================
    Antialiased,
    SubpixelAntialiased,
}

// ============================================================================
// Calc
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum Calc {
    Add(Box<Calc>, Box<Calc>),
    Sub(Box<Calc>, Box<Calc>),
    Mul(Box<Calc>, Box<Calc>),
    Div(Box<Calc>, Box<Calc>),
    Min(Vec<Calc>),
    Max(Vec<Calc>),
    Clamp(Box<Calc>, Box<Calc>, Box<Calc>), // min, val, max
    Value(Box<CssValue>),
}

// ============================================================================
// Transform Functions (collated using Axis and Axis3D)
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum Transform {
    Translate(Axis, Length), // TranslateX, TranslateY unified
    Translate2D(Length, Length),
    Translate3D(Length, Length, Length),

    Scale(Axis, f32), // ScaleX, ScaleY unified
    Scale2D(f32, f32),
    Scale3D(f32, f32, f32),

    Rotate(f32),                  // degrees
    Rotate3D(f32, f32, f32, f32), // x, y, z, angle
    RotateAxis(Axis3D, f32),      // RotateX, RotateY, RotateZ unified

    Skew(Axis, f32), // SkewX, SkewY unified
    Skew2D(f32, f32),

    Matrix(f32, f32, f32, f32, f32, f32),
    Matrix3D([f32; 16]),

    Perspective(Length),
}

// ============================================================================
// Filter Functions
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum Filter {
    Blur(Length),
    Brightness(f32),
    Contrast(f32),
    Grayscale(f32),
    HueRotate(f32),
    Invert(f32),
    Opacity(f32),
    Saturate(f32),
    Sepia(f32),
    DropShadow(Shadow),
}

// ============================================================================
// Shadow
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub struct Shadow {
    pub offset: (Length, Length),
    pub blur: Option<Length>,
    pub spread: Option<Length>,
    pub color: Color,
    pub inset: bool,
}

// ============================================================================
// Gradients
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum Gradient {
    Linear {
        angle: GradientAngle,
        stops: Vec<ColorStop>,
    },
    Radial {
        shape: GradientShape,
        position: (Length, Length),
        stops: Vec<ColorStop>,
    },
    Conic {
        angle: f32,
        position: (Length, Length),
        stops: Vec<ColorStop>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GradientAngle {
    Degrees(f32),
    ToEdge(Edge),
    ToCorner(Edge, Edge),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradientShape {
    Circle,
    Ellipse,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColorStop {
    pub color: Color,
    pub position: Option<Length>,
}

// ============================================================================
// Timing Functions
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimingFunction {
    Keyword(Keyword), // Ease, Linear, EaseIn, EaseOut, EaseInOut, StepStart, StepEnd
    CubicBezier(f32, f32, f32, f32),
    Steps(i32, Position), // Start, End from shared Position enum
}
