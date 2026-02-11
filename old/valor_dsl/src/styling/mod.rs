//! Styling system for Valor DSL
//!
//! Provides multiple approaches to styling:
//! 1. Tailwind-inspired utility classes
//! 2. Component-scoped styles
//! 3. CSS-in-Rust builder API
//! 4. Global theme configuration

pub mod builder;
pub mod scoped;
pub mod theme;
pub mod utilities;

pub use builder::{Style, StyleBuilder};
pub use scoped::ComponentStyles;
pub use theme::{ColorPalette, Theme, ThemeConfig};
pub use utilities::TailwindUtilities;

/// Generate complete CSS from all styling sources
pub fn generate_css(theme: &Theme, component_styles: &[ComponentStyles]) -> String {
    let mut css = String::new();

    // Add global theme styles
    css.push_str(&theme.to_css());
    css.push('\n');

    // Add Tailwind utilities
    css.push_str(&TailwindUtilities::generate(&theme.colors));
    css.push('\n');

    // Add component-scoped styles
    for styles in component_styles {
        css.push_str(&styles.to_css());
        css.push('\n');
    }

    css
}
