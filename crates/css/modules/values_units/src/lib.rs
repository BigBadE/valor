//! CSS Values and Units Module Level 3 — Property definition syntax and unit types.
//! Spec: <https://www.w3.org/TR/css-values-3/>

#![forbid(unsafe_code)]

// Per-chapter modules mirroring the spec table of contents.
// Each module documents functions with references to the exact spec section.
pub mod chapter_3_identifiers;
pub mod chapter_4_numbers;
pub mod chapter_5_percentages;
pub mod chapter_6_dimensions;
pub mod chapter_9_colors;

// Re-exports for ergonomic access from other crates.
pub use chapter_3_identifiers::{Ident, parse_ident};
pub use chapter_4_numbers::{Number, parse_number};
pub use chapter_5_percentages::{Percentage, parse_percentage};
pub use chapter_6_dimensions::{Length, LengthUnit, Viewport, compute_length_px, parse_length};
pub use chapter_9_colors::{Color, parse_color};

/// Parse error for Values & Units parsing utilities in this crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// The next token did not match the expected grammar.
    UnexpectedToken,
}
