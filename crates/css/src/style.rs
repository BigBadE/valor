//! CSS style application - matches selectors against DOM and stores properties.

use crate::ParsedRule;
use lightningcss::declaration::DeclarationBlock;
use lightningcss::properties::{Property, PropertyId};
use lightningcss::stylesheet::ParserOptions;
use rewrite_core::{NodeId, Specificity, Subscriptions};
use rewrite_html::{DomTree, NodeData};
use std::sync::Arc;

/// Minimum specificity to be considered "confident" - at least one class or id.
const CONFIDENCE_THRESHOLD: Specificity = Specificity::new(0, 1, 0);

/// Holds parsed CSS rules and applies them to the DOM.
pub struct Styler {
    rules: boxcar::Vec<ParsedRule>,
    /// Maps each node (by index) to indices of matching rules in `rules`.
    /// Kept in sync with DomTree - a new empty vec is added for each node in style_node.
    matched_rules: boxcar::Vec<boxcar::Vec<usize>>,
    tree: Arc<DomTree>,
    subscriptions: Arc<Subscriptions>,
}

impl Styler {
    /// Create a new Styler with the given tree and subscriptions.
    pub fn new(tree: Arc<DomTree>, subscriptions: Arc<Subscriptions>) -> Self {
        Self {
            rules: boxcar::Vec::new(),
            matched_rules: boxcar::Vec::new(),
            tree,
            subscriptions,
        }
    }

    /// Add a rule and apply it to all existing nodes in the tree.
    pub fn add_rule(&self, rule: ParsedRule) {
        let rule_idx = self.rules.count();
        self.rules.push(rule);

        (0..self.tree.nodes.count())
            .map(|idx| NodeId(idx as u32))
            .filter(|&node_id| self.rules[rule_idx].matches(node_id, &self.tree))
            .for_each(|node_id| self.apply_rule(node_id, rule_idx));
    }

    /// Apply all rules to a newly added node, including its inline styles.
    /// Called during CreateNode — the node may not have a parent yet.
    pub fn style_node(&self, node_id: NodeId) {
        // Ensure storage exists for this node
        while self.matched_rules.count() <= node_id.0 as usize {
            self.matched_rules.push(boxcar::Vec::new());
        }

        // Apply stylesheet rules
        self.rules
            .iter()
            .filter(|(_, rule)| rule.matches(node_id, &self.tree))
            .for_each(|(idx, _)| self.apply_rule(node_id, idx));

        // Parse and add inline styles as a rule
        if let Some(rule) = parse_inline_styles(node_id, &self.tree) {
            let rule_idx = self.rules.count();
            self.rules.push(rule);
            self.apply_rule(node_id, rule_idx);
        }
    }

    /// Re-match stylesheet rules after the node has been placed in the tree.
    /// Called during AppendChild — ancestor-dependent selectors (e.g. `div > p`)
    /// can now match because the node has a parent.
    pub fn restyle_node(&self, node_id: NodeId) {
        let node_rules = &self.matched_rules[node_id.0 as usize];

        for (rule_idx, rule) in self.rules.iter() {
            // Skip rules already matched for this node
            if node_rules.iter().any(|(_, &idx)| idx == rule_idx) {
                continue;
            }

            if rule.matches(node_id, &self.tree) {
                self.apply_rule(node_id, rule_idx);
            }
        }
    }

    /// Apply a rule to a node: record the match and notify for winning properties.
    /// Only notifies if the rule is confident (high specificity).
    fn apply_rule(&self, node_id: NodeId, rule_idx: usize) {
        let rule = &self.rules[rule_idx];
        let rule_specificity = rule.specificity();
        let props = rule.properties();
        let node_rules = &self.matched_rules[node_id.0 as usize];
        let is_confident = rule_specificity >= CONFIDENCE_THRESHOLD;

        // Check each normal property - notify if confident and not dominated
        for prop in &props.normal {
            let prop_id = prop.property_id();
            let dominated = self.is_dominated(node_rules, &prop_id, rule_specificity, false);
            if is_confident && !dominated {
                self.subscriptions.notify_property(node_id, prop);
            }
        }

        // Check each important property - notify if confident and not dominated
        for prop in &props.important {
            let prop_id = prop.property_id();
            let dominated = self.is_dominated(node_rules, &prop_id, rule_specificity, true);
            if is_confident && !dominated {
                self.subscriptions.notify_property(node_id, prop);
            }
        }

        // Record the match
        self.matched_rules[node_id.0 as usize].push(rule_idx);
    }

    /// Check if there's an existing rule with higher specificity for this property.
    fn is_dominated(
        &self,
        node_rules: &boxcar::Vec<usize>,
        prop_id: &PropertyId<'static>,
        specificity: Specificity,
        is_important: bool,
    ) -> bool {
        node_rules.iter().any(|(_, &idx)| {
            let existing = &self.rules[idx];
            let existing_props = existing.properties();
            let existing_spec = existing.specificity();

            if is_important {
                // Important property: only dominated by higher-specificity important
                existing_props.has_important(prop_id)
                    && existing_spec.with_important(true) > specificity.with_important(true)
            } else {
                // Normal property: dominated by higher-specificity normal OR any important
                (existing_props.has_property(prop_id) && existing_spec > specificity)
                    || existing_props.has_important(prop_id)
            }
        })
    }

    /// Check if a property is dominated by a confident rule for this node.
    fn is_dominated_by_confident(
        &self,
        node_rules: &boxcar::Vec<usize>,
        prop_id: &PropertyId<'static>,
        is_important: bool,
    ) -> bool {
        node_rules.iter().any(|(_, &idx)| {
            let existing = &self.rules[idx];
            let existing_props = existing.properties();
            let existing_spec = existing.specificity();

            // Must be confident to dominate
            if existing_spec < CONFIDENCE_THRESHOLD {
                return false;
            }

            if is_important {
                existing_props.has_important(prop_id)
            } else {
                existing_props.has_property(prop_id) || existing_props.has_important(prop_id)
            }
        })
    }

    /// Get a reference to the DOM tree.
    pub fn tree(&self) -> &DomTree {
        &self.tree
    }

    /// Resolve the cascade for a single property on a node.
    ///
    /// Returns the winning (highest-specificity) property from matched rules,
    /// without inheritance or unit resolution. Used internally by `flush()`
    /// and `apply_rule()`.
    fn cascade_winner(
        &self,
        node_id: NodeId,
        prop_id: &PropertyId<'static>,
    ) -> Option<&Property<'static>> {
        let node_rules = &self.matched_rules[node_id.0 as usize];
        let mut winner: Option<&Property<'static>> = None;
        let mut best_spec = Specificity::ZERO;

        for (_, &rule_idx) in node_rules.iter() {
            let rule = &self.rules[rule_idx];
            let spec = rule.specificity();
            let props = rule.properties();

            // Check important properties first (they always win over normal)
            for prop in &props.important {
                if prop.property_id() == *prop_id {
                    let important_spec = spec.with_important(true);
                    if important_spec >= best_spec {
                        winner = Some(prop);
                        best_spec = important_spec;
                    }
                }
            }

            // Check normal properties (only if no important winner yet)
            if !best_spec.important {
                for prop in &props.normal {
                    if prop.property_id() == *prop_id {
                        if spec >= best_spec {
                            winner = Some(prop);
                            best_spec = spec;
                        }
                    }
                }
            }
        }

        winner
    }

    /// Flush all low-confidence rules: resolve the cascade for each property
    /// and notify only the winning value.
    /// Call this after stylesheet parsing is complete.
    pub fn flush(&self) {
        // Collect all property IDs from low-confidence rules per node,
        // then resolve the full cascade to find the winner.
        for node_idx in 0..self.matched_rules.count() {
            let node_id = NodeId(node_idx as u32);
            let node_rules = &self.matched_rules[node_idx];

            // Collect unique property IDs from low-confidence rules for this node.
            let mut prop_ids: Vec<PropertyId<'static>> = Vec::new();
            for (_, &rule_idx) in node_rules.iter() {
                let rule = &self.rules[rule_idx];
                if rule.specificity() >= CONFIDENCE_THRESHOLD {
                    continue;
                }
                let props = rule.properties();
                for prop in props.normal.iter().chain(props.important.iter()) {
                    let pid = prop.property_id();
                    if !prop_ids.contains(&pid) {
                        prop_ids.push(pid);
                    }
                }
            }

            // For each property, resolve the full cascade and notify the winner.
            for prop_id in &prop_ids {
                if let Some(winner) = self.cascade_winner(node_id, prop_id) {
                    self.subscriptions.notify_property(node_id, winner);
                }
            }
        }
    }
}

/// Parse inline style attribute into a ParsedRule if present.
fn parse_inline_styles(node_id: NodeId, tree: &DomTree) -> Option<ParsedRule> {
    let NodeData::Element { attributes, .. } = &tree.nodes[node_id.0 as usize] else {
        return None;
    };

    let options = ParserOptions {
        error_recovery: true,
        ..Default::default()
    };

    tree.interner
        .get("style")
        .and_then(|key| attributes.get(&key))
        .and_then(|style| DeclarationBlock::parse_string(style, options).ok())
        .map(|decls| ParsedRule::Inline {
            node_id,
            properties: decls.into(),
        })
}
