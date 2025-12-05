//! Layout model types used by the orchestrator.
//!
//! These types are minimal representations used for snapshots and public API.
//! The actual layout is computed by `css_core::LayoutEngine`.
//!
//! Coordinates are stored in 1/64px units to preserve sub-pixel precision
//! and match browser layout behavior.

#[derive(Debug, Clone, Copy, Default)]
pub struct LayoutRect {
    /// X coordinate in 1/64px units
    pub x: i32,
    /// Y coordinate in 1/64px units
    pub y: i32,
    /// Width in 1/64px units
    pub width: i32,
    /// Height in 1/64px units
    pub height: i32,
}

#[derive(Debug, Clone)]
pub enum LayoutNodeKind {
    Document,
    Block { tag: String },
    InlineText { text: String },
}
