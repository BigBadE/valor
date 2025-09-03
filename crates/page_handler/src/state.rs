use anyhow::{anyhow, Result as AnyhowResult};
use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::RcDom;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use url::Url;

pub struct HtmlPage {
    dom: RcDom,
    loader: Option<JoinHandle<()>>,
    url: Url,
}

impl HtmlPage {
    /// Create a new HtmlPage by streaming the content from the given URL
    pub async fn new(handle: &Handle, url: Url) -> AnyhowResult<Self> {
        let mut response = reqwest::get(url.clone()).await
            .map_err(|e| anyhow!("Failed to fetch URL {}: {}", url, e))?;
        
        if !response.status().is_success() {
            return Err(anyhow!(
                "Failed to fetch URL: {} (Status: {})", 
                url, 
                response.status()
            ));
        }
        
        // Create HTML parser for streaming
        let mut parser = parse_document(RcDom::default(), Default::default());
        
        // Stream response in chunks and parse progressively
        while let Some(chunk) = response.chunk().await
            .map_err(|e| anyhow!("Error reading response chunk: {}", e))? {
            let chunk_str = String::from_utf8_lossy(&chunk);
            parser.process(chunk_str.as_ref().into());
        }
        
        // Complete parsing and get the DOM
        let dom = parser.finish();
        
        Ok(Self { dom, loader: Some(handle.spawn(Self::parse_html(url.clone()))), url })
    }

    pub async fn parse_html(url: Url) {

    }
}