#![allow(clippy::excessive_nesting)]
//! A minimal DOM index mirror for host lookups (e.g., document.getElementById).
//!
//! This mirror subscribes to DOMUpdate batches and maintains small indices
//! for quick lookups by id, tag name, and class name. It is intentionally
//! simplified and only tracks what is necessary for host functions.

use crate::{DOMSubscriber, DOMUpdate, NodeKey};
use anyhow::Result;
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
    fn remove_recursively(&mut self, node: NodeKey) {
        if let Some(children) = self.children_by_parent.remove(&node) {
            for child in children {
                self.remove_recursively(child);
            }
        }
        // Detach from parent mapping
        if let Some(parent) = self.parent_by_child.remove(&node) {
            if let Some(v) = self.children_by_parent.get_mut(&parent) {
                v.retain(|c| *c != node);
            }
        }
        // Remove id mapping
        if let Some(id) = self.id_by_key.remove(&node) {
            if let Some(existing) = self.id_index.get(&id) {
                if *existing == node {
                    self.id_index.remove(&id);
                }
            }
        }
        // Remove tag index
        if let Some(tag) = self.tag_by_key.remove(&node) {
            if let Some(list) = self.tag_index.get_mut(&tag) {
                list.retain(|k| *k != node);
            }
        }
        // Remove class indices
        if let Some(classes) = self.classes_by_key.remove(&node) {
            for class in classes {
                if let Some(list) = self.class_index.get_mut(&class) {
                    list.retain(|k| *k != node);
                }
            }
        }
        // Remove text content mapping if any
        self.text_by_key.remove(&node);
    }

    /// Update class indices for a node from a whitespace-separated class attribute.
    fn set_classes_for(&mut self, node: NodeKey, class_attr: &str) {
        // Remove previous classes
        if let Some(prev) = self.classes_by_key.get(&node).cloned() {
            for c in prev {
                if let Some(list) = self.class_index.get_mut(&c) {
                    list.retain(|k| *k != node);
                }
            }
        }
        let mut set: HashSet<String> = HashSet::new();
        for token in class_attr.split(|ch: char| ch.is_whitespace()) {
            let t = token.trim();
            if t.is_empty() {
                continue;
            }
            let lc = t.to_ascii_lowercase();
            set.insert(lc.clone());
            self.class_index.entry(lc).or_default().push(node);
        }
        if set.is_empty() {
            self.classes_by_key.remove(&node);
        } else {
            self.classes_by_key.insert(node, set);
        }
    }
}

/// A DOMSubscriber implementation that updates a shared DomIndexState.
#[derive(Clone)]
pub struct DomIndex {
    inner: Arc<Mutex<DomIndexState>>,
}

impl DomIndex {
    /// Create a new DomIndex and return the subscriber and its shared state Arc.
    pub fn new() -> (Self, Arc<Mutex<DomIndexState>>) {
        let inner = Arc::new(Mutex::new(DomIndexState::default()));
        (
            Self {
                inner: inner.clone(),
            },
            inner,
        )
    }
}

impl DOMSubscriber for DomIndex {
    /// Apply a DOM update to keep indices current.
    fn apply_update(&mut self, update: DOMUpdate) -> Result<()> {
        use DOMUpdate::*;
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("DomIndexState poisoned"))?;
        match update {
            InsertElement {
                parent,
                node,
                tag,
                pos: _,
            } => {
                // Register parent/child linkage
                guard.parent_by_child.insert(node, parent);
                let entry = guard.children_by_parent.entry(parent).or_default();
                if !entry.contains(&node) {
                    entry.push(node);
                }
                // Tag indices (lowercase)
                let lc = tag.to_ascii_lowercase();
                guard.tag_by_key.insert(node, lc.clone());
                let tag_list = guard.tag_index.entry(lc).or_default();
                if !tag_list.contains(&node) {
                    tag_list.push(node);
                }
            }
            InsertText {
                parent,
                node,
                text,
                pos: _,
            } => {
                guard.parent_by_child.insert(node, parent);
                let entry = guard.children_by_parent.entry(parent).or_default();
                if !entry.contains(&node) {
                    entry.push(node);
                }
                // Track/refresh text node content
                guard.text_by_key.insert(node, text);
            }
            SetAttr { node, name, value } => {
                let name_lc = name.to_ascii_lowercase();
                if name_lc == "id" {
                    // Update reverse index: remove old mapping only if it pointed to this node
                    if let Some(old) = guard.id_by_key.insert(node, value.clone()) {
                        let should_remove =
                            matches!(guard.id_index.get(&old), Some(&n) if n == node);
                        if should_remove {
                            guard.id_index.remove(&old);
                        }
                    }
                    if value.is_empty() {
                        guard.id_by_key.remove(&node);
                    } else {
                        guard.id_index.insert(value, node);
                    }
                } else if name_lc == "class" {
                    guard.set_classes_for(node, &value);
                }
            }
            RemoveNode { node } => {
                guard.remove_recursively(node);
            }
            EndOfDocument => {}
        }
        Ok(())
    }
}

/// Accessor helpers for host functions.
impl DomIndexState {
    /// Return the NodeKey for the element with the given id (case-sensitive, per HTML spec).
    pub fn get_element_by_id(&self, id: &str) -> Option<NodeKey> {
        self.id_index.get(id).copied()
    }
    /// Return NodeKeys for elements with a given tag name (case-insensitive), in DOM order.
    pub fn get_elements_by_tag_name(&self, tag: &str) -> Vec<NodeKey> {
        let needle = tag.to_ascii_lowercase();
        let mut out: Vec<NodeKey> = Vec::new();
        fn walk(state: &DomIndexState, node: NodeKey, needle: &str, out: &mut Vec<NodeKey>) {
            if let Some(tag) = state.tag_by_key.get(&node) {
                if tag == needle {
                    out.push(node);
                }
            }
            if let Some(children) = state.children_by_parent.get(&node) {
                for child in children {
                    walk(state, *child, needle, out);
                }
            }
        }
        walk(self, NodeKey::ROOT, &needle, &mut out);
        out
    }
    /// Return NodeKeys for elements that have the given class token (case-insensitive for HTML), in DOM order.
    pub fn get_elements_by_class_name(&self, class: &str) -> Vec<NodeKey> {
        let needle = class.to_ascii_lowercase();
        let mut out: Vec<NodeKey> = Vec::new();
        fn walk(state: &DomIndexState, node: NodeKey, needle: &str, out: &mut Vec<NodeKey>) {
            if let Some(classes) = state.classes_by_key.get(&node) {
                if classes.contains(needle) {
                    out.push(node);
                }
            }
            if let Some(children) = state.children_by_parent.get(&node) {
                for child in children {
                    walk(state, *child, needle, out);
                }
            }
        }
        walk(self, NodeKey::ROOT, &needle, &mut out);
        out
    }
    /// Compute the textContent for the given node by concatenating all descendant text node contents.
    pub fn get_text_content(&self, node: NodeKey) -> String {
        fn collect(state: &DomIndexState, current: NodeKey, out: &mut String) {
            if let Some(t) = state.text_by_key.get(&current) {
                out.push_str(t);
            }
            if let Some(children) = state.children_by_parent.get(&current) {
                for child in children {
                    collect(state, *child, out);
                }
            }
        }
        let mut result = String::new();
        collect(self, node, &mut result);
        result
    }

    /// Remove a node and all of its descendants from the index immediately.
    /// This is a public wrapper used by host bindings to keep the index in sync
    /// for same-tick getters (e.g., after setTextContent before DOM updates propagate).
    pub fn remove_node_and_descendants(&mut self, node: NodeKey) {
        self.remove_recursively(node);
    }
}

/// Return types that can be used by HostContext for lookups.
pub type SharedDomIndex = Arc<Mutex<DomIndexState>>;
