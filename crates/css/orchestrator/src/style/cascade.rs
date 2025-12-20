//! CSS cascade resolution and selector matching.
//!
//! Implements the CSS cascade algorithm for determining which declarations apply to each
//! element, considering origin, specificity, source order, and importance.

use std::collections::{HashMap, HashSet};

use crate::{selectors, selectors::Specificity, style_model, types};
use css_style_attr::parse_style_attribute_into_map;
use css_variables::{CustomProperties, extract_custom_properties};
use js::{DOMUpdate, NodeKey};

use super::{build_computed_from_inline, ua_stylesheet};

/// A declaration tracked during cascading with metadata used to resolve conflicts.
/// Cascaded declaration augmented with metadata for conflict resolution.
#[derive(Clone, Debug)]
struct CascadedDecl {
    /// Property value as authored (after var substitution).
    value: String,
    /// Whether the declaration was marked `!important`.
    important: bool,
    /// Rule origin (UA, user, author).
    origin: types::Origin,
    /// Winning selector specificity for the rule.
    specificity: selectors::Specificity,
    /// Rule source order used as a tie-breaker.
    source_order: u32,
    /// Inline style boost flag.
    inline_boost: bool,
}

/// Return a small integral weight for origin precedence comparisons.
const fn origin_weight(origin: types::Origin) -> u8 {
    match origin {
        types::Origin::UserAgent => 0,
        types::Origin::User => 1,
        types::Origin::Author => 2,
    }
}

/// Return true if `candidate` wins over `previous` according to CSS cascade rules.
fn wins_over(candidate: &CascadedDecl, previous: &CascadedDecl) -> bool {
    if candidate.inline_boost && !previous.inline_boost {
        return true;
    }
    if previous.inline_boost && !candidate.inline_boost {
        return false;
    }
    if candidate.important != previous.important {
        return candidate.important;
    }
    let ow_c = origin_weight(candidate.origin);
    let ow_p = origin_weight(previous.origin);
    if ow_c != ow_p {
        return ow_c > ow_p;
    }
    if candidate.specificity != previous.specificity {
        return candidate.specificity > previous.specificity;
    }
    if candidate.source_order != previous.source_order {
        return candidate.source_order > previous.source_order;
    }
    false
}

/// Insert a cascaded declaration into the property map if it wins over any existing one.
fn cascade_put(props: &mut HashMap<String, CascadedDecl>, name: &str, entry: CascadedDecl) {
    let should_insert = props
        .get(name)
        .is_none_or(|previous| wins_over(&entry, previous));
    if should_insert {
        props.insert(name.to_owned(), entry);
    }
}

/// Check if `node` matches a simple selector (tag/id/classes).
fn matches_simple_selector(
    node: NodeKey,
    sel: &selectors::SimpleSelector,
    style_comp: &StyleComputer,
) -> bool {
    if sel.is_universal() {
        return true;
    }
    if let Some(tag) = sel.tag() {
        let tag_name = style_comp.tag_by_node.get(&node);
        if !tag_name.is_some_and(|value| value.eq_ignore_ascii_case(tag)) {
            return false;
        }
    }
    if let Some(element_id) = sel.element_id() {
        let element_id_name = style_comp.id_by_node.get(&node);
        if !element_id_name.is_some_and(|value| value.eq_ignore_ascii_case(element_id)) {
            return false;
        }
    }
    for class in sel.classes() {
        if !node_has_class(&style_comp.classes_by_node, node, class) {
            return false;
        }
    }
    for (attr_name, attr_value) in sel.attr_equals_list() {
        let node_attr_value: Option<&String> = style_comp
            .attrs_by_node
            .get(&node)
            .and_then(|attrs| attrs.get(attr_name));
        if !node_attr_value.is_some_and(|value: &String| value.eq_ignore_ascii_case(attr_value)) {
            return false;
        }
    }
    true
}

/// Check whether a node matches the given parsed selector using ancestor traversal.
fn matches_selector(
    start_node: NodeKey,
    selector: &selectors::Selector,
    style_computer: &StyleComputer,
) -> bool {
    if selector.len() == 0 {
        return false;
    }
    let mut reversed = (0..selector.len()).rev().peekable();
    let mut current_node = start_node;
    loop {
        let Some(index) = reversed.next() else {
            return true;
        };
        let Some(part) = selector.part(index) else {
            return false;
        };
        if !matches_simple_selector(current_node, part.sel(), style_computer) {
            return false;
        }
        if reversed.peek().is_none() {
            return true;
        }
        let Some(prev_index) = reversed.peek().copied() else {
            return false;
        };
        let Some(prev_part) = selector.part(prev_index) else {
            return false;
        };
        let combinator = prev_part
            .combinator_to_next()
            .unwrap_or(selectors::Combinator::Descendant);
        match combinator {
            selectors::Combinator::Descendant => {
                let mut climb = current_node;
                let mut found = false;
                while let Some(parent) = style_computer.parent_by_node.get(&climb).copied() {
                    if matches_simple_selector(parent, prev_part.sel(), style_computer) {
                        current_node = parent;
                        found = true;
                        break;
                    }
                    climb = parent;
                }
                if !found {
                    return false;
                }
            }
            selectors::Combinator::Child => {
                if let Some(parent) = style_computer.parent_by_node.get(&current_node).copied() {
                    if !matches_simple_selector(parent, prev_part.sel(), style_computer) {
                        return false;
                    }
                    current_node = parent;
                } else {
                    return false;
                }
            }
        }
    }
}

/// Return true if `node` has the given CSS class in `classes_by_node`.
fn node_has_class(
    classes_by_node: &HashMap<NodeKey, Vec<String>>,
    node: NodeKey,
    class_name: &str,
) -> bool {
    classes_by_node.get(&node).is_some_and(|list| {
        list.iter()
            .any(|value| value.eq_ignore_ascii_case(class_name))
    })
}

/// Apply a single rule's winning declarations (if any) to the props map for node.
fn apply_rule_to_props(
    rule: &types::Rule,
    node: NodeKey,
    style_comp: &StyleComputer,
    props: &mut HashMap<String, CascadedDecl>,
) {
    let selector_list = selectors::parse_selector_list(&rule.prelude);

    for selector in selector_list {
        let matches = matches_selector(node, &selector, style_comp);
        if !matches {
            continue;
        }
        let specificity = selectors::compute_specificity(&selector);
        for decl in &rule.declarations {
            let entry = CascadedDecl {
                value: decl.value.clone(),
                important: decl.important,
                origin: rule.origin,
                specificity,
                source_order: rule.source_order,
                inline_boost: false,
            };
            cascade_put(props, &decl.name, entry);
        }
    }
}

/// Tracks stylesheet state and a tiny computed styles cache.
pub struct StyleComputer {
    /// The active stylesheet applied to the document.
    sheet: types::Stylesheet,
    /// Snapshot of computed styles (currently only the root is populated).
    computed: HashMap<NodeKey, style_model::ComputedStyle>,
    /// Whether the last recompute changed any styles.
    style_changed: bool,
    /// Nodes whose styles changed in the last recompute.
    changed_nodes: Vec<NodeKey>,
    /// Parsed inline style attribute declarations per node (author origin).
    inline_decls_by_node: HashMap<NodeKey, HashMap<String, String>>,
    /// Extracted custom properties (variables) per node for quick lookup.
    inline_custom_props_by_node: HashMap<NodeKey, CustomProperties>,
    /// Element metadata for selector matching.
    tag_by_node: HashMap<NodeKey, String>,
    /// Element id attributes by node (used for #id selectors).
    id_by_node: HashMap<NodeKey, String>,
    /// Element class lists by node (used for .class selectors).
    classes_by_node: HashMap<NodeKey, Vec<String>>,
    /// Parent pointers for descendant/child combinator matching.
    parent_by_node: HashMap<NodeKey, NodeKey>,
    /// Type attribute for form controls (input type="text", "checkbox", etc.).
    type_by_node: HashMap<NodeKey, String>,
    /// All element attributes by node (used for attribute selectors).
    attrs_by_node: HashMap<NodeKey, HashMap<String, String>>,
}

impl StyleComputer {
    /// Create a new `StyleComputer` with user-agent stylesheet and empty state.
    pub fn new() -> Self {
        Self {
            sheet: ua_stylesheet::create_ua_stylesheet(),
            computed: HashMap::new(),
            style_changed: false,
            changed_nodes: Vec::new(),
            inline_decls_by_node: HashMap::new(),
            inline_custom_props_by_node: HashMap::new(),
            tag_by_node: HashMap::new(),
            id_by_node: HashMap::new(),
            classes_by_node: HashMap::new(),
            parent_by_node: HashMap::new(),
            type_by_node: HashMap::new(),
            attrs_by_node: HashMap::new(),
        }
    }

    /// Replace the current author stylesheet and merge with user-agent rules.
    /// UA rules are always preserved and have lower precedence than author rules.
    pub fn replace_stylesheet(&mut self, author_sheet: types::Stylesheet) {
        // Create a new merged stylesheet with UA rules first (lower specificity)
        let ua_sheet = ua_stylesheet::create_ua_stylesheet();
        let mut merged_rules = ua_sheet.rules;

        // Add author rules after UA rules (higher source order)
        let base_order = u32::try_from(merged_rules.len()).unwrap_or(0);
        for mut rule in author_sheet.rules {
            // Offset author rule source orders to come after UA rules
            rule.source_order = rule.source_order.saturating_add(base_order);
            merged_rules.push(rule);
        }

        self.sheet = types::Stylesheet {
            rules: merged_rules,
            origin: author_sheet.origin,
        };
        self.style_changed = true;
        self.changed_nodes = self.tag_by_node.keys().copied().collect();
    }

    /// Return a clone of the current computed styles snapshot.
    pub fn computed_snapshot(&self) -> HashMap<NodeKey, style_model::ComputedStyle> {
        self.computed.clone()
    }

    /// Mirror a `DOMUpdate` into the style subsystem state.
    pub fn apply_update(&mut self, update: DOMUpdate) {
        match update {
            DOMUpdate::InsertElement {
                parent, node, tag, ..
            } => {
                self.tag_by_node.insert(node, tag);
                if parent == NodeKey::ROOT {
                    self.parent_by_node.remove(&node);
                } else {
                    self.parent_by_node.insert(node, parent);
                }
                self.changed_nodes.push(node);
            }
            DOMUpdate::InsertText { .. }
            | DOMUpdate::UpdateText { .. }
            | DOMUpdate::EndOfDocument => {}
            DOMUpdate::SetAttr { node, name, value } => {
                // Store all attributes for attribute selector matching
                self.attrs_by_node
                    .entry(node)
                    .or_default()
                    .insert(name.clone(), value.clone());

                // Handle specific attributes with special processing
                if name.eq_ignore_ascii_case("id") {
                    self.id_by_node.insert(node, value);
                } else if name.eq_ignore_ascii_case("class") {
                    let classes: Vec<String> = value
                        .split(|character: char| character.is_ascii_whitespace())
                        .filter(|segment| !segment.is_empty())
                        .map(|segment: &str| segment.to_owned())
                        .collect();
                    self.classes_by_node.insert(node, classes);
                } else if name.eq_ignore_ascii_case("style") {
                    let map = parse_style_attribute_into_map(&value);
                    let custom = extract_custom_properties(&map);
                    self.inline_decls_by_node.insert(node, map);
                    self.inline_custom_props_by_node.insert(node, custom);
                } else if name.eq_ignore_ascii_case("type") {
                    self.type_by_node.insert(node, value);
                }
                self.changed_nodes.push(node);
            }
            DOMUpdate::RemoveNode { node } => {
                self.tag_by_node.remove(&node);
                self.id_by_node.remove(&node);
                self.classes_by_node.remove(&node);
                self.inline_decls_by_node.remove(&node);
                self.inline_custom_props_by_node.remove(&node);
                self.parent_by_node.remove(&node);
                self.type_by_node.remove(&node);
                self.attrs_by_node.remove(&node);
                self.computed.remove(&node);
                self.style_changed = true;
            }
        }
    }

    /// Recompute styles for nodes changed since the last pass; returns whether any styles changed.
    pub fn recompute_dirty(&mut self) -> bool {
        if self.changed_nodes.is_empty() && self.computed.is_empty() {
            self.computed.entry(NodeKey::ROOT).or_default();
        }
        let mut any_changed = false;
        let mut visited: HashSet<NodeKey> = HashSet::new();
        let mut nodes: Vec<NodeKey> = Vec::new();
        for node_id in self.changed_nodes.drain(..) {
            if visited.insert(node_id) {
                nodes.push(node_id);
            }
        }
        if !self.tag_by_node.is_empty() && !visited.contains(&NodeKey::ROOT) {
            nodes.push(NodeKey::ROOT);
        }

        // Sort nodes in DOM tree order (parents before children) to ensure
        // parent styles are computed before children need them for inheritance
        nodes.sort_by_key(|node| {
            let mut depth = 0;
            let mut current = *node;
            while let Some(&parent) = self.parent_by_node.get(&current) {
                depth += 1;
                current = parent;
                if depth > 1000 {
                    break; // Prevent infinite loops
                }
            }
            depth
        });

        for node in nodes {
            let mut props: HashMap<String, CascadedDecl> = HashMap::new();

            // DEBUG: Log stylesheet rules for first few nodes
            for rule in &self.sheet.rules {
                apply_rule_to_props(rule, node, self, &mut props);
            }
            if let Some(inline) = self.inline_decls_by_node.get(&node) {
                let mut names: Vec<&String> = inline.keys().collect();
                names.sort();
                for name in names {
                    let value = inline.get(name).cloned().unwrap_or_default();
                    let entry = CascadedDecl {
                        value,
                        important: false,
                        origin: types::Origin::Author,
                        specificity: Specificity(1_000, 0, 0),
                        source_order: u32::MAX,
                        inline_boost: true,
                    };
                    cascade_put(&mut props, name, entry);
                }
            }
            let mut decls: HashMap<String, String> = HashMap::new();
            let mut pairs: Vec<(String, CascadedDecl)> = props.into_iter().collect();
            pairs.sort_by(|left, right| left.0.cmp(&right.0));
            for (name, entry) in pairs {
                decls.insert(name, entry.value);
            }
            // Get parent style for inheritance
            let parent_style = self
                .parent_by_node
                .get(&node)
                .and_then(|parent_key| self.computed.get(parent_key));
            let computed = build_computed_from_inline(&decls, parent_style);
            let prev = self.computed.get(&node).cloned();
            if prev.as_ref() != Some(&computed) {
                self.computed.insert(node, computed.clone());
                any_changed = true;
            }
        }
        self.style_changed = any_changed;
        any_changed
    }
}
