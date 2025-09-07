use anyhow::Error;
use tokio::sync::{broadcast, mpsc};
use crate::dom::NodeKey;

/// Generic mirror that can apply incoming DOM updates and send changes back to the DOM runtime.
pub struct DOMMirror<T: DOMSubscriber> {
    in_updater: broadcast::Receiver<Vec<DOMUpdate>>,
    out_updater: mpsc::Sender<Vec<DOMUpdate>>,
    mirror: T,
}

impl<T: DOMSubscriber> DOMMirror<T> {
    pub fn new(
        out_updater: mpsc::Sender<Vec<DOMUpdate>>,
        in_updater: broadcast::Receiver<Vec<DOMUpdate>>,
        mirror: T,
    ) -> Self {
        Self {
            in_updater,
            out_updater,
            mirror,
        }
    }

    /// Updates the DOM with the updates on the channel, returning true if it's finished.
    pub async fn update(&mut self) -> Result<(), Error> {
        use tokio::sync::broadcast::error::TryRecvError;
        while let Some(updates) = match self.in_updater.try_recv() {
            Ok(updates) => Ok::<_, Error>(Some(updates)),
            Err(TryRecvError::Closed) => {
                return Err(anyhow::anyhow!("Recv channel was closed before document ended!"));
            }
            _ => Ok(None),
        }? {
            for update in updates {
                self.mirror.apply_update(update)?;
            }
        }
        Ok(())
    }

    /// Synchronous, non-async variant for draining pending updates (for blocking threads)
    pub fn try_update_sync(&mut self) -> Result<(), Error> {
        use tokio::sync::broadcast::error::TryRecvError;
        loop {
            match self.in_updater.try_recv() {
                Ok(batch) => {
                    for update in batch {
                        self.mirror.apply_update(update)?;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Lagged(_)) => continue,
                Err(TryRecvError::Closed) => {
                    return Err(anyhow::anyhow!("Recv channel was closed before document ended!"));
                }
            }
        }
        Ok(())
    }

    /// Access the inner mirror mutably (engine-level integration)
    pub fn mirror_mut(&mut self) -> &mut T { &mut self.mirror }

    pub async fn send_dom_change(&mut self, changes: Vec<DOMUpdate>) -> Result<(), Error> {
        self.out_updater.send(changes).await?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum DOMUpdate {
    InsertElement {
        parent: NodeKey,
        node: NodeKey,
        tag: String,
        pos: usize,
    },
    InsertText {
        parent: NodeKey,
        node: NodeKey,
        text: String,
        pos: usize,
    },
    SetAttr {
        node: NodeKey,
        name: String,
        value: String,
    },
    RemoveNode {
        node: NodeKey,
    },
    EndOfDocument,
}

pub trait DOMSubscriber {
    fn apply_update(&mut self, update: DOMUpdate) -> Result<(), Error>;
}