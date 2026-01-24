//! Offset mode types.
//!
//! This crate only defines the OffsetMode enum. The actual offset computation
//! logic is in the offset crate, which calls the layout modules directly.

/// Offset mode enumeration - specifies the type of offset computation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, rewrite_macros::Markers)]
pub enum OffsetMode {
    /// Static offset (normal flow positioning)
    Static,
    /// Relative offset (relative positioning)
    Relative,
    /// Absolute offset (absolute positioning)
    Absolute,
}
