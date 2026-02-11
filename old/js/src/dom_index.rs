//! A minimal DOM index mirror for host lookups (e.g., document.getElementById).
//!
//! This mirror subscribes to DOMUpdate batches and maintains small indices
//! for quick lookups by id, tag name, and class name. It is intentionally
//! simplified and only tracks what is necessary for host functions.

use crate::{DOMSubscriber, DOMUpdate, NodeKey};
use anyhow::{Error as AnyhowError, Result};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

/// Internal mutable state for the DOM index.
#[derive(Default)]
pub struct DomIndexState {
    /// Map node -> current tag name (for elements), lowercase.
    pub tag_by_key: HashMap<NodeKey, String>,
    /// Map node -> current id attribute (if any).
    pub id_by_key: HashMap<NodeKey, String>,
    /// Map node -> current class list tokens (lowercase).
    pub classes_by_key: HashMap<NodeKey, HashSet<String>>,
    /// Parent -> children relation for recursive removals.
    pub children_by_parent: HashMap<NodeKey, Vec<NodeKey>>,
    /// Child -> parent relation.
    pub parent_by_child: HashMap<NodeKey, NodeKey>,
    /// Lookup indices
    pub id_index: HashMap<String, NodeKey>,
    pub tag_index: HashMap<String, Vec<NodeKey>>, // lowercased tag -> nodes
    pub class_index: HashMap<String, Vec<NodeKey>>, // lowercased class -> nodes
    /// Map text node key -> current text content.
    pub text_by_key: HashMap<NodeKey, String>,
}

impl DomIndexState {
    /// Remove a node (and its descendants) from all indices.
    #[inline]
    fn remove_recursively(&mut self, node: NodeKey) {
        if let Some(children) = self.children_by_parent.remove(&node) {
            for child in children {
                self.remove_recursively(child);
            }
        }
        // Detach from parent mapping
        if let Some(parent) = self.parent_by_child.remove(&node) {
            if let Some(children_vec) = self.children_by_parent.get_mut(&parent) {
                children_vec.retain(|child_key| *child_key != node);
            }
        }
        // Remove id mapping
        if let Some(element_id) = self.id_by_key.remove(&node) {
            if let Some(existing) = self.id_index.get(&element_id) {
                if *existing == node {
                    self.id_index.remove(&element_id);
                }
            }
        }
        // Remove tag index
        if let Some(tag) = self.tag_by_key.remove(&node) {
            if let Some(list) = self.tag_index.get_mut(&tag) {
                list.retain(|key| *key != node);
            }
        }
        // Remove class indices
        if let Some(classes) = self.classes_by_key.remove(&node) {
            let classes_vec: Vec<_> = classes.into_iter().collect();
            for class in classes_vec {
                if let Some(list) = self.class_index.get_mut(&class) {
                    list.retain(|key| *key != node);
                }
            }
        }
        // Remove text content mapping if any
        self.text_by_key.remove(&node);
    }

    /// Update class indices for a node from a whitespace-separated class attribute.
    #[inline]
    fn set_classes_for(&mut self, node: NodeKey, class_attr: &str) {
        // Remove previous classes
        if let Some(prev) = self.classes_by_key.get(&node).cloned() {
            let prev_vec: Vec<_> = prev.into_iter().collect();
            for class_name in prev_vec {
                if let Some(list) = self.class_index.get_mut(&class_name) {
                    list.retain(|key| *key != node);
                }
            }
        }
        let mut set: HashSet<String> = HashSet::new();
        for token in class_attr.split(|character: char| character.is_whitespace()) {
            let trimmed_token = token.trim();
            if trimmed_token.is_empty() {
                continue;
            }
            let lowercase_class = trimmed_token.to_ascii_lowercase();
            set.insert(lowercase_class.clone());
            self.class_index
                .entry(lowercase_class)
                .or_default()
                .push(node);
        }
        if set.is_empty() {
            self.classes_by_key.remove(&node);
        } else {
            self.classes_by_key.insert(node, set);
        }
    }

    /// Update the id attribute mapping and reverse index for a node.
    #[inline]
    fn update_id_for(&mut self, node: NodeKey, value: &str) {
        if let Some(old) = self.id_by_key.insert(node, value.to_owned()) {
            if self
                .id_index
                .get(&old)
                .is_some_and(|&existing_node| existing_node == node)
            {
                self.id_index.remove(&old);
            }
        }
        if value.is_empty() {
            self.id_by_key.remove(&node);
        } else {
            self.id_index.insert(value.to_owned(), node);
        }
    }

    /// Return the `NodeKey` for the element with the given id (case-sensitive, per HTML spec).
    #[inline]
    pub fn get_element_by_id(&self, element_id: &str) -> Option<NodeKey> {
        self.id_index.get(element_id).copied()
    }

    /// Return `NodeKey`s for elements with a given tag name (case-insensitive), in DOM order.
    #[inline]
    pub fn get_elements_by_tag_name(&self, tag: &str) -> Vec<NodeKey> {
        let needle = tag.to_ascii_lowercase();
        let mut out: Vec<NodeKey> = Vec::new();
        self.walk_for_tag(&needle, NodeKey::ROOT, &mut out);
        out
    }

    /// Helper to recursively walk the tree for tag matching.
    fn walk_for_tag(&self, needle: &str, node: NodeKey, out: &mut Vec<NodeKey>) {
        if let Some(tag) = self.tag_by_key.get(&node) {
            if tag == needle {
                out.push(node);
            }
        }
        if let Some(children) = self.children_by_parent.get(&node) {
            for child in children {
                self.walk_for_tag(needle, *child, out);
            }
        }
    }

    /// Return `NodeKey`s for elements that have the given class token (case-insensitive for HTML), in DOM order.
    #[inline]
    pub fn get_elements_by_class_name(&self, class: &str) -> Vec<NodeKey> {
        let needle = class.to_ascii_lowercase();
        let mut out: Vec<NodeKey> = Vec::new();
        self.walk_for_class(&needle, NodeKey::ROOT, &mut out);
        out
    }

    /// Helper to recursively walk the tree for class matching.
    fn walk_for_class(&self, needle: &str, node: NodeKey, out: &mut Vec<NodeKey>) {
        if let Some(classes) = self.classes_by_key.get(&node) {
            if classes.contains(needle) {
                out.push(node);
            }
        }
        if let Some(children) = self.children_by_parent.get(&node) {
            for child in children {
                self.walk_for_class(needle, *child, out);
            }
        }
    }

    /// Compute the `textContent` for the given node by concatenating all descendant text node contents.
    #[inline]
    pub fn get_text_content(&self, node: NodeKey) -> String {
        let mut result = String::new();
        self.collect_text(node, &mut result);
        result
    }

    /// Helper to recursively collect text content.
    fn collect_text(&self, current: NodeKey, out: &mut String) {
        if let Some(text) = self.text_by_key.get(&current) {
            out.push_str(text);
        }
        if let Some(children) = self.children_by_parent.get(&current) {
            for child in children {
                self.collect_text(*child, out);
            }
        }
    }

    /// Remove a node and all of its descendants from the index immediately.
    /// This is a public wrapper used by host bindings to keep the index in sync
    /// for same-tick getters (e.g., after `setTextContent` before DOM updates propagate).
    #[inline]
    pub fn remove_node_and_descendants(&mut self, node: NodeKey) {
        self.remove_recursively(node);
    }
}

/// A `DOMSubscriber` implementation that updates a shared `DomIndexState`.
#[derive(Clone)]
pub struct DomIndex {
    /// Shared state protected by a mutex.
    inner: Arc<Mutex<DomIndexState>>,
}

impl DomIndex {
    /// Create a new `DomIndex` and return the subscriber and its shared state `Arc`.
    #[inline]
    pub fn new() -> (Self, Arc<Mutex<DomIndexState>>) {
        let inner = Arc::new(Mutex::new(DomIndexState::default()));
        (
            Self {
                inner: Arc::clone(&inner),
            },
            inner,
        )
    }
}

impl DOMSubscriber for DomIndex {
    /// Apply a DOM update to keep indices current.
    #[inline]
    fn apply_update(&mut self, update: DOMUpdate) -> Result<()> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| AnyhowError::msg("DomIndexState poisoned"))?;
        match update {
            DOMUpdate::InsertElement {
                parent, node, tag, ..
            } => {
                // Register parent/child linkage
                guard.parent_by_child.insert(node, parent);
                let entry = guard.children_by_parent.entry(parent).or_default();
                if !entry.contains(&node) {
                    entry.push(node);
                }
                // Tag indices (lowercase)
                let lowercase_tag = tag.to_ascii_lowercase();
                guard.tag_by_key.insert(node, lowercase_tag.clone());
                let tag_list = guard.tag_index.entry(lowercase_tag).or_default();
                if !tag_list.contains(&node) {
                    tag_list.push(node);
                }
            }
            DOMUpdate::InsertText {
                parent, node, text, ..
            } => {
                guard.parent_by_child.insert(node, parent);
                let entry = guard.children_by_parent.entry(parent).or_default();
                if !entry.contains(&node) {
                    entry.push(node);
                }
                // Track/refresh text node content
                guard.text_by_key.insert(node, text);
            }
            DOMUpdate::SetAttr { node, name, value } => {
                let name_lc = name.to_ascii_lowercase();
                if name_lc == "id" {
                    guard.update_id_for(node, &value);
                } else if name_lc == "class" {
                    guard.set_classes_for(node, &value);
                }
            }
            DOMUpdate::RemoveNode { node } => {
                guard.remove_recursively(node);
                drop(guard);
            }
            DOMUpdate::UpdateText { node, text } => {
                // Update the text content of an existing text node in-place
                guard.text_by_key.insert(node, text);
                drop(guard);
            }
            DOMUpdate::EndOfDocument => {
                drop(guard);
            }
        }
        Ok(())
    }
}

/// Return types that can be used by `HostContext` for lookups.
pub type SharedDomIndex = Arc<Mutex<DomIndexState>>;
