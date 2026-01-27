//! Formula providers for size and offset queries.
//!
//! This crate provides the LayoutProvider which dispatches to appropriate
//! layout modes (block, flex, grid, etc.) based on display property.

pub mod formula_trait;
pub mod provider;

pub use formula_trait::{OffsetFormulaProvider, SizeFormulaProvider};
pub use provider::LayoutProvider;
