//! Visual effects (shadows, filters, blend modes).

use rewrite_core::{Axis, Color};

use super::BorderRadius;

/// Box shadow.
#[derive(Debug, Clone, PartialEq)]
pub struct BoxShadow {
    /// Horizontal offset.
    pub offset_x: f32,
    /// Vertical offset.
    pub offset_y: f32,
    /// Blur radius.
    pub blur_radius: f32,
    /// Spread radius.
    pub spread_radius: f32,
    /// Shadow color.
    pub color: Color,
    /// Inset shadow (inside the box).
    pub inset: bool,
    /// Border radius (if the box has rounded corners).
    pub border_radius: BorderRadius,
}

/// Filter effect (re-export from core).
pub use rewrite_core::Filter;

/// Clip path.
#[derive(Debug, Clone, PartialEq)]
pub enum ClipPath {
    /// Circle clip.
    Circle {
        center_x: f32,
        center_y: f32,
        radius: f32,
    },

    /// Ellipse clip.
    Ellipse {
        center_x: f32,
        center_y: f32,
        radius_x: f32,
        radius_y: f32,
    },

    /// Polygon clip.
    Polygon { points: Vec<(f32, f32)> },

    /// Arbitrary SVG path.
    SvgPath { path_data: String },

    /// Reference to SVG clipPath element.
    SvgClipPath { clip_path_id: u64 },
}

/// Scrollbar appearance.
#[derive(Debug, Clone, PartialEq)]
pub struct Scrollbar {
    /// Scrollbar orientation (using Axis: Horizontal/Vertical).
    pub orientation: Axis,
    /// Track color.
    pub track_color: Color,
    /// Thumb color.
    pub thumb_color: Color,
    /// Thumb position (0.0 = start, 1.0 = end).
    pub thumb_position: f32,
    /// Thumb size (0.0 to 1.0).
    pub thumb_size: f32,
}

/// Form control types.
#[derive(Debug, Clone, PartialEq)]
pub enum FormControl {
    /// Button.
    Button {
        label: String,
        pressed: bool,
        disabled: bool,
    },

    /// Text input.
    TextInput {
        value: String,
        placeholder: String,
        focused: bool,
        disabled: bool,
        selection_start: usize,
        selection_end: usize,
    },

    /// Checkbox.
    Checkbox { checked: bool, disabled: bool },

    /// Radio button.
    Radio { checked: bool, disabled: bool },

    /// Select dropdown.
    Select {
        options: Vec<String>,
        selected_index: usize,
        disabled: bool,
    },

    /// Textarea.
    Textarea {
        value: String,
        placeholder: String,
        focused: bool,
        disabled: bool,
        selection_start: usize,
        selection_end: usize,
    },

    /// Range slider.
    Range {
        value: f32,
        min: f32,
        max: f32,
        disabled: bool,
    },

    /// Color picker.
    ColorPicker { color: Color, disabled: bool },

    /// File input.
    FileInput { files: Vec<String>, disabled: bool },
}
