//! Text measurement and shaping for the Valor browser engine.
//!
//! This crate provides text measurement capabilities needed by the layout
//! engine to compute the size and position of text nodes.
//!
//! # Architecture
//!
//! - [`font_system`]: Global font database singleton and platform font mapping.
//! - [`font_attrs`]: Convert CSS properties (lightningcss) to cosmic-text `Attrs`.
//! - [`measure`]: Single-line and wrapped text measurement with Chrome-compatible rounding.

pub mod font_attrs;
pub mod font_system;
pub mod measure;
pub mod whitespace;

// Re-export the main public API at crate root.
pub use font_attrs::{DEFAULT_FONT_SIZE_PX, build_attrs};
pub use font_system::{get_font_system, map_font_family};
pub use measure::{
    TextMetrics, WrappedTextMetrics, measure_text, measure_text_width, measure_text_wrapped,
};
pub use whitespace::collapse_whitespace;
