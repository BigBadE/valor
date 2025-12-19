//! CSS-in-Rust builder API (styled-components pattern)

use std::collections::HashMap;
use std::fmt;

/// A style builder for creating inline styles
#[derive(Clone, Debug, Default)]
pub struct Style {
    properties: HashMap<String, String>,
    hover_properties: HashMap<String, String>,
    active_properties: HashMap<String, String>,
}

impl Style {
    /// Create a new empty style
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a CSS property
    pub fn set(mut self, property: impl Into<String>, value: impl Into<String>) -> Self {
        self.properties.insert(property.into(), value.into());
        self
    }

    // Common properties with type-safe methods

    pub fn padding(self, value: impl Into<String>) -> Self {
        self.set("padding", value)
    }

    pub fn margin(self, value: impl Into<String>) -> Self {
        self.set("margin", value)
    }

    pub fn background(self, value: impl Into<String>) -> Self {
        self.set("background", value)
    }

    pub fn color(self, value: impl Into<String>) -> Self {
        self.set("color", value)
    }

    pub fn font_size(self, value: impl Into<String>) -> Self {
        self.set("font-size", value)
    }

    pub fn font_weight(self, value: impl Into<String>) -> Self {
        self.set("font-weight", value)
    }

    pub fn font_family(self, value: impl Into<String>) -> Self {
        self.set("font-family", value)
    }

    pub fn border(self, value: impl Into<String>) -> Self {
        self.set("border", value)
    }

    pub fn border_radius(self, value: impl Into<String>) -> Self {
        self.set("border-radius", value)
    }

    pub fn width(self, value: impl Into<String>) -> Self {
        self.set("width", value)
    }

    pub fn height(self, value: impl Into<String>) -> Self {
        self.set("height", value)
    }

    pub fn display(self, value: impl Into<String>) -> Self {
        self.set("display", value)
    }

    pub fn flex_direction(self, value: impl Into<String>) -> Self {
        self.set("flex-direction", value)
    }

    pub fn align_items(self, value: impl Into<String>) -> Self {
        self.set("align-items", value)
    }

    pub fn justify_content(self, value: impl Into<String>) -> Self {
        self.set("justify-content", value)
    }

    pub fn gap(self, value: impl Into<String>) -> Self {
        self.set("gap", value)
    }

    pub fn box_shadow(self, value: impl Into<String>) -> Self {
        self.set("box-shadow", value)
    }

    pub fn text_shadow(self, value: impl Into<String>) -> Self {
        self.set("text-shadow", value)
    }

    pub fn transform(self, value: impl Into<String>) -> Self {
        self.set("transform", value)
    }

    pub fn transition(self, value: impl Into<String>) -> Self {
        self.set("transition", value)
    }

    pub fn opacity(self, value: impl Into<String>) -> Self {
        self.set("opacity", value)
    }

    pub fn cursor(self, value: impl Into<String>) -> Self {
        self.set("cursor", value)
    }

    pub fn text_align(self, value: impl Into<String>) -> Self {
        self.set("text-align", value)
    }

    /// Add hover styles
    pub fn hover<F>(mut self, f: F) -> Self
    where
        F: FnOnce(Style) -> Style,
    {
        let hover_style = f(Style::new());
        self.hover_properties = hover_style.properties;
        self
    }

    /// Add active styles
    pub fn active<F>(mut self, f: F) -> Self
    where
        F: FnOnce(Style) -> Style,
    {
        let active_style = f(Style::new());
        self.active_properties = active_style.properties;
        self
    }

    /// Convert to inline style attribute string
    pub fn to_inline(&self) -> String {
        self.properties
            .iter()
            .map(|(k, v)| format!("{}: {};", k, v))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Convert to a CSS class definition
    pub fn to_class(&self, class_name: &str) -> String {
        let mut css = format!(".{} {{\n", class_name);

        for (prop, val) in &self.properties {
            css.push_str(&format!("    {}: {};\n", prop, val));
        }

        css.push_str("}\n");

        // Add hover styles
        if !self.hover_properties.is_empty() {
            css.push_str(&format!(".{}:hover {{\n", class_name));
            for (prop, val) in &self.hover_properties {
                css.push_str(&format!("    {}: {};\n", prop, val));
            }
            css.push_str("}\n");
        }

        // Add active styles
        if !self.active_properties.is_empty() {
            css.push_str(&format!(".{}:active {{\n", class_name));
            for (prop, val) in &self.active_properties {
                css.push_str(&format!("    {}: {};\n", prop, val));
            }
            css.push_str("}\n");
        }

        css
    }
}

impl fmt::Display for Style {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_inline())
    }
}

/// Type alias for the builder pattern
pub type StyleBuilder = Style;

/// Macro to create styles more concisely
#[macro_export]
macro_rules! style {
    ($($prop:ident: $val:expr),* $(,)?) => {
        {
            let mut s = $crate::styling::Style::new();
            $(
                s = s.$prop($val);
            )*
            s
        }
    };
}
