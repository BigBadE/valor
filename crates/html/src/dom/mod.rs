pub mod updating;
mod printing;

use anyhow::Error;
use indextree::{Arena, NodeId};
use smallvec::SmallVec;
use std::collections::HashMap;
use std::hash::Hash;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, mpsc};
use crate::dom::updating::DOMUpdate;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct NodeKey(pub u64);

impl NodeKey {
    pub const ROOT: NodeKey = NodeKey(0);
    #[inline]
    pub fn pack(epoch: u16, shard: u8, counter: u64) -> Self {
        let c = counter & ((1u64 << 40) - 1);
        NodeKey(((epoch as u64) << 48) | ((shard as u64) << 40) | c)
    }
    #[inline]
    pub fn epoch(self) -> u16 {
        (self.0 >> 48) as u16
    }
    #[inline]
    pub fn shard(self) -> u8 {
        ((self.0 >> 40) & 0xFF) as u8
    }
    #[inline]
    pub fn counter(self) -> u64 {
        self.0 & ((1u64 << 40) - 1)
    }
}

#[derive(Debug)]
pub struct KeySpace {
    epoch: u16,
    next_shard_id: u8,
}

impl KeySpace {
    pub fn new() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        // Use low 16 bits of nanos+secs for a cheap epoch; good enough per-process
        let epoch = (((now.as_secs() as u32) ^ now.subsec_nanos()) & 0xFFFF) as u16;
        Self {
            epoch,
            next_shard_id: 1,
        }
    }
    pub fn register_manager<L: Eq + Hash + Copy>(&mut self) -> NodeKeyManager<L> {
        let shard = self.next_shard_id;
        self.next_shard_id = self.next_shard_id.wrapping_add(1);
        NodeKeyManager::new(self.epoch, shard)
    }
    pub fn epoch(&self) -> u16 {
        self.epoch
    }
}

#[derive(Clone, Debug)]
pub struct NodeKeyManager<L: Eq + Hash + Copy> {
    epoch: u16,
    shard: u8,
    counter: u64,
    map: HashMap<L, NodeKey>,
}

impl<L: Eq + Hash + Copy> NodeKeyManager<L> {
    fn new(epoch: u16, shard: u8) -> Self {
        Self { epoch, shard, counter: 1, map: HashMap::new() }
    }
    #[inline]
    pub fn key_of(&mut self, id: L) -> NodeKey {
        if let Some(&k) = self.map.get(&id) { return k; }
        let key = NodeKey::pack(self.epoch, self.shard, self.counter);
        self.counter = self.counter.wrapping_add(1);
        self.map.insert(id, key);
        key
    }
    #[inline]
    pub fn seed(&mut self, id: L, key: NodeKey) {
        self.map.insert(id, key);
    }
}

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
            for update in &batch {
                self.apply_update(update);
            }
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
            if let Some(k) = kind {
                if let Some(n) = self.dom.get_mut(mapped) {
                    let node = n.get_mut();
                    match (&node.kind, &k) {
                        (NodeKind::Document, _) => {
                            node.kind = k;
                        }
                        _ => { /* keep existing kind (e.g., text or element) */ }
                    }
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
    fn map_parent<'a>(&mut self, parser_parent: NodeKey) -> NodeId {
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
                } else {
                    if let Some(target_sibling) = parent_rt.children(&self.dom).nth(*pos) {
                        target_sibling.insert_before(child_rt, &mut self.dom);
                    } else {
                        parent_rt.append(child_rt, &mut self.dom);
                    }
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
                if let Some(n) = self.dom.get_mut(child_rt) {
                    if let NodeKind::Text { text: t } = &mut n.get_mut().kind {
                        *t = text.clone();
                    }
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
