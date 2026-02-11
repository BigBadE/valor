//! Opacity compositing and offscreen rendering.
//!
//! This component handles all opacity-related rendering operations including:
//! - Collecting opacity composites from display items
//! - Rendering items to offscreen textures with alpha blending
//! - Two-phase opacity rendering (extract + composite)
//! - Managing offscreen texture lifecycle

#[path = "opacity/mod.rs"]
mod opacity;

pub use opacity::{OpacityCompositor, OpacityExtraction};
