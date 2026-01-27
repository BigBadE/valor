//! Page handler that coordinates HTML parsing and DOM tree management.

use futures::stream::Stream;
use rewrite_core::*;
use rewrite_css::{CategorizedRules, CssUpdate};
use rewrite_html::{DomTree, DomUpdate};
use std::sync::mpsc;

mod parser;

/// Page handler that manages the DOM tree.
pub struct Page {
    db: Database,
    tree: DomTree,
    css_rules: CategorizedRules,
    work_queue: WorkQueue,
    dom_rx: mpsc::Receiver<DomUpdate>,
    dom_tx: mpsc::Sender<DomUpdate>,
    css_rx: mpsc::Receiver<CssUpdate>,
    css_tx: mpsc::Sender<CssUpdate>,
    runtime: tokio::runtime::Runtime,
}

impl Page {
    /// Create a new empty page.
    pub fn new() -> Self {
        let (dom_tx, dom_rx) = mpsc::channel();
        let (css_tx, css_rx) = mpsc::channel();
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");

        Self {
            db: Database::new(),
            tree: DomTree::new(),
            css_rules: CategorizedRules::default(),
            work_queue: WorkQueue::new_with_workers(),
            dom_rx,
            dom_tx,
            css_rx,
            css_tx,
            runtime,
        }
    }

    /// Navigate to a new page by parsing HTML from a stream.
    /// Drops all previous work and state, then starts parsing the new page.
    pub fn navigate(&mut self, html_stream: impl Stream<Item = String> + Send + 'static) {
        // Drop all previous state
        self.tree = DomTree::new();
        self.css_rules = CategorizedRules::default();
        self.db = Database::new();

        // Clear all pending work from the queue
        self.work_queue.clear();

        // Recreate channels to invalidate any old senders from previous navigation
        // This ensures long-running tasks from the old page can't send updates
        let (dom_tx, dom_rx) = mpsc::channel();
        let (css_tx, css_rx) = mpsc::channel();
        self.dom_rx = dom_rx;
        self.dom_tx = dom_tx.clone();
        self.css_rx = css_rx;
        self.css_tx = css_tx;

        // Spawn streaming HTML parser
        parser::spawn_streaming_parser(&self.runtime, html_stream, move |chunk_rx| {
            use html5ever::tendril::TendrilSink;
            use html5ever::{ParseOpts, parse_document};
            use rewrite_html::TreeBuilder;

            let tree_builder = TreeBuilder::new(dom_tx);
            let opts = ParseOpts::default();
            let mut parser = parse_document(tree_builder, opts);

            // Process chunks as they arrive from the channel
            while let Ok(chunk) = chunk_rx.recv() {
                parser.process(chunk.into());
            }

            // Finish parsing when channel closes
            let _ = parser.finish();
        });
    }

    /// Load and parse a CSS stylesheet from a stream.
    pub fn load_stylesheet(&mut self, css_stream: impl Stream<Item = String> + Send + 'static) {
        let css_tx = self.css_tx.clone();

        // Spawn streaming CSS parser
        parser::spawn_streaming_parser(&self.runtime, css_stream, move |chunk_rx| {
            use rewrite_css::StreamingCssParser;

            let mut parser = StreamingCssParser::new(css_tx);

            // Process chunks as they arrive from the channel
            while let Ok(chunk) = chunk_rx.recv() {
                parser.feed(&chunk);
            }

            // Finish parsing when channel closes
            parser.finish();
        });
    }

    /// Poll for updates from workers.
    /// Should be called regularly to process DOM and CSS updates.
    pub fn poll_updates(&mut self) {
        // Process DOM updates
        while let Ok(dom_update) = self.dom_rx.try_recv() {
            // Apply DOM update to tree
            match dom_update {
                DomUpdate::CreateNode { id, data } => {
                    self.tree.set_node_data(id, data);
                }
                DomUpdate::AppendChild { parent, child } => {
                    self.tree.append_child(parent, child);
                }
            }
            // TODO: Enqueue style matching for new nodes
        }

        // Process CSS updates
        while let Ok(css_update) = self.css_rx.try_recv() {
            match css_update {
                CssUpdate::Rules(rules) => {
                    self.css_rules.merge(rules);
                    // TODO: Enqueue style matching for affected nodes
                }
            }
        }
    }

    /// Get the database.
    pub fn database(&self) -> &Database {
        &self.db
    }

    /// Get the DOM tree.
    pub fn tree(&self) -> &DomTree {
        &self.tree
    }

    /// Get the work queue.
    pub fn work_queue(&self) -> &WorkQueue {
        &self.work_queue
    }
}

impl Default for Page {
    fn default() -> Self {
        Self::new()
    }
}
