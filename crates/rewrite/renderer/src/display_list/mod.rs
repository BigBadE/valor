//! Display list representation.
//!
//! Display lists are GPU-friendly representations of what to render.
//! They are built from layout information and submitted to the GPU.

mod background;
mod border;
mod effects;
mod text;
mod transform;

pub use background::*;
pub use border::*;
pub use effects::*;
pub use text::*;
pub use transform::*;

use rewrite_core::{Color, Corner, Keyword};

/// A display list command.
#[derive(Debug, Clone)]
pub enum DisplayList {
    // ========================================================================
    // Basic shapes
    // ========================================================================
    /// Fill a rectangle with a solid color.
    FillRect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
    },

    /// Fill a rectangle with rounded corners.
    FillRoundedRect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        border_radius: BorderRadius,
        color: Color,
    },

    // ========================================================================
    // Backgrounds
    // ========================================================================
    /// Draw a linear gradient.
    LinearGradient {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        gradient: LinearGradient,
    },

    /// Draw a radial gradient.
    RadialGradient {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        gradient: RadialGradient,
    },

    /// Draw a conic gradient.
    ConicGradient {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        gradient: ConicGradient,
    },

    /// Draw a background image.
    BackgroundImage {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        image: BackgroundImage,
    },

    // ========================================================================
    // Borders
    // ========================================================================
    /// Draw a border with individual edge styles.
    DrawBorder {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        border: Border,
    },

    /// Draw an outline (separate from border).
    DrawOutline {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        outline: Outline,
    },

    // ========================================================================
    // Text
    // ========================================================================
    /// Draw shaped text (with full text shaping).
    DrawText { x: f32, y: f32, text: ShapedText },

    /// Draw text decoration (underline, overline, line-through).
    DrawTextDecoration {
        x: f32,
        y: f32,
        width: f32,
        decoration: TextDecoration,
    },

    /// Draw text selection highlight.
    DrawTextSelection {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
    },

    // ========================================================================
    // Images and media
    // ========================================================================
    /// Draw an image.
    DrawImage {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        image_id: u64,
        image_rendering: Keyword, // Auto, Pixelated, CrispEdges, Smooth
    },

    /// Draw an SVG.
    DrawSvg {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        svg_id: u64,
    },

    /// Draw a video frame.
    DrawVideo {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        video_id: u64,
    },

    /// Draw a canvas (2D or WebGL).
    DrawCanvas {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        canvas_id: u64,
    },

    // ========================================================================
    // Effects and filters
    // ========================================================================
    /// Draw a box shadow.
    DrawBoxShadow {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        shadow: BoxShadow,
    },

    /// Draw a text shadow.
    DrawTextShadow {
        x: f32,
        y: f32,
        text: ShapedText,
        shadow: TextShadow,
    },

    /// Apply a filter effect.
    PushFilter { filter: Filter },

    /// Pop a filter effect.
    PopFilter,

    /// Apply a backdrop filter.
    PushBackdropFilter { filter: Filter },

    /// Pop a backdrop filter.
    PopBackdropFilter,

    /// Apply a blend mode.
    PushBlendMode { mode: Keyword }, // Multiply, Screen, Overlay, etc.

    /// Pop a blend mode.
    PopBlendMode,

    // ========================================================================
    // Clipping and masking
    // ========================================================================
    /// Push a rectangular clip.
    PushClip {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },

    /// Push a rounded rect clip.
    PushRoundedClip {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        border_radius: BorderRadius,
    },

    /// Push an arbitrary path clip.
    PushPathClip { path: ClipPath },

    /// Pop a clip.
    PopClip,

    /// Apply a mask.
    PushMask { mask_id: u64 },

    /// Pop a mask.
    PopMask,

    // ========================================================================
    // Transforms and layers
    // ========================================================================
    /// Push a 2D transform.
    PushTransform { transform: Transform2D },

    /// Push a 3D transform.
    PushTransform3D { transform: Transform3D },

    /// Pop a transform.
    PopTransform,

    /// Apply an opacity layer.
    PushOpacity { opacity: f32 },

    /// Pop an opacity layer.
    PopOpacity,

    /// Push a stacking context.
    PushStackingContext {
        z_index: i32,
        transform_style: Keyword, // Flat, Preserve3d
        mix_blend_mode: Keyword,  // Normal, Multiply, Screen, etc.
    },

    /// Pop a stacking context.
    PopStackingContext,

    // ========================================================================
    // Scrolling
    // ========================================================================
    /// Define a scroll container.
    PushScrollContainer {
        scroll_id: u64,
        content_width: f32,
        content_height: f32,
        viewport_width: f32,
        viewport_height: f32,
        overflow_x: Keyword, // Visible, Hidden, Scroll, Auto, Clip
        overflow_y: Keyword,
    },

    /// Pop a scroll container.
    PopScrollContainer,

    /// Draw a scrollbar.
    DrawScrollbar {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        scrollbar: Scrollbar,
    },

    // ========================================================================
    // Form controls
    // ========================================================================
    /// Draw a form control.
    DrawFormControl {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        control: FormControl,
    },

    // ========================================================================
    // Cursor and selection
    // ========================================================================
    /// Set cursor shape for a region.
    SetCursor {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        cursor: Keyword, // Pointer, Default, TextCursor, Move, etc.
    },

    /// Draw a focus ring.
    DrawFocusRing {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        style: FocusRingStyle,
    },
}

/// Border radius for rounded corners (using Corner from core).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderRadius {
    corners: [f32; 4], // Indexed by Corner enum
}

impl BorderRadius {
    pub const fn uniform(radius: f32) -> Self {
        Self {
            corners: [radius, radius, radius, radius],
        }
    }

    pub const fn zero() -> Self {
        Self::uniform(0.0)
    }

    pub fn new(top_left: f32, top_right: f32, bottom_right: f32, bottom_left: f32) -> Self {
        let mut corners = [0.0; 4];
        corners[Corner::TopLeft as usize] = top_left;
        corners[Corner::TopRight as usize] = top_right;
        corners[Corner::BottomRight as usize] = bottom_right;
        corners[Corner::BottomLeft as usize] = bottom_left;
        Self { corners }
    }

    pub fn get(&self, corner: Corner) -> f32 {
        self.corners[corner as usize]
    }

    pub fn set(&mut self, corner: Corner, radius: f32) {
        self.corners[corner as usize] = radius;
    }
}

/// Focus ring style.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FocusRingStyle {
    pub color: Color,
    pub width: f32,
    pub offset: f32,
    pub style: Keyword, // Solid, Dotted, Dashed, etc.
}
