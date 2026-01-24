//! Stylesheet storage and rule representation.

use crate::CssValue;
use rewrite_core::Input;
use std::collections::HashMap;

/// A parsed CSS rule with selector string, declarations, and metadata.
#[derive(Debug, Clone)]
pub struct StyleRule {
    /// The selector string (e.g., "div.class", "#id > p")
    pub selector_text: String,

    /// Property declarations (e.g., "width" -> Length(100px))
    pub declarations: HashMap<String, CssValue>,

    /// Properties marked with !important
    pub important_declarations: HashMap<String, CssValue>,

    /// Specificity value for cascading (higher = higher priority)
    /// Calculated as: (id_count << 16) | (class_count << 8) | tag_count
    pub specificity: u32,

    /// Source order index (for tiebreaking when specificity is equal)
    pub source_order: usize,
}

impl StyleRule {
    /// Create a new style rule.
    pub fn new(
        selector_text: String,
        declarations: HashMap<String, CssValue>,
        specificity: u32,
        source_order: usize,
    ) -> Self {
        Self {
            selector_text,
            declarations,
            important_declarations: HashMap::new(),
            specificity,
            source_order,
        }
    }

    /// Create a new style rule with important declarations.
    pub fn with_important(
        selector_text: String,
        declarations: HashMap<String, CssValue>,
        important_declarations: HashMap<String, CssValue>,
        specificity: u32,
        source_order: usize,
    ) -> Self {
        Self {
            selector_text,
            declarations,
            important_declarations,
            specificity,
            source_order,
        }
    }

    /// Check if this rule sets a specific property.
    pub fn has_property(&self, property: &str) -> bool {
        self.declarations.contains_key(property)
    }
}

/// Collection of all stylesheets for a document.
#[derive(Debug, Clone, Default)]
pub struct StyleSheets {
    /// All CSS rules from all stylesheets.
    pub rules: Vec<StyleRule>,
}

impl StyleSheets {
    /// Create an empty stylesheet collection.
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Add a rule to the collection.
    pub fn add_rule(&mut self, rule: StyleRule) {
        self.rules.push(rule);
    }

    /// Add multiple rules.
    pub fn extend_rules(&mut self, rules: impl IntoIterator<Item = StyleRule>) {
        self.rules.extend(rules);
    }
}

/// Input for storing stylesheets in the database.
/// Key is () since there's only one stylesheet collection per document.
pub struct StyleSheetsInput;

impl Input for StyleSheetsInput {
    type Key = ();
    type Value = StyleSheets;

    fn name() -> &'static str {
        "StyleSheetsInput"
    }

    fn default_value(_key: &Self::Key) -> Self::Value {
        StyleSheets::new()
    }
}
