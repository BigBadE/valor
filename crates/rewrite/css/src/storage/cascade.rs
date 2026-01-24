//! CSS cascade implementation.

use crate::CssValue;
use rewrite_core::{Database, DependencyContext, Input, NodeId, Query};

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

/// A CSS declaration for storage - simpler than Declaration (no property name).
#[derive(Debug, Clone)]
pub struct StyleDeclaration {
    pub value: CssValue,
    pub origin: Origin,
    pub specificity: Specificity,
    pub source_order: u32,
    pub important: bool,
}

impl StyleDeclaration {
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

/// User agent stylesheet declarations.
pub struct UserAgentDeclarationInput;

impl Input for UserAgentDeclarationInput {
    type Key = (NodeId, String);
    type Value = StyleDeclaration;

    fn name() -> &'static str {
        "UserAgentDeclaration"
    }

    fn default_value(_key: &Self::Key) -> Self::Value {
        // No default - if not set, there's no UA declaration
        StyleDeclaration {
            value: CssValue::Keyword(crate::CssKeyword::Initial),
            origin: Origin::UserAgent,
            specificity: Specificity::zero(),
            source_order: 0,
            important: false,
        }
    }
}

/// Author stylesheet declarations (including style attributes at author level).
pub struct AuthorDeclarationInput;

impl Input for AuthorDeclarationInput {
    type Key = (NodeId, String);
    type Value = Vec<StyleDeclaration>;

    fn name() -> &'static str {
        "AuthorDeclaration"
    }

    fn default_value(_key: &Self::Key) -> Self::Value {
        Vec::new()
    }
}

/// Inline style declarations (highest priority).
pub struct InlineDeclarationInput;

impl Input for InlineDeclarationInput {
    type Key = (NodeId, String);
    type Value = StyleDeclaration;

    fn name() -> &'static str {
        "InlineDeclaration"
    }

    fn default_value(_key: &Self::Key) -> Self::Value {
        StyleDeclaration {
            value: CssValue::Keyword(crate::CssKeyword::Initial),
            origin: Origin::Inline,
            specificity: Specificity::inline(),
            source_order: 0,
            important: false,
        }
    }
}

/// Cascaded property query - resolves the winning declaration.
pub struct CascadedPropertyQuery;

impl Query for CascadedPropertyQuery {
    type Key = (NodeId, String);
    type Value = CssValue;

    fn execute(db: &Database, key: Self::Key, ctx: &mut DependencyContext) -> Self::Value {
        let (node, property) = &key;
        let mut candidates = Vec::new();

        // Collect user agent declaration
        if let Some(ua_decl) = db.get_input::<UserAgentDeclarationInput>(&key) {
            // Only include if it's not the default initial value
            if !matches!(ua_decl.value, CssValue::Keyword(crate::CssKeyword::Initial)) {
                candidates.push(ua_decl);
            }
        }

        // Collect author stylesheet declarations by matching CSS rules
        let matched_indices = db.query::<crate::matching::MatchedRulesQuery>(*node, ctx);
        let stylesheets = db
            .get_input::<crate::matching::StyleSheetsInput>(&())
            .unwrap_or_default();

        for &rule_idx in &matched_indices {
            if let Some(rule) = stylesheets.rules.get(rule_idx) {
                // Convert specificity from u32 to Specificity struct
                let ids = (rule.specificity >> 16) & 0xFF;
                let classes = (rule.specificity >> 8) & 0xFF;
                let elements = rule.specificity & 0xFF;

                // Check normal declarations
                if let Some(value) = rule.declarations.get(property) {
                    candidates.push(StyleDeclaration {
                        value: value.clone(),
                        origin: Origin::Author,
                        specificity: Specificity::new(ids, classes, elements),
                        source_order: rule.source_order as u32,
                        important: false,
                    });
                }

                // Check important declarations
                if let Some(value) = rule.important_declarations.get(property) {
                    candidates.push(StyleDeclaration {
                        value: value.clone(),
                        origin: Origin::Author,
                        specificity: Specificity::new(ids, classes, elements),
                        source_order: rule.source_order as u32,
                        important: true,
                    });
                }
            }
        }

        // Collect inline declaration
        if let Some(inline_decl) = db.get_input::<InlineDeclarationInput>(&key) {
            // Only include if it's not the default initial value
            if !matches!(
                inline_decl.value,
                CssValue::Keyword(crate::CssKeyword::Initial)
            ) {
                candidates.push(inline_decl);
            }
        }

        // Find the winning declaration
        let mut winner: Option<StyleDeclaration> = None;
        for candidate in candidates {
            if let Some(ref current_winner) = winner {
                if candidate.wins_over(current_winner) {
                    winner = Some(candidate);
                }
            } else {
                winner = Some(candidate);
            }
        }

        // Return winning value or default
        winner.map(|decl| decl.value).unwrap_or_else(|| {
            // No declarations found - try inheritance or use initial value
            if is_inherited_property(property) {
                // Try to inherit from parent
                if let Some(parent) = db
                    .resolve_relationship(*node, rewrite_core::Relationship::Parent)
                    .first()
                {
                    return db.query::<CascadedPropertyQuery>((*parent, property.clone()), ctx);
                }
            }
            // Use initial value (transparent for most properties)
            CssValue::Keyword(crate::CssKeyword::Initial)
        })
    }
}
