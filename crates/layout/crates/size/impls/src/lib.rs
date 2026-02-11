//! Formula providers for size and offset queries.
//!
//! This crate provides the LayoutProvider which dispatches to appropriate
//! layout modes (block, flex, grid, etc.) based on display property.

pub mod formula_trait;
pub mod provider;

pub use formula_trait::{OffsetFormulaProvider, SizeFormulaProvider};
pub use provider::LayoutProvider;

/// Size mode enumeration - specifies the type of size computation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, rewrite_macros::Markers)]
pub enum SizeMode {
    /// Constrained size (fit within available space)
    Constrained,
    /// Intrinsic minimum size (min-content)
    IntrinsicMin,
    /// Intrinsic maximum size (max-content)
    IntrinsicMax,
}
