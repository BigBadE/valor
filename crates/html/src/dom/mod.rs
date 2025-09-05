use anyhow::Error;
use indextree::{Arena, NodeId};
use smallvec::SmallVec;
use tokio::sync::{broadcast, mpsc};

#[derive(Debug, Clone, Default)]
pub enum NodeKind {
    #[default]
    Document,
    Element { tag: String },
    Text { text: String },
}

#[derive(Debug)]
pub struct DOM {
    dom: Arena<DOMNode>,
    root: NodeId,
    update_sender: broadcast::Sender<Vec<DOMUpdate>>,
    in_receiver: mpsc::Receiver<Vec<DOMUpdate>>,
}

impl DOM {
    pub fn new(out_updater: broadcast::Sender<Vec<DOMUpdate>>,
               in_receiver: mpsc::Receiver<Vec<DOMUpdate>>) -> Self {
        let mut dom = Arena::new();
        Self {
            root: dom.new_node(DOMNode::default()),
            dom,
            update_sender: out_updater,
            in_receiver
        }
    }

    pub async fn update(&mut self) -> Result<(), Error> {
        while let Ok(batch) = self.in_receiver.try_recv() {
            for update in &batch {
                self.apply_update(update);
            }
            // Send update to mirrors
            self.update_sender.send(batch)?;
        }
        Ok(())
    }

    fn apply_update(&mut self, update: &DOMUpdate) {
        use DOMUpdate::*;

        match update {
            InsertElement { parent, node, tag, pos } => {

            }
            InsertText { parent, node, text, pos } => {

            }
            SetAttr { node, name, value } => {

            }
            RemoveNode { node } => {

            }
            EndOfDocument => {

            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DOMNode {
    pub kind: NodeKind,
    pub attrs: SmallVec<(String, String), 4>,
}

impl DOM {

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