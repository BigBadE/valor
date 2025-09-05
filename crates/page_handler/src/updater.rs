use anyhow::{anyhow, Error};
use tokio::sync::broadcast::error::TryRecvError;
use tokio::sync::broadcast::Receiver;
use html::dom::{DOMSubscriber, DOMUpdate};

pub struct DOMUpdater<T: DOMSubscriber> {
    update_channel: Receiver<Vec<DOMUpdate>>,
    dom: T
}

impl<T: DOMSubscriber> DOMUpdater<T> {
    /// Updates the DOM with the updates on the channel, returning true if it's finished.
    pub async fn update(&mut self) -> Result<(), Error> {
        while let Some(updates) = match self.update_channel.try_recv() {
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
}