use anyhow::{anyhow, Error};
use tokio::sync::{broadcast, mpsc};
use tokio::sync::broadcast::error::TryRecvError;
use html::dom::{DOMSubscriber, DOMUpdate};

pub struct DOMUpdater<T: DOMSubscriber> {
    in_updater: broadcast::Receiver<Vec<DOMUpdate>>,
    out_updater: mpsc::Sender<Vec<DOMUpdate>>,
    dom: T
}

impl<T: DOMSubscriber> DOMUpdater<T> {
    /// Updates the DOM with the updates on the channel, returning true if it's finished.
    pub async fn update(&mut self) -> Result<(), Error> {
        while let Some(updates) = match self.in_updater.try_recv() {
            Ok(updates) => Ok::<_, Error>(Some(updates)),
            Err(TryRecvError::Closed) => return Err(anyhow!("Recv channel was closed before document ended!")),
            _ => Ok(None)
        }? {
            for update in updates {
                self.dom.update(update)?;
            }
        }
        Ok(())
    }

    pub async fn send_dom_change(&mut self, changes: Vec<DOMUpdate>) -> Result<(), Error> {
        self.out_updater.send(changes).await?;
        Ok(())
    }
}