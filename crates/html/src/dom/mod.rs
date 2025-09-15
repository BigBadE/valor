mod printing;

use anyhow::Error;
use indextree::{Arena, NodeId};
use js::{DOMUpdate, KeySpace, NodeKey, NodeKeyManager};
use smallvec::SmallVec;
use std::collections::HashMap;
use std::hash::Hash;
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
    dom: Arena<DOMNode>,
    root: NodeId,
    out_updater: broadcast::Sender<Vec<DOMUpdate>>,
    in_receiver: mpsc::Receiver<Vec<DOMUpdate>>,
    // Map stable NodeKey -> runtime DOM NodeId
    id_map: HashMap<NodeKey, NodeId>,
    keyspace: KeySpace,
}

impl DOM {
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

    pub fn register_manager<L: Eq + Hash + Copy>(&mut self) -> NodeKeyManager<L> {
        self.keyspace.register_manager()
    }

    // Convenience for the parser's local id type
    pub fn register_parser_manager(&mut self) -> NodeKeyManager<NodeId> {
        self.keyspace.register_manager()
    }

    pub async fn update(&mut self) -> Result<(), Error> {
        while let Ok(batch) = self.in_receiver.try_recv() {
            // Apply and collect simple counts for test-printing diagnostics
            let mut insert_element_count = 0usize;
            let mut insert_text_count = 0usize;
            let mut set_attr_count = 0usize;
            let mut remove_node_count = 0usize;
            let mut end_of_document_count = 0usize;
            for update in &batch {
                match update {
                    DOMUpdate::InsertElement { .. } => insert_element_count += 1,
                    DOMUpdate::InsertText { .. } => insert_text_count += 1,
                    DOMUpdate::SetAttr { .. } => set_attr_count += 1,
                    DOMUpdate::RemoveNode { .. } => remove_node_count += 1,
                    DOMUpdate::EndOfDocument => end_of_document_count += 1,
                }
                self.apply_update(update);
            }
            // Test printing: summarize the batch we just applied
            log::info!(
                "DOM.update: applied batch_size={} InsertElement={} InsertText={} SetAttr={} RemoveNode={} EndOfDocument={}",
                batch.len(),
                insert_element_count,
                insert_text_count,
                set_attr_count,
                remove_node_count,
                end_of_document_count
            );
            // Send update to mirrors, ignoring it if there's no listeners.
            let _ = self.out_updater.send(batch);
        }
        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Vec<DOMUpdate>> {
        self.out_updater.subscribe()
    }

    /// Helper to get or create a runtime node for a parser node id
    fn ensure_node(&mut self, parser_id: NodeKey, kind: Option<NodeKind>) -> NodeId {
        if let Some(&mapped) = self.id_map.get(&parser_id) {
            // Optionally update kind if provided and current is default
            if let Some(k) = kind
                && let Some(n) = self.dom.get_mut(mapped)
            {
                let node = n.get_mut();
                match (&node.kind, &k) {
                    (NodeKind::Document, _) => {
                        node.kind = k;
                    }
                    _ => { /* keep existing kind (e.g., text or element) */ }
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

    fn apply_update(&mut self, update: &DOMUpdate) {
        use DOMUpdate::*;

        match update {
            InsertElement {
                parent,
                node,
                tag,
                pos,
            } => {
                let parent_rt = self.map_parent(*parent);
                let child_rt =
                    self.ensure_node(*node, Some(NodeKind::Element { tag: tag.clone() }));
                // If child is already attached somewhere, detach first
                if self.dom.get(child_rt).and_then(|n| n.parent()).is_some() {
                    child_rt.detach(&mut self.dom);
                }
                // Insert at position among parent's children
                let count = parent_rt.children(&self.dom).count();
                if *pos >= count {
                    parent_rt.append(child_rt, &mut self.dom);
                } else if let Some(target_sibling) = parent_rt.children(&self.dom).nth(*pos) {
                    target_sibling.insert_before(child_rt, &mut self.dom);
                } else {
                    parent_rt.append(child_rt, &mut self.dom);
                }
            }
            InsertText {
                parent,
                node,
                text,
                pos,
            } => {
                let parent_rt = self.map_parent(*parent);
                let child_rt = self.ensure_node(*node, Some(NodeKind::Text { text: text.clone() }));
                // Update text content if node existed already
                if let Some(n) = self.dom.get_mut(child_rt)
                    && let NodeKind::Text { text: t } = &mut n.get_mut().kind
                {
                    *t = text.clone();
                }
                if self.dom.get(child_rt).and_then(|n| n.parent()).is_some() {
                    child_rt.detach(&mut self.dom);
                }
                let count = parent_rt.children(&self.dom).count();
                if *pos >= count {
                    parent_rt.append(child_rt, &mut self.dom);
                } else if let Some(target_sibling) = parent_rt.children(&self.dom).nth(*pos) {
                    target_sibling.insert_before(child_rt, &mut self.dom);
                } else {
                    parent_rt.append(child_rt, &mut self.dom);
                }
            }
            SetAttr { node, name, value } => {
                let rt = self.ensure_node(*node, None);
                if let Some(n) = self.dom.get_mut(rt) {
                    let attrs = &mut n.get_mut().attrs;
                    if let Some((_, v)) = attrs.iter_mut().find(|(k, _)| k == name) {
                        *v = value.clone();
                    } else {
                        attrs.push((name.clone(), value.clone()));
                    }
                }
            }
            RemoveNode { node } => {
                if let Some(&rt) = self.id_map.get(node) {
                    // Detach from parent if attached
                    rt.detach(&mut self.dom);
                    // Keep mapping for potential future references; minimal change.
                }
            }
            EndOfDocument => {
                // No-op, mirrors should react to this if needed.
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DOMNode {
    pub kind: NodeKind,
    pub attrs: SmallVec<(String, String), 4>,
}
