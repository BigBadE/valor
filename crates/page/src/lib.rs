//! Page handler that coordinates HTML parsing and DOM tree management.

use futures::Stream;
use futures::StreamExt;
use rewrite_core::{Database, DomBroadcast, Parser, Specificity, Subscriptions};
use rewrite_css::{CssParser, ParsedRule, Styler};
use rewrite_html::{DomTree, DomUpdate, HtmlParser};
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::task::LocalSet;

mod browser;

pub use browser::Browser;

/// Per-navigation page state.
pub struct Page<'br> {
    pub db: Arc<Database>,
    pub tree: Arc<DomTree>,
    pub styler: Arc<Styler>,
    pub subscriptions: Arc<Subscriptions>,
    runtime: &'br Runtime,
}

impl<'br> Page<'br> {
    pub fn new(
        runtime: &'br Runtime,
        tree: Arc<DomTree>,
        styler: Arc<Styler>,
        subscriptions: Arc<Subscriptions>,
    ) -> Self {
        let database = Arc::new(Database::new(tree.clone()));

        // Register the database as a subscriber so it receives all
        // property notifications from the Styler.
        subscriptions.add_subscriber(Box::new(DatabaseSubscriber(database.clone())));

        Self {
            db: database,
            tree,
            styler,
            subscriptions,
            runtime,
        }
    }

    /// Load HTML from a stream.
    pub fn load_html(&self, html_stream: impl Stream<Item = String> + Send + 'static) {
        let tree = self.tree.clone();
        let interner = self.tree.interner.clone();
        let styler = self.styler.clone();
        let subs = self.subscriptions.clone();

        self.runtime.block_on(async move {
            let local = LocalSet::new();

            local
                .run_until(async move {
                    let mut parser = HtmlParser::new(
                        move |update| {
                            match &update {
                                DomUpdate::CreateNode(_) => {
                                    // Apply update first to get node_id
                                    let node_id = tree.apply_update(update);
                                    // For CreateNode, we don't have parent yet
                                    // Parent comes with AppendChild
                                    styler.style_node(node_id);
                                    node_id
                                }
                                DomUpdate::AppendChild { parent, child } => {
                                    let parent = *parent;
                                    let child = *child;
                                    let node_id = tree.apply_update(update);
                                    // Re-match rules now that the node has a parent
                                    // (ancestor-dependent selectors like `div > p` can now match)
                                    styler.restyle_node(child);
                                    // Now we have parent info, broadcast
                                    subs.notify_dom(DomBroadcast::CreateNode {
                                        node: child,
                                        parent,
                                    });
                                    node_id
                                }
                            }
                        },
                        interner,
                    );

                    let mut stream = Box::pin(html_stream);
                    while let Some(chunk) = stream.next().await {
                        parser.process(&chunk);
                    }

                    parser.finish();
                })
                .await;
        });
    }

    /// Load and parse a CSS stylesheet from a stream.
    pub fn load_stylesheet(&self, css_stream: impl Stream<Item = String> + Send + 'static) {
        let interner = self.tree.interner.clone();
        let styler = self.styler.clone();
        let styler_flush = self.styler.clone();

        // Tokio handles the async stream, rayon handles the parsing
        self.runtime.spawn(async move {
            let mut parser =
                CssParser::new(move |rule: ParsedRule| styler.add_rule(rule), interner);

            let mut stream = Box::pin(css_stream);
            while let Some(chunk) = stream.next().await {
                parser.push_chunk(&chunk).await;
            }

            parser.finish().await;

            // Flush low-confidence rules now that parsing is complete
            styler_flush.flush();
        });
    }
}

/// Subscriber wrapper that feeds CSS property notifications into the `Database`.
struct DatabaseSubscriber(Arc<Database>);

impl rewrite_core::Subscriber for DatabaseSubscriber {
    fn on_property(&self, node: rewrite_core::NodeId, property: &rewrite_css::Property<'static>) {
        // Store with INLINE specificity — the Styler already resolved the
        // cascade, so the property we receive here is the current winner
        // and should always overwrite any previous value.
        self.0
            .set_property(node, property.clone(), Specificity::INLINE);
    }

    fn on_dom(&self, _update: DomBroadcast) {
        // DOM structure is handled by the tree itself; the Database
        // discovers parent relationships lazily via TreeAccess.
    }
}
