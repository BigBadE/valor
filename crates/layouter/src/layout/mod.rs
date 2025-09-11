//! Layout module root: submodules and public re-exports.

mod args;
mod styles;
mod geometry;
mod inline;
mod block;
mod compute;
mod flex;
mod text;

// Phase 2 scaffolding modules
pub mod boxes;
pub mod fragments;

pub use geometry::LayoutRect;
pub use compute::{compute_simple_layout, compute_layout_geometry};
pub use text::{collapse_whitespace, reorder_bidi_for_display};
