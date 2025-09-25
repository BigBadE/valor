//! Public rendering-facing types for the css crate.

// Re-export core style model types for consumers of the css crate.
pub use css_orchestrator::style_model::{
    AlignItems, BorderStyle, BorderWidths, BoxSizing, ComputedStyle, Display, Edges, FlexDirection,
    FlexWrap, JustifyContent, Overflow, Position, Rgba,
};

// Re-export core layout types to avoid duplication in this crate.
pub use css_orchestrator::layout_model::{LayoutNodeKind, LayoutRect};
