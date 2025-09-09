use std::collections::{HashSet, HashMap};
use anyhow::Error;
use css::parser::parse_declarations;
use html::dom::updating::{DOMSubscriber, DOMUpdate};
use crate::{NodeInfo, StyleEngine};
use html::dom::NodeKey;

impl StyleEngine {
    /// Construct a placeholder NodeInfo with empty/default fields.
    fn placeholder_node_info() -> NodeInfo {
        NodeInfo {
            tag: String::new(),
            id: None,
            classes: HashSet::new(),
            attributes: HashMap::new(),
            parent: None,
            children: Vec::new(),
        }
    }

    /// Ensure the parent→child relationship is recorded, creating a placeholder parent if needed.
    fn ensure_parent_child_link(&mut self, parent: NodeKey, child: NodeKey, pos: usize) {
        if let Some(parent_info) = self.nodes.get_mut(&parent) {
            if !parent_info.children.contains(&child) {
                let idx = pos.min(parent_info.children.len());
                parent_info.children.insert(idx, child);
            }
        } else {
            let mut placeholder = Self::placeholder_node_info();
            placeholder.children = vec![child];
            self.nodes.insert(parent, placeholder);
        }
    }
    /// Return an existing NodeInfo clone for the node, or a placeholder if unknown.
    fn get_node_info_or_placeholder(&self, node: NodeKey) -> NodeInfo {
        self.nodes.get(&node).cloned().unwrap_or_else(Self::placeholder_node_info)
    }

    /// Handle insertion of an element node into the DOM stream.
    fn handle_insert_element(&mut self, parent: NodeKey, node: NodeKey, tag: &str, pos: usize) {
        // Merge with any pending info that may have arrived via SetAttr before InsertElement
        let pending = self.nodes.get(&node).cloned();
        let info = NodeInfo {
            tag: tag.to_ascii_lowercase(),
            id: pending.as_ref().and_then(|p| p.id.clone()),
            classes: pending.as_ref().map(|p| p.classes.clone()).unwrap_or_default(),
            attributes: pending.as_ref().map(|p| p.attributes.clone()).unwrap_or_default(),
            parent: Some(parent),
            children: pending.as_ref().map(|p| p.children.clone()).unwrap_or_default(),
        };
        self.nodes.insert(node, info);
        self.ensure_parent_child_link(parent, node, pos);
        self.add_tag_index(node, tag);
        self.rematch_node(node, true);
        // Defer recomputation; mark node dirty for batch recompute
        self.mark_dirty(node);
    }

    /// Parse and store inline declarations once; recompute styles.
    fn handle_set_attr_style(&mut self, node: NodeKey, value: &str) {
        let declarations = parse_declarations(value);
        if declarations.is_empty() {
            self.inline_decls.remove(&node);
        } else {
            self.inline_decls.insert(node, declarations);
        }
        // Record attribute for [style] presence and value selectors
        let mut info = self.get_node_info_or_placeholder(node);
        info.attributes.insert("style".to_string(), value.to_string());
        self.nodes.insert(node, info);
        // Inline styles do not affect selector matching; mark subtree dirty for inheritance.
        self.mark_subtree_dirty(node);
    }

    /// Handle setting the `id` attribute: update indices and rematch selectors.
    fn handle_set_attr_id(&mut self, node: NodeKey, value: &str) {
        let mut info = self.get_node_info_or_placeholder(node);
        let old_id = info.id.clone();
        let new_id = if value.is_empty() { None } else { Some(value.to_string()) };
        info.id = new_id.clone();
        self.nodes.insert(node, info);
        self.update_id_index(node, old_id, new_id);
        self.rematch_node(node, true);
        // Conservatively mark node and descendants dirty because inheritance can affect children.
        self.mark_subtree_dirty(node);
    }

    /// Handle setting the `class` attribute: update indices and rematch selectors.
    fn handle_set_attr_class(&mut self, node: NodeKey, value: &str) {
        let mut info = self.get_node_info_or_placeholder(node);
        let old_classes = info.classes.clone();
        let new_classes: HashSet<String> = value
            .split_whitespace()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        info.classes = new_classes.clone();
        self.nodes.insert(node, info);
        self.update_class_index(node, &old_classes, &new_classes);
        self.rematch_node(node, true);
        // Conservatively mark node and descendants dirty because inheritance can affect children.
        self.mark_subtree_dirty(node);
    }
}

/// DOMSubscriber implementation that mirrors DOM updates into the StyleEngine state
/// and keeps style-related indices and computed styles in sync.
impl DOMSubscriber for StyleEngine {
    /// Apply a single DOMUpdate to the StyleEngine mirror, keeping indices and
    /// computed styles in sync. Large cases are delegated to smaller helpers.
    fn apply_update(&mut self, update: DOMUpdate) -> Result<(), Error> {
        use DOMUpdate::*;
        match update {
            InsertElement {
                parent,
                node,
                tag,
                pos,
            } => {
                self.handle_insert_element(parent, node, &tag, pos);
            }
            InsertText { .. } => {
                // No computed style for text nodes at the moment.
            }
            SetAttr { node, name, value } => {
                if name.eq_ignore_ascii_case("style") {
                    self.handle_set_attr_style(node, &value);
                } else if name.eq_ignore_ascii_case("id") {
                    self.handle_set_attr_id(node, &value);
                } else if name.eq_ignore_ascii_case("class") {
                    self.handle_set_attr_class(node, &value);
                } else {
                    // Generic attribute: record it for attribute selectors and rematch
                    let mut info = self.get_node_info_or_placeholder(node);
                    info.attributes.insert(name.to_ascii_lowercase(), value.clone());
                    self.nodes.insert(node, info);
                    self.rematch_node(node, true);
                    // Attribute changes do not affect inheritance; only node needs recompute
                    self.mark_dirty(node);
                }
            }
            RemoveNode { node } => {
                self.remove_node_recursive(node);
            }
            EndOfDocument => {
                // Flush pending style recomputations at batch end.
                self.recompute_dirty();
            }
        }
        Ok(())
    }
}