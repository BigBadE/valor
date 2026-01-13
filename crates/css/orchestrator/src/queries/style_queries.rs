//! Style computation queries.
//!
//! These queries implement the CSS cascade and style computation using
//! the query system for automatic memoization and dependency tracking.

use crate::{selectors, style_model::ComputedStyle, types::Stylesheet};
use js::NodeKey;
use std::collections::HashMap;
use valor_query::{InputQuery, Query, QueryDatabase};

/// Input: The active stylesheet.
pub struct StylesheetInput;

impl InputQuery for StylesheetInput {
    type Key = (); // Singleton - only one stylesheet
    type Value = Stylesheet;

    fn default_value() -> Self::Value {
        Stylesheet::default()
    }

    fn name() -> &'static str {
        "StylesheetInput"
    }
}

/// Query: Rules matching a specific element.
///
/// This performs selector matching and returns all matching rules
/// with their specificity and source order.
pub struct MatchingRulesQuery;

#[derive(Clone, Debug)]
pub struct MatchedRule {
    pub declarations: HashMap<String, String>, // property -> value
    pub specificity: (u32, u32, u32),          // (a, b, c) specificity
    pub source_order: u32,
    pub important: bool,
}

impl Query for MatchingRulesQuery {
    type Key = NodeKey;
    type Value = Vec<MatchedRule>;

    fn execute(db: &QueryDatabase, key: Self::Key) -> Self::Value {
        use super::dom_inputs::*;

        // Get DOM data for selector matching
        let tag = db.input::<DomTagInput>(key);
        let id = db.input::<DomIdInput>(key);
        let classes = db.input::<DomClassesInput>(key);
        let attrs = db.input::<DomAttributesInput>(key);
        let parent = db.input::<DomParentInput>(key);

        // Get stylesheet
        let stylesheet = db.input::<StylesheetInput>(());

        let mut matched = Vec::new();

        // Match each rule in the stylesheet
        for rule in &stylesheet.rules {
            // Parse selector list from the rule prelude
            let selector_list = selectors::parse_selector_list(&rule.prelude);

            for selector in selector_list {
                // Check if this selector matches the node
                if matches_selector_for_node(
                    key,
                    &selector,
                    &tag,
                    &id,
                    &classes,
                    &attrs,
                    (*parent).as_ref(),
                    db,
                ) {
                    // Compute specificity
                    let spec = selectors::compute_specificity(&selector);

                    // Convert declarations to HashMap
                    let mut decls = HashMap::new();
                    for decl in &rule.declarations {
                        decls.insert(decl.name.clone(), decl.value.clone());
                    }

                    matched.push(MatchedRule {
                        declarations: decls,
                        specificity: (spec.0, spec.1, spec.2),
                        source_order: rule.source_order,
                        important: false, // TODO: Parse !important from declarations
                    });
                    break; // Match found for this selector, move to next rule
                }
            }
        }

        matched
    }

    fn name() -> &'static str {
        "MatchingRulesQuery"
    }
}

/// Check if a selector matches a node.
/// This is a simplified recursive version that avoids lifetime issues.
fn matches_selector_for_node(
    _node: NodeKey,
    selector: &selectors::Selector,
    tag: &str,
    id: &Option<String>,
    classes: &[String],
    attrs: &HashMap<String, String>,
    parent: Option<&NodeKey>,
    db: &QueryDatabase,
) -> bool {
    use super::dom_inputs::*;

    if selector.len() == 0 {
        return false;
    }

    // Match rightmost selector part against current node
    let parts: Vec<&selectors::SelectorPart> = (0..selector.len())
        .rev()
        .filter_map(|i| selector.part(i))
        .collect();

    if parts.is_empty() {
        return false;
    }

    // Match first (rightmost) part against this node
    if !matches_simple_selector_node(parts[0].sel(), tag, id, classes, attrs) {
        return false;
    }

    // If only one part, we're done
    if parts.len() == 1 {
        return true;
    }

    // Match remaining parts by walking up ancestor chain
    let mut current_ancestor = parent.copied();
    let mut part_idx = 1;

    while part_idx < parts.len() && current_ancestor.is_some() {
        let anc_key = current_ancestor.unwrap();
        let anc_tag = db.input::<DomTagInput>(anc_key);
        let anc_id = db.input::<DomIdInput>(anc_key);
        let anc_classes = db.input::<DomClassesInput>(anc_key);
        let anc_attrs = db.input::<DomAttributesInput>(anc_key);

        let part = parts[part_idx];

        // Check if this ancestor matches
        if matches_simple_selector_node(part.sel(), &anc_tag, &anc_id, &anc_classes, &anc_attrs) {
            // Match found, advance to next selector part
            part_idx += 1;
            if part_idx >= parts.len() {
                return true;
            }
        }

        // Get previous part's combinator to see if we can continue searching
        let prev_part = parts[part_idx - 1];
        match prev_part.combinator_to_next() {
            Some(selectors::Combinator::Child) => {
                // Child combinator requires direct parent match
                // If we didn't match this ancestor, fail
                if !matches_simple_selector_node(
                    part.sel(),
                    &anc_tag,
                    &anc_id,
                    &anc_classes,
                    &anc_attrs,
                ) {
                    return false;
                }
            }
            Some(selectors::Combinator::Descendant) => {
                // Descendant combinator allows walking further up
                current_ancestor = *db.input::<DomParentInput>(anc_key);
                continue;
            }
            None => break,
        }

        current_ancestor = *db.input::<DomParentInput>(anc_key);
    }

    part_idx >= parts.len()
}

/// Check if a simple selector matches the given node data.
fn matches_simple_selector_node(
    sel: &selectors::SimpleSelector,
    tag: &str,
    id: &Option<String>,
    classes: &[String],
    attrs: &HashMap<String, String>,
) -> bool {
    // Universal selector matches everything
    if sel.is_universal() && sel.pseudo_classes().is_empty() {
        return true;
    }

    // Tag selector
    if let Some(sel_tag) = sel.tag() {
        if !tag.eq_ignore_ascii_case(sel_tag) {
            return false;
        }
    }

    // ID selector
    if let Some(sel_id) = sel.element_id() {
        match id {
            Some(node_id) if node_id.eq_ignore_ascii_case(sel_id) => {}
            _ => return false,
        }
    }

    // Class selectors
    for sel_class in sel.classes() {
        if !classes.iter().any(|c| c.eq_ignore_ascii_case(sel_class)) {
            return false;
        }
    }

    // Attribute selectors
    for (attr_name, attr_value) in sel.attr_equals_list() {
        match attrs.get(attr_name) {
            Some(node_value) if node_value.eq_ignore_ascii_case(attr_value) => {}
            _ => return false,
        }
    }

    // Pseudo-classes (simplified - not fully implemented yet)
    if !sel.pseudo_classes().is_empty() {
        // TODO: Implement pseudo-class matching
        return false;
    }

    true
}

/// Query: Inherited style properties from parent.
pub struct InheritedStyleQuery;

#[derive(Clone, Debug, PartialEq)]
pub struct InheritedStyle {
    pub font_size: f32,
    pub font_family: Option<String>,
    pub font_weight: u16,
    pub color: crate::style_model::Rgba,
    pub line_height: Option<f32>,
    pub text_align: crate::style_model::TextAlign,
}

impl Default for InheritedStyle {
    fn default() -> Self {
        Self {
            font_size: 16.0,
            font_family: None,
            font_weight: 400,
            color: crate::style_model::Rgba {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 255,
            },
            line_height: None,
            text_align: crate::style_model::TextAlign::Left,
        }
    }
}

impl Query for InheritedStyleQuery {
    type Key = NodeKey;
    type Value = InheritedStyle;

    fn execute(db: &QueryDatabase, key: Self::Key) -> Self::Value {
        use super::dom_inputs::DomParentInput;

        // Get parent
        let parent = db.input::<DomParentInput>(key);

        match parent.as_ref() {
            Some(parent_key) if *parent_key != NodeKey::ROOT => {
                // Get parent's computed style
                let parent_style = db.query::<ComputedStyleQuery>(*parent_key);

                InheritedStyle {
                    font_size: parent_style.font_size,
                    font_family: parent_style.font_family.clone(),
                    font_weight: parent_style.font_weight,
                    color: parent_style.color,
                    line_height: parent_style.line_height,
                    text_align: parent_style.text_align,
                }
            }
            _ => InheritedStyle::default(),
        }
    }

    fn name() -> &'static str {
        "InheritedStyleQuery"
    }
}

/// Query: Fully computed style for a node.
pub struct ComputedStyleQuery;

impl Query for ComputedStyleQuery {
    type Key = NodeKey;
    type Value = ComputedStyle;

    fn execute(db: &QueryDatabase, key: Self::Key) -> Self::Value {
        use super::dom_inputs::*;

        // Get parent for inheritance
        let parent = db.input::<DomParentInput>(key);
        let parent_style = match parent.as_ref() {
            Some(parent_key) if *parent_key != NodeKey::ROOT => {
                Some(db.query::<ComputedStyleQuery>(*parent_key))
            }
            _ => None,
        };

        // Get matching rules from stylesheet
        let matched_rules = db.query::<MatchingRulesQuery>(key);

        // Perform cascade - collect all declarations with their metadata
        let mut cascaded_decls: HashMap<String, CascadedDecl> = HashMap::new();

        // Apply user-agent default styles for HTML elements
        let tag = db.input::<DomTagInput>(key);
        if matches!(
            tag.as_str(),
            "div"
                | "body"
                | "html"
                | "section"
                | "article"
                | "nav"
                | "header"
                | "footer"
                | "main"
                | "aside"
        ) {
            let ua_display = CascadedDecl {
                value: "block".to_string(),
                important: false,
                specificity: (0, 0, 0), // Lowest specificity (user-agent)
                source_order: 0,
                inline_boost: false,
            };
            cascaded_decls.insert("display".to_string(), ua_display);
        }

        // Body element gets overflow:hidden by default in many browsers
        if tag.as_str() == "body" {
            let ua_overflow = CascadedDecl {
                value: "hidden".to_string(),
                important: false,
                specificity: (0, 0, 0),
                source_order: 0,
                inline_boost: false,
            };
            cascaded_decls.insert("overflow".to_string(), ua_overflow);
        }

        // All elements default to border-box (modern UA behavior)
        let ua_box_sizing = CascadedDecl {
            value: "border-box".to_string(),
            important: false,
            specificity: (0, 0, 0),
            source_order: 0,
            inline_boost: false,
        };
        cascaded_decls.insert("box-sizing".to_string(), ua_box_sizing);

        // Apply each matched rule
        for matched_rule in matched_rules.iter() {
            for (prop_name, prop_value) in &matched_rule.declarations {
                let entry = CascadedDecl {
                    value: prop_value.clone(),
                    important: matched_rule.important,
                    specificity: matched_rule.specificity,
                    source_order: matched_rule.source_order,
                    inline_boost: false,
                };
                cascade_insert(&mut cascaded_decls, prop_name, entry);
            }
        }

        // Apply inline styles (high specificity boost)
        // Get inline style attribute
        let attrs = db.input::<DomAttributesInput>(key);
        if let Some(inline_style_value) = attrs.get("style") {
            let inline_decls = css_style_attr::parse_style_attribute_into_map(inline_style_value);
            for (prop_name, prop_value) in inline_decls {
                let entry = CascadedDecl {
                    value: prop_value.clone(),
                    important: false,
                    specificity: (1000, 0, 0), // Inline styles have very high specificity
                    source_order: u32::MAX,
                    inline_boost: true,
                };
                cascade_insert(&mut cascaded_decls, &prop_name, entry);
            }
        }

        // Extract final property values
        let mut final_decls = HashMap::new();
        for (name, entry) in cascaded_decls {
            final_decls.insert(name, entry.value);
        }

        // Build computed style from declarations
        crate::style::build_computed_from_inline(&final_decls, parent_style.as_deref())
    }

    fn name() -> &'static str {
        "ComputedStyleQuery"
    }
}

/// A declaration with cascade metadata.
#[derive(Clone, Debug)]
struct CascadedDecl {
    value: String,
    important: bool,
    specificity: (u32, u32, u32),
    source_order: u32,
    inline_boost: bool,
}

/// Insert a declaration into the cascade map if it wins over the existing one.
fn cascade_insert(props: &mut HashMap<String, CascadedDecl>, name: &str, entry: CascadedDecl) {
    let should_insert = props
        .get(name)
        .is_none_or(|prev| cascade_wins(&entry, prev));

    if should_insert {
        props.insert(name.to_owned(), entry);
    }
}

/// Check if candidate declaration wins over previous according to CSS cascade rules.
fn cascade_wins(candidate: &CascadedDecl, previous: &CascadedDecl) -> bool {
    // Step 1: !important flag (important declarations win)
    if candidate.important != previous.important {
        return candidate.important;
    }

    // Step 2: Inline styles beat stylesheet rules (at same importance level)
    if candidate.inline_boost && !previous.inline_boost {
        return true;
    }
    if previous.inline_boost && !candidate.inline_boost {
        return false;
    }

    // Step 3: Specificity
    if candidate.specificity != previous.specificity {
        return candidate.specificity > previous.specificity;
    }

    // Step 4: Source order (tie-breaker)
    candidate.source_order > previous.source_order
}

impl valor_query::ParallelQuery for ComputedStyleQuery {}
impl valor_query::ParallelQuery for InheritedStyleQuery {}
impl valor_query::ParallelQuery for MatchingRulesQuery {}
