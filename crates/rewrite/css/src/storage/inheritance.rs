//! CSS property inheritance.

use super::CssPropertyInput;
use crate::CssValue;
use crate::matching::{MatchedRulesQuery, StyleSheetsInput};
use rewrite_core::{Database, DependencyContext, NodeId, Query, Relationship};

/// Query for CSS property values with inheritance support.
///
/// This query applies the CSS cascade:
/// 1. Check inline styles (highest priority)
/// 2. Check matched stylesheet rules (by specificity)
/// 3. Check inheritance (if property is inherited)
/// 4. Fall back to initial value
pub struct InheritedCssPropertyQuery;

impl Query for InheritedCssPropertyQuery {
    type Key = (NodeId, String);
    type Value = CssValue;

    fn execute(db: &Database, key: Self::Key, ctx: &mut DependencyContext) -> Self::Value {
        let (node, property) = key;

        // 1. Check inline styles (highest priority)
        if let Some(value) = db.get_input::<CssPropertyInput>(&(node, property.clone())) {
            return value;
        }

        // 2. Check matched stylesheet rules (cascade by specificity)
        let matched_indices = db.query::<MatchedRulesQuery>(node, ctx);
        let stylesheets = db.get_input::<StyleSheetsInput>(&()).unwrap_or_default();

        // Iterate in reverse (highest specificity last)
        for &rule_idx in matched_indices.iter().rev() {
            if let Some(rule) = stylesheets.rules.get(rule_idx) {
                if let Some(value) = rule.declarations.get(&property) {
                    return value.clone();
                }
            }
        }

        // 3. Check if this property is inherited
        if is_inherited_property(&property) {
            // Query parent node
            let parents = db.resolve_relationship(node, Relationship::Parent);
            if let Some(&parent) = parents.first() {
                // Recursively query parent
                return db.query::<InheritedCssPropertyQuery>((parent, property), ctx);
            }
        }

        // 4. Fall back to initial value
        super::store::get_initial_value(&property)
    }
}

/// Check if a CSS property is inherited.
fn is_inherited_property(property: &str) -> bool {
    matches!(
        property,
        "color"
            | "font-family"
            | "font-size"
            | "font-style"
            | "font-variant"
            | "font-weight"
            | "line-height"
            | "letter-spacing"
            | "text-align"
            | "text-indent"
            | "text-transform"
            | "visibility"
            | "white-space"
            | "word-spacing"
            | "cursor"
            | "direction"
            | "quotes"
    )
}
