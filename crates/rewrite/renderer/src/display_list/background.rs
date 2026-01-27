//! Background rendering types.

use rewrite_core::{ColorStop, GradientShape, Keyword};

pub use rewrite_core::{ColorStop as GradientColorStop, Gradient as CssGradient};

/// Linear gradient definition.
#[derive(Debug, Clone, PartialEq)]
pub struct LinearGradient {
    /// Angle in degrees (0 = up, 90 = right, 180 = down, 270 = left).
    pub angle: f32,
    /// Color stops.
    pub stops: Vec<ColorStop>,
}

/// Radial gradient definition.
#[derive(Debug, Clone, PartialEq)]
pub struct RadialGradient {
    /// Center X position (0.0 = left, 1.0 = right).
    pub center_x: f32,
    /// Center Y position (0.0 = top, 1.0 = bottom).
    pub center_y: f32,
    /// Radius in pixels.
    pub radius: f32,
    /// Shape (Circle or Ellipse from core).
    pub shape: GradientShape,
    /// Color stops.
    pub stops: Vec<ColorStop>,
}

/// Conic gradient definition.
#[derive(Debug, Clone, PartialEq)]
pub struct ConicGradient {
    /// Center X position (0.0 = left, 1.0 = right).
    pub center_x: f32,
    /// Center Y position (0.0 = top, 1.0 = bottom).
    pub center_y: f32,
    /// Starting angle in degrees.
    pub angle: f32,
    /// Color stops.
    pub stops: Vec<ColorStop>,
}

/// Background image with positioning and sizing.
#[derive(Debug, Clone, PartialEq)]
pub struct BackgroundImage {
    /// Image resource ID.
    pub image_id: u64,
    /// Position X.
    pub position_x: BackgroundPosition,
    /// Position Y.
    pub position_y: BackgroundPosition,
    /// Size.
    pub size: BackgroundSize,
    /// Repeat mode (Repeat, RepeatAxis(X/Y), NoRepeat, Round, Space).
    pub repeat: Keyword,
    /// Attachment (Scroll, BackgroundFixed, Local).
    pub attachment: Keyword,
    /// Clip area (BorderBox, PaddingBox, ContentBox).
    pub clip: Keyword,
    /// Origin area (BorderBox, PaddingBox, ContentBox).
    pub origin: Keyword,
}

/// Background position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BackgroundPosition {
    /// Percentage (0.0 = left/top, 1.0 = right/bottom).
    Percent(f32),
    /// Pixels from left/top.
    Pixels(f32),
    /// Keyword (Center, etc.).
    Keyword(Keyword),
}

/// Background size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BackgroundSize {
    /// Auto (intrinsic size).
    Auto,
    /// Cover (scale to cover).
    Cover,
    /// Contain (scale to fit).
    Contain,
    /// Explicit width and height.
    Explicit { width: f32, height: f32 },
}
