//! CSS rule representation and categorization.

use std::collections::HashMap;

/// Category of CSS property based on its usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropertyCategory {
    /// Layout properties (width, height, display, position, flex, margin, padding, etc.)
    Layout,

    /// Rendering properties (color, background, border-color, text-decoration, etc.)
    Render,

    /// Special properties that only apply in certain contexts
    /// (e.g., flex-grow only on flex items, grid-column only in grid)
    Special,
}

/// A parsed CSS rule with selector and declarations.
#[derive(Debug, Clone)]
pub struct CssRule {
    /// The selector string (e.g., ".class", "#id", "div > p")
    pub selector: String,

    /// Specificity (a, b, c) where a=IDs, b=classes/attrs/pseudo, c=elements
    pub specificity: (u32, u32, u32),

    /// Source order for cascade resolution
    pub source_order: usize,

    /// All declarations in this rule
    pub declarations: HashMap<String, PropertyValue>,
}

/// A property value with importance flag.
#[derive(Debug, Clone)]
pub struct PropertyValue {
    pub value: String,
    pub important: bool,
}

/// CSS rules categorized by property type.
#[derive(Debug, Clone, Default)]
pub struct CategorizedRules {
    /// Rules containing layout properties
    pub layout: Vec<CssRule>,

    /// Rules containing render properties
    pub render: Vec<CssRule>,

    /// Rules containing special properties
    pub special: Vec<CssRule>,
}

impl CssRule {
    /// Create a new CSS rule.
    pub fn new(
        selector: String,
        specificity: (u32, u32, u32),
        source_order: usize,
        declarations: HashMap<String, PropertyValue>,
    ) -> Self {
        Self {
            selector,
            specificity,
            source_order,
            declarations,
        }
    }

    /// Categorize this rule's properties.
    pub fn categorize(&self) -> PropertyCategory {
        // If ANY property is layout, it's a layout rule
        for property in self.declarations.keys() {
            if is_layout_property(property) {
                return PropertyCategory::Layout;
            }
        }

        // Check for special properties
        for property in self.declarations.keys() {
            if is_special_property(property) {
                return PropertyCategory::Special;
            }
        }

        // Default to render
        PropertyCategory::Render
    }

    /// Split this rule into multiple rules by category.
    pub fn split_by_category(&self) -> CategorizedRules {
        let mut layout_decls = HashMap::new();
        let mut render_decls = HashMap::new();
        let mut special_decls = HashMap::new();

        for (prop, value) in &self.declarations {
            if is_layout_property(prop) {
                layout_decls.insert(prop.clone(), value.clone());
            } else if is_special_property(prop) {
                special_decls.insert(prop.clone(), value.clone());
            } else {
                render_decls.insert(prop.clone(), value.clone());
            }
        }

        let mut result = CategorizedRules::default();

        if !layout_decls.is_empty() {
            result.layout.push(CssRule::new(
                self.selector.clone(),
                self.specificity,
                self.source_order,
                layout_decls,
            ));
        }

        if !render_decls.is_empty() {
            result.render.push(CssRule::new(
                self.selector.clone(),
                self.specificity,
                self.source_order,
                render_decls,
            ));
        }

        if !special_decls.is_empty() {
            result.special.push(CssRule::new(
                self.selector.clone(),
                self.specificity,
                self.source_order,
                special_decls,
            ));
        }

        result
    }
}

impl CategorizedRules {
    /// Merge another set of categorized rules into this one.
    pub fn merge(&mut self, other: CategorizedRules) {
        self.layout.extend(other.layout);
        self.render.extend(other.render);
        self.special.extend(other.special);
    }

    /// Check if there are any rules in any category.
    pub fn is_empty(&self) -> bool {
        self.layout.is_empty() && self.render.is_empty() && self.special.is_empty()
    }
}

/// Check if a property is a layout property.
fn is_layout_property(property: &str) -> bool {
    matches!(
        property,
        "display" | "position" | "float" | "clear"
        | "width" | "height" | "min-width" | "min-height" | "max-width" | "max-height"
        | "margin" | "margin-top" | "margin-right" | "margin-bottom" | "margin-left"
        | "padding" | "padding-top" | "padding-right" | "padding-bottom" | "padding-left"
        | "border-width" | "border-top-width" | "border-right-width" | "border-bottom-width" | "border-left-width"
        | "top" | "right" | "bottom" | "left"
        | "flex-direction" | "flex-wrap" | "justify-content" | "align-items" | "align-content" | "align-self"
        | "flex-grow" | "flex-shrink" | "flex-basis" | "flex"
        | "order"
        | "grid-template-columns" | "grid-template-rows" | "grid-gap" | "gap"
        | "overflow" | "overflow-x" | "overflow-y"
        | "visibility"
        | "box-sizing"
        | "transform" // Affects layout positioning
        | "z-index"
    )
}

/// Check if a property is a special context-dependent property.
fn is_special_property(property: &str) -> bool {
    matches!(
        property,
        "flex-grow" | "flex-shrink" | "flex-basis" // Only apply to flex items
        | "grid-column" | "grid-row" | "grid-area" // Only apply to grid items
        | "align-self" // Context-dependent
        | "order" // Context-dependent
    )
}
