//! CSS cascade implementation.

use crate::CssValue;

/// Origin of a CSS declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origin {
    UserAgent = 0,
    Author = 1,
    Inline = 2,
}

/// Specificity of a CSS selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Specificity {
    pub ids: u32,
    pub classes: u32,
    pub elements: u32,
}

impl Specificity {
    pub const fn new(ids: u32, classes: u32, elements: u32) -> Self {
        Self {
            ids,
            classes,
            elements,
        }
    }

    pub const fn zero() -> Self {
        Self::new(0, 0, 0)
    }

    pub const fn inline() -> Self {
        Self::new(u32::MAX, 0, 0)
    }
}

impl PartialOrd for Specificity {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Specificity {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.ids
            .cmp(&other.ids)
            .then(self.classes.cmp(&other.classes))
            .then(self.elements.cmp(&other.elements))
    }
}

/// A CSS declaration with cascade metadata.
#[derive(Debug, Clone)]
pub struct Declaration {
    pub property: String,
    pub value: CssValue,
    pub origin: Origin,
    pub specificity: Specificity,
    pub source_order: u32,
    pub important: bool,
}

impl Declaration {
    pub fn wins_over(&self, other: &Self) -> bool {
        if self.important != other.important {
            return self.important;
        }
        if self.origin != other.origin {
            return self.origin > other.origin;
        }
        if self.specificity != other.specificity {
            return self.specificity > other.specificity;
        }
        self.source_order > other.source_order
    }
}

/// Cascade engine.
pub struct CascadeEngine {
    next_source_order: u32,
}

impl CascadeEngine {
    pub fn new() -> Self {
        Self {
            next_source_order: 0,
        }
    }

    pub fn next_source_order(&mut self) -> u32 {
        let order = self.next_source_order;
        self.next_source_order += 1;
        order
    }

    pub fn cascade(
        &self,
        declarations: Vec<Declaration>,
    ) -> std::collections::HashMap<String, CssValue> {
        use std::collections::HashMap;

        let mut winning: HashMap<String, Declaration> = HashMap::new();

        for decl in declarations {
            let property = decl.property.clone();
            if let Some(existing) = winning.get(&property) {
                if decl.wins_over(existing) {
                    winning.insert(property, decl);
                }
            } else {
                winning.insert(property, decl);
            }
        }

        winning
            .into_iter()
            .map(|(prop, decl)| (prop, decl.value))
            .collect()
    }
}

impl Default for CascadeEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if property inherits by default.
pub fn is_inherited_property(property: &str) -> bool {
    matches!(
        property,
        "color"
            | "font-family"
            | "font-size"
            | "font-weight"
            | "font-style"
            | "line-height"
            | "text-align"
            | "text-transform"
            | "letter-spacing"
            | "word-spacing"
            | "white-space"
            | "direction"
            | "writing-mode"
    )
}
