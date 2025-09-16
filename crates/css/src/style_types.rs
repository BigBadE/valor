//! Public rendering-facing types for the css crate.

// Re-export core style model types for consumers of the css crate.
pub use css_core::style_model::{
    AlignItems, BorderStyle, BorderWidths, BoxSizing, ComputedStyle, Display, Edges, FlexDirection,
    FlexWrap, JustifyContent, Overflow, Position, Rgba,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct LayoutRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone)]
pub enum LayoutNodeKind {
    Document,
    Block { tag: String },
    InlineText { text: String },
}
