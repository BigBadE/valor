/// DOM printing and serialization utilities.
mod printing;

use anyhow::Error;
use core::hash::Hash;
use indextree::{Arena, Node, NodeId};
use js::{DOMUpdate, KeySpace, NodeKey, NodeKeyManager};
use log::info;
use serde_json::Value;
use smallvec::SmallVec;
use std::collections::HashMap;
use tokio::sync::{broadcast, mpsc};

#[derive(Debug, Clone, Default)]
pub enum NodeKind {
    #[default]
    Document,
    Element {
        tag: String,
    },
    Text {
        text: String,
    },
}

pub struct DOM {
    /// The arena storing all DOM nodes.
    dom: Arena<DOMNode>,
    /// The root node ID.
    root: NodeId,
    /// Sender for broadcasting DOM updates.
    out_updater: broadcast::Sender<Vec<DOMUpdate>>,
    /// Receiver for incoming DOM updates.
    in_receiver: mpsc::Receiver<Vec<DOMUpdate>>,
    /// Map from parser-local `NodeKey` to runtime `NodeId`.
    id_map: HashMap<NodeKey, NodeId>,
    /// Keyspace for managing node keys.
    keyspace: KeySpace,
}

impl DOM {
    /// Build a deterministic JSON representation of the DOM.
    /// Schema:
    /// - Document: { "type":"document", "children":[ ... ] }
    /// - Element: { "type":"element", "tag": "div", "attrs": {..}, "children":[ ... ] }
    /// - Text: { "type":"text", "text":"..." }
    #[inline]
    pub fn to_json_value(&self) -> Value {
        printing::node_to_json(self, self.root)
    }

    /// Pretty JSON string for snapshots and test comparisons.
    #[inline]
    pub fn to_json_string(&self) -> String {
        use serde_json::to_string_pretty;
        to_string_pretty(&self.to_json_value()).unwrap_or_else(|_| String::from("{}"))
    }

    #[inline]
    pub fn new(
        out_updater: broadcast::Sender<Vec<DOMUpdate>>,
        in_receiver: mpsc::Receiver<Vec<DOMUpdate>>,
    ) -> Self {
        let mut dom = Arena::new();
        let root = dom.new_node(DOMNode::default());
        let mut id_map = HashMap::new();
        id_map.insert(NodeKey::ROOT, root);
        Self {
            root,
            dom,
            out_updater,
            in_receiver,
            id_map,
            keyspace: KeySpace::new(),
        }
    }

    #[inline]
    pub fn register_manager<L: Eq + Hash + Copy>(&mut self) -> NodeKeyManager<L> {
        self.keyspace.register_manager()
    }

    // Convenience for the parser's local id type
    #[inline]
    pub fn register_parser_manager(&mut self) -> NodeKeyManager<NodeId> {
        self.keyspace.register_manager()
    }

    /// Apply pending DOM updates from the receiver and return them.
    ///
    /// # Errors
    /// Returns an error if update application fails.
    #[inline]
    pub fn update(&mut self) -> Result<Vec<DOMUpdate>, Error> {
        let mut all_updates = Vec::new();
        while let Ok(batch) = self.in_receiver.try_recv() {
            // Collect all updates for return
            all_updates.extend(batch.clone());

            // Apply and collect simple counts for test-printing diagnostics
            let mut insert_element_count = 0usize;
            let mut insert_text_count = 0usize;
            let mut set_attr_count = 0usize;
            let mut remove_node_count = 0usize;
            let mut update_text_count = 0usize;
            let mut end_of_document_count = 0usize;
            for update in &batch {
                match update {
                    DOMUpdate::InsertElement { .. } => insert_element_count += 1,
                    DOMUpdate::InsertText { .. } => insert_text_count += 1,
                    DOMUpdate::SetAttr { .. } => set_attr_count += 1,
                    DOMUpdate::RemoveNode { .. } => remove_node_count += 1,
                    DOMUpdate::UpdateText { .. } => update_text_count += 1,
                    DOMUpdate::EndOfDocument => end_of_document_count += 1,
                }
                self.apply_update(update);
            }
            // Test printing: summarize the batch we just applied
            info!(
                "DOM.update: applied batch_size={} InsertElement={} InsertText={} SetAttr={} RemoveNode={} UpdateText={} EndOfDocument={}",
                batch.len(),
                insert_element_count,
                insert_text_count,
                set_attr_count,
                remove_node_count,
                update_text_count,
                end_of_document_count
            );
            // Send update to mirrors, ignoring it if there's no listeners.
            drop(self.out_updater.send(batch));
        }
        Ok(all_updates)
    }

    #[inline]
    pub fn subscribe(&self) -> broadcast::Receiver<Vec<DOMUpdate>> {
        self.out_updater.subscribe()
    }

    /// Helper to get or create a runtime node for a parser node id
    fn ensure_node(&mut self, parser_id: NodeKey, kind: Option<NodeKind>) -> NodeId {
        if let Some(&mapped) = self.id_map.get(&parser_id) {
            // Optionally update kind if provided and current is default
            if let Some(node_kind) = kind
                && let Some(node_ref) = self.dom.get_mut(mapped)
            {
                let node = node_ref.get_mut();
                if matches!(node.kind, NodeKind::Document) {
                    node.kind = node_kind;
                }
            }
            return mapped;
        }
        let new_id = self.dom.new_node(DOMNode {
            kind: kind.unwrap_or_default(),
            attrs: SmallVec::new(),
        });
        self.id_map.insert(parser_id, new_id);
        new_id
    }

    /// Helper to map a parent id, defaulting to root if unknown
    fn map_parent(&mut self, parser_parent: NodeKey) -> NodeId {
        if let Some(&mapped) = self.id_map.get(&parser_parent) {
            mapped
        } else {
            self.id_map.insert(parser_parent, self.root);
            self.root
        }
    }

    /// Apply a single DOM update to the tree.
    fn apply_update(&mut self, update: &DOMUpdate) {
        use DOMUpdate::{
            EndOfDocument, InsertElement, InsertText, RemoveNode, SetAttr, UpdateText,
        };

        match update {
            InsertElement {
                parent,
                node,
                tag,
                pos,
            } => self.apply_insert_element(*parent, *node, tag, *pos),
            InsertText {
                parent,
                node,
                text,
                pos,
            } => self.apply_insert_text(*parent, *node, text, *pos),
            SetAttr { node, name, value } => self.apply_set_attr(*node, name, value),
            RemoveNode { node } => self.apply_remove_node(*node),
            UpdateText { node, text } => self.apply_update_text(*node, text),
            EndOfDocument => {
                // No-op, mirrors should react to this if needed.
            }
        }
    }

    /// Insert an element node into the DOM tree at the specified position.
    fn apply_insert_element(&mut self, parent: NodeKey, node: NodeKey, tag: &str, pos: usize) {
        let parent_rt = self.map_parent(parent);
        let child_rt = self.ensure_node(
            node,
            Some(NodeKind::Element {
                tag: tag.to_owned(),
            }),
        );
        // If child is already attached somewhere, detach first
        if self.dom.get(child_rt).and_then(Node::parent).is_some() {
            child_rt.detach(&mut self.dom);
        }
        self.insert_child_at_position(parent_rt, child_rt, pos);
    }

    /// Insert a text node into the DOM tree at the specified position.
    fn apply_insert_text(&mut self, parent: NodeKey, node: NodeKey, text: &str, pos: usize) {
        let parent_rt = self.map_parent(parent);
        let child_rt = self.ensure_node(
            node,
            Some(NodeKind::Text {
                text: text.to_owned(),
            }),
        );
        // Update text content if node existed already
        if let Some(node_ref) = self.dom.get_mut(child_rt)
            && let NodeKind::Text { text: text_ref } = &mut node_ref.get_mut().kind
        {
            text.clone_into(text_ref);
        }
        if self.dom.get(child_rt).and_then(Node::parent).is_some() {
            child_rt.detach(&mut self.dom);
        }
        self.insert_child_at_position(parent_rt, child_rt, pos);
    }

    /// Insert a child node at a specific position among parent's children.
    fn insert_child_at_position(&mut self, parent_rt: NodeId, child_rt: NodeId, pos: usize) {
        let count = parent_rt.children(&self.dom).count();
        if pos >= count {
            parent_rt.append(child_rt, &mut self.dom);
        } else if let Some(target_sibling) = parent_rt.children(&self.dom).nth(pos) {
            target_sibling.insert_before(child_rt, &mut self.dom);
        } else {
            parent_rt.append(child_rt, &mut self.dom);
        }
    }

    /// Set an attribute on a node, updating or adding as needed.
    fn apply_set_attr(&mut self, node: NodeKey, name: &str, value: &str) {
        let runtime_id = self.ensure_node(node, None);
        if let Some(node_ref) = self.dom.get_mut(runtime_id) {
            let attrs = &mut node_ref.get_mut().attrs;
            if let Some((_, val)) = attrs.iter_mut().find(|(key, _)| key == name) {
                value.clone_into(val);
            } else {
                attrs.push((name.to_owned(), value.to_owned()));
            }
        }
    }

    /// Remove a node from its parent without deleting it.
    fn apply_remove_node(&mut self, node: NodeKey) {
        if let Some(&runtime_id) = self.id_map.get(&node) {
            // Detach from parent if attached
            runtime_id.detach(&mut self.dom);
            // Keep mapping for potential future references; minimal change.
        }
    }

    /// Update the text content of an existing text node.
    fn apply_update_text(&mut self, node: NodeKey, text: &str) {
        if let Some(&runtime_id) = self.id_map.get(&node)
            && let Some(node_ref) = self.dom.get_mut(runtime_id)
            && let NodeKind::Text { text: text_ref } = &mut node_ref.get_mut().kind
        {
            text.clone_into(text_ref);
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DOMNode {
    pub kind: NodeKind,
    pub attrs: SmallVec<(String, String), 4>,
}
