//! CSS parsing and rule management.
//!
//! This crate provides streaming CSS parsing with automatic categorization
//! of rules into layout, render, and special categories.

mod keyword;
mod parser;
mod rule;
mod value;

pub use keyword::*;
pub use parser::{CssUpdate, StreamingCssParser};
pub use rule::{CategorizedRules, CssRule, PropertyCategory, PropertyValue};
pub use value::*;

/// Subpixel precision type (64ths of a pixel).
pub type Subpixels = i32;

/// Viewport size information.
#[derive(Debug, Clone, Copy)]
pub struct ViewportSize {
    pub width: f32,
    pub height: f32,
}

impl Default for ViewportSize {
    fn default() -> Self {
        Self {
            width: 1024.0,
            height: 768.0,
        }
    }
}

/// Viewport input for queries.
#[derive(Debug, Clone, Copy)]
pub struct ViewportInput;
