mod html5ever_engine;

use crate::dom::{DOMNode, DOMUpdate, DOM};
use crate::parser::html5ever_engine::Html5everEngine;
use anyhow::{anyhow, Error};
use bytes::Bytes;
use indextree::{Arena, NodeId};
use smallvec::SmallVec;
use tokio::runtime::Handle;
use tokio::sync::{broadcast, broadcast::Sender, mpsc};
use tokio::task::JoinHandle;
use tokio_stream::{Stream, StreamExt};

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
}

impl HTMLParser {
    pub fn parse<S>(handle: &Handle, in_updater: Sender<Vec<DOMUpdate>>, byte_stream: S) -> Self
    where
        S: Stream<Item = Result<Bytes, Error>> + Send + Unpin + 'static,
    {
        let process_handle = handle.spawn(HTMLParser::process(ParserDOMMirror {

            in_updater
        }, byte_stream));
        HTMLParser { process_handle }
    }

    pub async fn process<S: Stream<Item = Result<Bytes, Error>> + Send + Unpin + 'static>(
        in_updater: Sender<Vec<DOMUpdate>>,
        mut byte_stream: S,
    ) -> Result<DOM, Error> {
        // Bridge async stream into a blocking worker so !Send html5ever stays off async threads.
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Bytes>(64);
        let worker_updater = updater.clone();
        let worker = tokio::task::spawn_blocking(move || {
            let mut dom = DOM::default();
            dom.set_update_sender(worker_updater);
            let mut engine = Html5everEngine::new(&mut dom);
            // Receive chunks and process streaming updates
            while let Some(chunk) = rx.blocking_recv() {
                let text = String::from_utf8_lossy(&chunk);
                engine.push(text.as_ref());
            }
            // Finalize parser and emit EndOfDocument via DOM
            engine.finalize();
            dom
        });

        // Forward incoming async chunks to the worker task
        while let Some(chunk) = byte_stream.next().await {
            let chunk = chunk?;
            if tx.send(chunk).await.is_err() {
                break;
            }
        }
        drop(tx);
        let dom = worker.await.map_err(|_| anyhow!("worker task panicked"))?;
        Ok(dom)
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
