use crate::url::stream_url;
use anyhow::{anyhow, Error};
use html::dom::updating::{DOMMirror, DOMSubscriber, DOMUpdate};
use html::dom::DOM;
use html::parser::HTMLParser;
use log::{trace, info};
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc};
use url::Url;
use css::CSSMirror;
use css::types::Stylesheet;
use layouter::Layouter;

pub struct HtmlPage {
    // If none, loading is finished. If some, still streaming.
    loader: Option<HTMLParser>,
    // The DOM of the page.
    dom: DOM,
    // Mirror that collects CSS from the DOM stream.
    css_mirror: DOMMirror<CSSMirror>,
    // Layouter mirror that maintains a layout tree from DOM updates.
    layouter_mirror: DOMMirror<Layouter>,
    // For sending updates to the DOM
    in_updater: mpsc::Sender<Vec<DOMUpdate>>,
    #[allow(dead_code)]
    url: Url,
}

impl HtmlPage {
    /// Create a new HtmlPage by streaming the content from the given URL
    pub async fn new(handle: &Handle, url: Url) -> Result<Self, Error> {
        // For updates from the DOM to subcomponents
        let (out_updater, out_receiver) = broadcast::channel(128);

        // For updates from subcomponents to the DOM
        let (in_updater, in_receiver) = mpsc::channel(128);

        // Create DOM first so it can assign a producer shard for NodeKey generation
        let mut dom = DOM::new(out_updater, in_receiver);
        let keyman = dom.register_parser_manager();

        let loader = HTMLParser::parse(
            handle,
            in_updater.clone(),
            keyman,
            stream_url(&url).await?,
            out_receiver,
        );

        // Create and attach the CSS mirror to observe DOM updates
        let css_mirror = DOMMirror::new(in_updater.clone(), dom.subscribe(), CSSMirror::new());
        // Create and attach the Layouter mirror to observe DOM updates
        let layouter_mirror = DOMMirror::new(in_updater.clone(), dom.subscribe(), Layouter::new());
        
        Ok(Self {
            loader: Some(loader),
            dom,
            css_mirror,
            layouter_mirror,
            in_updater,
            url
        })
    }

    /// Returns true once parsing has fully finalized and the loader has been consumed.
    /// This becomes true only after an update() call has observed the parser finished
    /// and awaited its completion.
    pub fn parsing_finished(&self) -> bool {
        self.loader.is_none()
    }

    pub async fn update(&mut self) -> Result<(), Error> {
        if let Some(true) = self.loader.as_ref().map(|l| l.is_finished()) {
            let loader = self
                .loader
                .take()
                .ok_or_else(|| anyhow!("Loader is finished and None!"))?;
            trace!("Loader finished, finalizing DOM");
            loader.finish().await?;
        }

        // Apply any pending DOM updates
        self.dom.update().await?;
        // Drain CSS mirror after DOM broadcast
        self.css_mirror.update().await?;

        // Forward current stylesheet to layouter
        let current_styles = self.css_mirror.mirror_mut().styles().clone();
        self.layouter_mirror.mirror_mut().set_stylesheet(current_styles);
        // Drain layouter updates after DOM broadcast
        self.layouter_mirror.update().await?;
        // Compute layout – can be used by renderer later
        let node_count = self.layouter_mirror.mirror_mut().compute_layout();
        info!("Layouter computed layout for {node_count} nodes");
        Ok(())
    }

    pub fn create_mirror<T: DOMSubscriber>(&self, mirror: T) -> DOMMirror<T> {
        DOMMirror::new(self.in_updater.clone(), self.dom.subscribe(), mirror)
    }

    /// Drain CSS mirror and return a snapshot clone of the collected stylesheet
    pub fn styles_snapshot(&mut self) -> Result<Stylesheet, Error> {
        // For blocking-thread callers, keep it non-async
        self.css_mirror.try_update_sync()?;
        Ok(self.css_mirror.mirror_mut().styles().clone())
    }

    /// Drain CSS mirror and return a snapshot clone of discovered external stylesheet URLs
    pub fn discovered_stylesheets_snapshot(&mut self) -> Result<Vec<String>, Error> {
        self.css_mirror.try_update_sync()?;
        Ok(self.css_mirror.mirror_mut().discovered_stylesheets().to_vec())
    }

    /// Return a JSON snapshot of the current DOM tree (deterministic schema for comparison)
    pub fn dom_json_snapshot_string(&self) -> String {
        self.dom.to_json_string()
    }
}
