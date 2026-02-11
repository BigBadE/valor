//! Component-scoped styling (CSS Modules pattern)

use std::collections::HashMap;

/// Component-scoped styles
#[derive(Clone, Debug, Default)]
pub struct ComponentStyles {
    pub component_name: String,
    pub rules: HashMap<String, HashMap<String, String>>,
}

impl ComponentStyles {
    /// Create new component styles
    pub fn new(component_name: impl Into<String>) -> Self {
        Self {
            component_name: component_name.into(),
            rules: HashMap::new(),
        }
    }

    /// Add a style rule for a selector
    pub fn rule(
        mut self,
        selector: impl Into<String>,
        properties: HashMap<String, String>,
    ) -> Self {
        self.rules.insert(selector.into(), properties);
        self
    }

    /// Add a single property to a selector
    pub fn add(
        mut self,
        selector: impl Into<String>,
        property: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        let selector = selector.into();
        self.rules
            .entry(selector)
            .or_default()
            .insert(property.into(), value.into());
        self
    }

    /// Convert to scoped CSS
    pub fn to_css(&self) -> String {
        let mut css = format!("/* Scoped styles for {} */\n", self.component_name);

        for (selector, properties) in &self.rules {
            // Scope the selector to this component
            let scoped_selector = if selector.starts_with('.') || selector.starts_with('#') {
                // Class or ID selector - just use as-is for now
                // In a real implementation, we'd hash the component name and append
                selector.clone()
            } else {
                // Element selector - scope it
                format!(".{} {}", self.component_name, selector)
            };

            css.push_str(&format!("{} {{\n", scoped_selector));
            for (prop, val) in properties {
                css.push_str(&format!("    {}: {};\n", prop, val));
            }
            css.push_str("}\n\n");
        }

        css
    }
}

/// Macro to define component styles
#[macro_export]
macro_rules! component_styles {
    ($comp_name:expr, { $($selector:expr => { $($prop:ident: $val:expr),* $(,)? }),* $(,)? }) => {
        {
            let mut styles = $crate::styling::ComponentStyles::new($comp_name);
            $(
                let mut props = std::collections::HashMap::new();
                $(
                    props.insert(stringify!($prop).replace('_', "-"), $val.to_string());
                )*
                styles = styles.rule($selector, props);
            )*
            styles
        }
    };
}
