use std::mem;
use anyhow::Error;
use indextree::{Arena, NodeId};
use log::warn;
use smallvec::SmallVec;

#[derive(Debug, Clone, Default)]
pub enum NodeKind {
    #[default]
    Document,
    Element { tag: String },
    Text { text: String },
}

#[derive(Debug, Clone)]
pub struct DOM {
    dom: Arena<DOMNode>,
    root: NodeId,
    updates: Vec<DOMUpdate>,
    update_sender: Option<tokio::sync::broadcast::Sender<Vec<DOMUpdate>>>,
}

impl Default for DOM {
    fn default() -> Self {
        let mut dom = Arena::new();
        Self {
            root: dom.new_node(DOMNode::default()),
            dom,
            updates: vec![],
            update_sender: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DOMNode {
    pub kind: NodeKind,
    pub attrs: SmallVec<(String, String), 4>,
}

impl DOM {
    pub fn set_update_sender(&mut self, sender: tokio::sync::broadcast::Sender<Vec<DOMUpdate>>) {
        self.update_sender = Some(sender);
    }

    pub fn push_update(&mut self, update: DOMUpdate) {
        self.updates.push(update);
    }

    pub fn prepare_for_update(&mut self) {
        if !self.updates.is_empty() {
            warn!("DOM started update prep while updates still in the buffer.");
        }
        self.updates.clear();
    }

    pub fn finish_update(&mut self) -> Vec<DOMUpdate> {
        let batch = mem::take(&mut self.updates);
        if let Some(sender) = &self.update_sender {
            if !batch.is_empty() {
                let _ = sender.send(batch.clone());
            }
        }
        batch
    }

    pub fn root_id(&self) -> NodeId {
        self.root
    }

    pub fn append_element(&mut self, parent: NodeId, tag: String) -> NodeId {
        let id = self.dom.new_node(DOMNode { kind: NodeKind::Element { tag }, attrs: SmallVec::new() });
        parent.append(id, &mut self.dom);
        id
    }

    pub fn append_text(&mut self, parent: NodeId, text: String) -> NodeId {
        let id = self.dom.new_node(DOMNode { kind: NodeKind::Text { text }, attrs: SmallVec::new() });
        parent.append(id, &mut self.dom);
        id
    }

    // Create an unattached element node
    pub fn new_element(&mut self, tag: String) -> NodeId {
        self.dom.new_node(DOMNode { kind: NodeKind::Element { tag }, attrs: SmallVec::new() })
    }

    // Create an unattached text node
    pub fn new_text(&mut self, text: String) -> NodeId {
        self.dom.new_node(DOMNode { kind: NodeKind::Text { text }, attrs: SmallVec::new() })
    }

    // Append an existing node as a child
    pub fn append_child(&mut self, parent: NodeId, child: NodeId) {
        // Compute insertion position before appending
        let pos = self.children_len(parent);
        parent.append(child, &mut self.dom);
        // Record update depending on node kind
        if let Some(n) = self.dom.get(child) {
            match &n.get().kind {
                NodeKind::Element { tag } => {
                    self.updates.push(DOMUpdate::InsertElement { parent, node: child, tag: tag.clone(), pos });
                }
                NodeKind::Text { text } => {
                    self.updates.push(DOMUpdate::InsertText { parent, node: child, text: text.clone(), pos });
                }
                NodeKind::Document => {}
            }
        }
    }

    // Insert a node before the given sibling
    pub fn insert_before(&mut self, sibling: NodeId, new_node: NodeId) {
        // Determine parent and position index for the new insertion
        let parent = self.parent(sibling).unwrap_or(self.root);
        let pos = self.index_in_parent(sibling).unwrap_or(0);
        sibling.insert_before(new_node, &mut self.dom);
        if let Some(n) = self.dom.get(new_node) {
            match &n.get().kind {
                NodeKind::Element { tag } => {
                    self.updates.push(DOMUpdate::InsertElement { parent, node: new_node, tag: tag.clone(), pos });
                }
                NodeKind::Text { text } => {
                    self.updates.push(DOMUpdate::InsertText { parent, node: new_node, text: text.clone(), pos });
                }
                NodeKind::Document => {}
            }
        }
    }

    // Detach a node from its parent
    pub fn remove_from_parent(&mut self, node: NodeId) {
        node.detach(&mut self.dom);
        self.updates.push(DOMUpdate::RemoveNode { node });
    }

    // Move all children of `node` to be children of `new_parent`
    pub fn reparent_children(&mut self, node: NodeId, new_parent: NodeId) {
        let children: Vec<NodeId> = node.children(&self.dom).collect();
        for child in children {
            child.detach(&mut self.dom);
            new_parent.append(child, &mut self.dom);
        }
    }

    pub fn children(&self, node: NodeId) -> Vec<NodeId> {
        node.children(&self.dom).collect()
    }

    pub fn has_attr(&self, node: NodeId, name: &str) -> bool {
        self.dom
            .get(node)
            .map(|n| n.get().attrs.iter().any(|(n, _)| n == name))
            .unwrap_or(false)
    }

    pub fn get_tag(&self, node: NodeId) -> Option<&str> {
        self.dom.get(node).and_then(|n| match &n.get().kind {
            NodeKind::Element { tag } => Some(tag.as_str()),
            _ => None,
        })
    }

    pub fn get_text(&self, node: NodeId) -> Option<&str> {
        self.dom.get(node).and_then(|n| match &n.get().kind {
            NodeKind::Text { text } => Some(text.as_str()),
            _ => None,
        })
    }

    pub fn parent(&self, node: NodeId) -> Option<NodeId> {
        node.ancestors(&self.dom).skip(1).next()
    }

    pub fn index_in_parent(&self, node: NodeId) -> Option<usize> {
        let parent = self.parent(node)?;
        let mut idx = 0usize;
        for child in parent.children(&self.dom) {
            if child == node {
                return Some(idx);
            }
            idx += 1;
        }
        None
    }

    pub fn children_len(&self, node: NodeId) -> usize {
        node.children(&self.dom).count()
    }

    pub fn set_attr(&mut self, node: NodeId, name: String, value: String) {
        if let Some(n) = self.dom.get_mut(node) {
            n.get_mut().attrs.push((name.clone(), value.clone()));
            // Record update in the DOM
            self.updates.push(DOMUpdate::SetAttr { node, name, value });
        }
    }
}

#[derive(Debug, Clone)]
pub enum DOMUpdate {
    InsertElement { parent: NodeId, node: NodeId, tag: String, pos: usize },
    InsertText { parent: NodeId, node: NodeId, text: String, pos: usize },
    SetAttr { node: NodeId, name: String, value: String },
    RemoveNode { node: NodeId },
    EndOfDocument
}

pub trait DOMSubscriber {
    fn update(&mut self, update: DOMUpdate) -> Result<(), Error>;
}