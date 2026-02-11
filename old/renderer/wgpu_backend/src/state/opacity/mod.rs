//! Opacity compositing and offscreen rendering module.
//!
//! This module handles all opacity-related rendering operations including:
//! - Collecting opacity composites from display items
//! - Rendering items to offscreen textures with alpha blending
//! - Two-phase opacity rendering (extract + composite)
//! - Managing offscreen texture lifecycle

mod batching;
mod core;
mod offscreen;
mod texture;

pub use core::{OpacityCompositor, OpacityExtraction};
