use anyhow::{anyhow, Error};
use html::dom::{DOMUpdate, DOM};
use html::parser::HTMLParser;
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::StreamExt;
use url::Url;

pub struct HtmlPage {
    // If none, loading is finished. If some, still streaming.
    loader: Option<HTMLParser>,
    // The DOM of the page.
    dom: DOM,
    // For sending updates to the DOM
    in_updater: mpsc::Sender<Vec<DOMUpdate>>,
    url: Url,
}

impl HtmlPage {
    /// Create a new HtmlPage by streaming the content from the given URL
    pub async fn new(handle: &Handle, url: Url) -> Result<Self, Error> {
        let response = reqwest::get(url.clone())
            .await
            .map_err(|e| anyhow!("Failed to fetch URL {}: {}", url, e))?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "Failed to fetch URL: {} (Status: {})",
                url,
                response.status()
            ));
        }

        // For updates from the DOM to subcomponents
        let (out_updater, _) = broadcast::channel(128);
        // For updates from subcomponents to the DOM
        let (in_updater, in_receiver) = mpsc::channel(128);
        
        Ok(Self {
            loader: Some(HTMLParser::parse(
                handle,
                updater.clone(),
                response
                    .bytes_stream()
                    .map(|res| res.map_err(|e| anyhow!(e))),
            )),
            dom: DOM::new(out_updater, in_receiver),
            in_updater,
            url,
        })
    }

    pub async fn update(&mut self) -> Result<(), Error> {
        // Handle finishing the loader
        if let Some(true) = self.loader.as_mut().and_then(|loader| Some(loader.is_finished())) {
            self.loader
                .take()
                .ok_or_else(|| anyhow!("Loader is finished and None!"))?
                .finish()
                .await?;
        }

        self.dom.update()?;

        Ok(())
    }
}
