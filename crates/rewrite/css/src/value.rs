pub use crate::keyword::CssKeyword;

/// Comprehensive CSS property values.
#[derive(Debug, Clone, PartialEq)]
pub enum CssValue {
    // Numeric values
    Length(LengthValue),
    Percentage(f32),
    Number(f32),
    Integer(i32),

    // Color
    Color(ColorValue),

    // Keywords
    Keyword(CssKeyword),

    // Functions
    Calc(Box<CalcValue>),
    Var(String, Option<Box<CssValue>>), // CSS variable with optional fallback

    // Complex values
    Transform(Vec<TransformFunction>),
    Filter(Vec<FilterFunction>),
    Shadow(Vec<ShadowValue>),

    // Lists
    List(Vec<CssValue>),

    // URLs
    Url(String),

    // Gradients
    LinearGradient(LinearGradient),
    RadialGradient(RadialGradient),
    ConicGradient(ConicGradient),

    // Custom (for extensions)
    Custom(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LengthValue {
    Px(f32),
    Em(f32),
    Rem(f32),
    Vw(f32),
    Vh(f32),
    Vmin(f32),
    Vmax(f32),
    Percent(f32),
    Ch(f32),
    Ex(f32),
    Cm(f32),
    Mm(f32),
    In(f32),
    Pt(f32),
    Pc(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorValue {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CalcValue {
    Add(Box<CalcValue>, Box<CalcValue>),
    Sub(Box<CalcValue>, Box<CalcValue>),
    Mul(Box<CalcValue>, f32),
    Div(Box<CalcValue>, f32),
    Value(CssValue),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TransformFunction {
    Translate(LengthValue, LengthValue),
    TranslateX(LengthValue),
    TranslateY(LengthValue),
    Scale(f32, f32),
    ScaleX(f32),
    ScaleY(f32),
    Rotate(f32), // degrees
    RotateX(f32),
    RotateY(f32),
    RotateZ(f32),
    Skew(f32, f32),
    SkewX(f32),
    SkewY(f32),
    Matrix(f32, f32, f32, f32, f32, f32),
    Perspective(LengthValue),
}

#[derive(Debug, Clone, PartialEq)]
pub enum FilterFunction {
    Blur(LengthValue),
    Brightness(f32),
    Contrast(f32),
    Grayscale(f32),
    HueRotate(f32),
    Invert(f32),
    Opacity(f32),
    Saturate(f32),
    Sepia(f32),
    DropShadow(LengthValue, LengthValue, LengthValue, ColorValue),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShadowValue {
    pub offset_x: LengthValue,
    pub offset_y: LengthValue,
    pub blur_radius: Option<LengthValue>,
    pub spread_radius: Option<LengthValue>,
    pub color: ColorValue,
    pub inset: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinearGradient {
    pub angle: f32,
    pub stops: Vec<ColorStop>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RadialGradient {
    pub shape: GradientShape,
    pub position: (LengthValue, LengthValue),
    pub stops: Vec<ColorStop>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConicGradient {
    pub angle: f32,
    pub position: (LengthValue, LengthValue),
    pub stops: Vec<ColorStop>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColorStop {
    pub color: ColorValue,
    pub position: Option<LengthValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GradientShape {
    Circle,
    Ellipse,
}
