mod html5ever_engine;

use js::{NodeKey, NodeKeyManager};
use crate::parser::html5ever_engine::Html5everEngine;
use anyhow::{anyhow, Error};
use bytes::Bytes;
use indextree::{Arena, NodeId};
use smallvec::SmallVec;
use std::collections::HashMap;
use log::{trace, warn};
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc};
use tokio::task;
use tokio::task::JoinHandle;
use tokio_stream::{Stream, StreamExt};
use js::{DOMMirror, DOMSubscriber, DOMUpdate};

/// This is the parser itself, the DOM has refs here, and is
/// responsible for sending DOM updates to the tree
pub struct HTMLParser {
    process_handle: JoinHandle<Result<(), Error>>,
}

#[derive(Debug, Clone, Default)]
pub enum ParserNodeKind {
    #[default]
    Document,
    Element { tag: String },
    Text { text: String },
}

#[derive(Debug, Clone, Default)]
pub struct ParserDOMNode {
    pub kind: ParserNodeKind,
    pub attrs: SmallVec<(String, String), 4>,
}

#[derive(Debug)]
pub struct ParserDOMMirror {
    dom: Arena<ParserDOMNode>,
    root: NodeId,
    in_updater: mpsc::Sender<Vec<DOMUpdate>>,
    // batch of DOM updates to be sent to runtime DOM
    current_batch: Vec<DOMUpdate>,
    // Stable NodeKey manager (maps local NodeId -> NodeKey and mints keys)
    keyman: NodeKeyManager<NodeId>,
    // Map stable NodeKey -> parser-local NodeId (for applying incoming DOM updates)
    id_map: HashMap<NodeKey, NodeId>,
}

impl ParserDOMMirror {
    pub fn root_id(&mut self) -> NodeId { self.root }

    fn key_of(&mut self, id: NodeId) -> NodeKey {
        self.keyman.key_of(id)
    }

    pub fn prepare_for_update(&mut self) {
        if !self.current_batch.is_empty() {
            warn!("Batch not empty before prepare_for_update!");
        }
        self.current_batch.clear();
    }

    pub fn push_update(&mut self, upd: DOMUpdate) { self.current_batch.push(upd); }

    pub fn finish_update(&mut self) -> Result<(), Error> { self.in_updater.try_send(std::mem::take(&mut self.current_batch)).map_err(|e| anyhow::anyhow!(e)) }

    pub fn new_element(&mut self, tag: String) -> NodeId {
        let id = self.dom.new_node(ParserDOMNode { kind: ParserNodeKind::Element { tag: tag.clone() }, attrs: SmallVec::new() });
        // We'll emit insertion when appended; creation itself has no update
        id
    }

    pub fn new_text(&mut self, text: String) -> NodeId {
        let id = self.dom.new_node(ParserDOMNode { kind: ParserNodeKind::Text { text: text.clone() }, attrs: SmallVec::new() });
        id
    }

    pub fn set_attr(&mut self, node: NodeId, name: String, value: String) {
        let key = self.key_of(node);
        if let Some(n) = self.dom.get_mut(node) {
            if let Some((_k, v)) = n.get_mut().attrs.iter_mut().find(|(k, _)| *k == name) {
                *v = value.clone();
            } else {
                n.get_mut().attrs.push((name.clone(), value.clone()));
            }
        }
        self.push_update(DOMUpdate::SetAttr { node: key, name, value });
    }

    pub fn has_attr(&self, node: NodeId, name: &str) -> bool {
        self.dom.get(node).map(|n| n.get().attrs.iter().any(|(k, _)| k == name)).unwrap_or(false)
    }

    pub fn append_child(&mut self, parent: NodeId, child: NodeId) {
        // Append in local arena
        let pos = parent.children(&self.dom).count();
        parent.append(child, &mut self.dom);
        let (tag, text) = match self.dom.get(child).map(|n| n.get().kind.clone()).unwrap_or_default() {
            ParserNodeKind::Element { tag } => (Some(tag), None),
            ParserNodeKind::Text { text } => (None, Some(text)),
            ParserNodeKind::Document => (Some(String::from("document")), None),
        };
        let parent_key = self.key_of(parent);
        let child_key = self.key_of(child);
        if let Some(tag) = tag { self.push_update(DOMUpdate::InsertElement { parent: parent_key, node: child_key, tag, pos }); }
        if let Some(text) = text { self.push_update(DOMUpdate::InsertText { parent: parent_key, node: child_key, text, pos }); }
    }

    pub fn insert_before(&mut self, sibling: NodeId, child: NodeId) {
        let parent = sibling.ancestors(&self.dom).nth(1).unwrap_or(self.root);
        let mut index = 0usize;
        for (i, c) in parent.children(&self.dom).enumerate() { if c == sibling { index = i; break; } }
        sibling.insert_before(child, &mut self.dom);
        let (tag, text) = match self.dom.get(child).map(|n| n.get().kind.clone()).unwrap_or_default() {
            ParserNodeKind::Element { tag } => (Some(tag), None),
            ParserNodeKind::Text { text } => (None, Some(text)),
            ParserNodeKind::Document => (Some(String::from("document")), None),
        };
        let parent_key = self.key_of(parent);
        let child_key = self.key_of(child);
        if let Some(tag) = tag { self.push_update(DOMUpdate::InsertElement { parent: parent_key, node: child_key, tag, pos: index }); }
        if let Some(text) = text { self.push_update(DOMUpdate::InsertText { parent: parent_key, node: child_key, text, pos: index }); }
    }

    pub fn remove_from_parent(&mut self, node: NodeId) {
        node.detach(&mut self.dom);
        let key = self.key_of(node);
        self.push_update(DOMUpdate::RemoveNode { node: key });
    }

    pub fn reparent_children(&mut self, node: NodeId, new_parent: NodeId) {
        // Move children one by one
        let children: Vec<NodeId> = node.children(&self.dom).collect();
        for child in children {
            self.append_child(new_parent, child);
        }
    }

    // Incoming DOM updates application (subscriber side): no re-emission
    fn ensure_node(&mut self, key: NodeKey, kind: Option<ParserNodeKind>) -> NodeId {
        if let Some(&id) = self.id_map.get(&key) { return id; }
        let nid = self.dom.new_node(ParserDOMNode { kind: kind.unwrap_or_default(), attrs: SmallVec::new() });
        self.id_map.insert(key, nid);
        nid
    }

    fn map_parent(&mut self, key: NodeKey) -> NodeId {
        if let Some(&id) = self.id_map.get(&key) { id } else { self.id_map.insert(key, self.root); self.root }
    }
}

impl DOMSubscriber for ParserDOMMirror {
    fn apply_update(&mut self, update: DOMUpdate) -> Result<(), Error> {
        use DOMUpdate::*;
        match update {
            InsertElement { parent, node, tag, pos } => {
                let parent_id = self.map_parent(parent);
                let child_id = self.ensure_node(node, Some(ParserNodeKind::Element { tag }));
                // Detach if attached
                if self.dom.get(child_id).and_then(|n| n.parent()).is_some() { child_id.detach(&mut self.dom); }
                let count = parent_id.children(&self.dom).count();
                if pos >= count { parent_id.append(child_id, &mut self.dom); }
                else if let Some(sib) = parent_id.children(&self.dom).nth(pos) { sib.insert_before(child_id, &mut self.dom); }
                else { parent_id.append(child_id, &mut self.dom); }
            }
            InsertText { parent, node, text, pos } => {
                let parent_id = self.map_parent(parent);
                let child_id = self.ensure_node(node, Some(ParserNodeKind::Text { text: text.clone() }));
                if let Some(n) = self.dom.get_mut(child_id) {
                    if let ParserNodeKind::Text { text: t } = &mut n.get_mut().kind { *t = text.clone(); }
                }
                if self.dom.get(child_id).and_then(|n| n.parent()).is_some() { child_id.detach(&mut self.dom); }
                let count = parent_id.children(&self.dom).count();
                if pos >= count { parent_id.append(child_id, &mut self.dom); }
                else if let Some(sib) = parent_id.children(&self.dom).nth(pos) { sib.insert_before(child_id, &mut self.dom); }
                else { parent_id.append(child_id, &mut self.dom); }
            }
            SetAttr { node, name, value } => {
                let id = self.ensure_node(node, None);
                if let Some(n) = self.dom.get_mut(id) {
                    let attrs = &mut n.get_mut().attrs;
                    if let Some((_, v)) = attrs.iter_mut().find(|(k, _)| *k == name) { *v = value; }
                    else { attrs.push((name, value)); }
                }
            }
            RemoveNode { node } => {
                if let Some(&id) = self.id_map.get(&node) { id.detach(&mut self.dom); }
            }
            EndOfDocument => {}
        }
        Ok(())
    }
}

impl HTMLParser {
    pub fn parse<S>(handle: &Handle, in_updater: mpsc::Sender<Vec<DOMUpdate>>, mut keyman: NodeKeyManager<NodeId>, byte_stream: S,
                    dom_updates: broadcast::Receiver<Vec<DOMUpdate>>, script_tx: tokio::sync::mpsc::UnboundedSender<String>) -> Self
    where
        S: Stream<Item = Result<Bytes, Error>> + Send + Unpin + 'static,
    {
        let mut dom = Arena::new();
        let root = dom.new_node(ParserDOMNode::default());
        // Seed the manager with the parser-local root mapping
        keyman.seed(root, NodeKey::ROOT);
        let mut id_map = HashMap::new();
        id_map.insert(NodeKey::ROOT, root);
        // Clone the sender so the DOMMirror wrapper can send if needed
        let mirror_out = in_updater.clone();
        let mirror = ParserDOMMirror {
            root,
            dom,
            in_updater,
            current_batch: Vec::with_capacity(128),
            keyman,
            id_map,
        };

        // Wrap the parser mirror with DOMMirror so it can receive runtime DOM updates
        let dom_mirror = DOMMirror::new(mirror_out, dom_updates, mirror);
        let process_handle = handle.spawn(HTMLParser::process(dom_mirror, byte_stream, script_tx));
        HTMLParser { process_handle }
    }

    pub async fn process<S: Stream<Item = Result<Bytes, Error>> + Send + Unpin + 'static>(
        dom: DOMMirror<ParserDOMMirror>,
        mut byte_stream: S,
        script_tx: tokio::sync::mpsc::UnboundedSender<String>,
    ) -> Result<(), Error> {
        trace!("Started processing!");
        // This function is a bit complicated due to html5ever not being Send
        let parser_worker = {
            let (tx, mut rx) = mpsc::channel::<Result<Bytes, Error>>(128);

            // Blocking parser worker: owns the non-Send html5ever engine
            let parser_worker = task::spawn_blocking(move || {
                let mut engine = Html5everEngine::new(dom, script_tx);
                loop {
                    engine.try_update_sync()?;
                    match rx.blocking_recv() {
                        Some(item) => {
                            let chunk = match item {
                                Ok(b) => b,
                                Err(e) => return Err(e)
                            };
                            let text = String::from_utf8_lossy(&chunk);
                            engine.push(text.as_ref());
                        }
                        None => { break; }
                    }
                }
                trace!("Finalizing parser");
                engine.finalize();
                Ok::<(), Error>(())
            });

            while let Some(item) = byte_stream.next().await {
                if tx.send(item).await.is_err() {
                    break;
                }
            }
            parser_worker
        };
        trace!("Done reading content in parser");
        parser_worker.await??;
        Ok(())
    }

    pub fn is_finished(&self) -> bool {
        self.process_handle.is_finished()
    }

    pub async fn finish(self) -> Result<(), Error> {
        if !self.process_handle.is_finished() {
            return Err(anyhow!("Expected process to be finished, but it wasn't!"));
        }
        self.process_handle.await?
    }
}
