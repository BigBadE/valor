//! Page handler that coordinates HTML parsing and DOM tree management.

use futures::Stream;
use futures::StreamExt;
use rewrite_core::{Database, DomBroadcast, NodeId, Parser, Specificity, Subscriptions};
use rewrite_css::{CssParser, ParsedRule, Styler};
use rewrite_html::{DomTree, DomUpdate, HtmlParser, NodeData};
use std::sync::Arc;
use std::time::Instant;
use tokio::runtime::Runtime;
use tokio::task::LocalSet;

mod browser;
mod ua_stylesheet;

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
        let css_interner = self.tree.interner.clone();
        let styler = self.styler.clone();
        let subs = self.subscriptions.clone();

        self.runtime.block_on(async move {
            let local = LocalSet::new();

            local
                .run_until(async move {
                    let t0 = Instant::now();

                    // Load UA stylesheet first so its rules have the lowest
                    // source-order priority in the cascade.
                    let ua_styler = self.styler.clone();
                    let ua_interner = css_interner.clone();
                    let mut ua_parser = CssParser::new(
                        move |rule: ParsedRule| ua_styler.add_rule(rule),
                        ua_interner,
                    );
                    ua_parser.push_chunk(ua_stylesheet::UA_CSS).await;
                    ua_parser.finish().await;

                    let t1 = Instant::now();

                    // Buffer CSS text from <style> elements during HTML parsing.
                    // Each entry is a complete CSS chunk from a text node.
                    let css_chunks: Arc<std::sync::Mutex<Vec<String>>> =
                        Arc::new(std::sync::Mutex::new(Vec::new()));

                    let css_chunks_cb = css_chunks.clone();
                    let db = self.db.clone();

                    let mut parser = HtmlParser::new(
                        move |update| {
                            match &update {
                                DomUpdate::CreateNode(_) => {
                                    let node_id = tree.apply_update(update);
                                    styler.style_node(node_id);
                                    node_id
                                }
                                DomUpdate::AppendChild { parent, child } => {
                                    let parent = *parent;
                                    let child = *child;
                                    let node_id = tree.apply_update(update);
                                    db.relink_node(child);
                                    styler.restyle_node(child);

                                    // Buffer CSS text from <style> elements.
                                    if is_style_element(&tree, parent) {
                                        if let Some(css_text) = tree.text_content(child) {
                                            css_chunks_cb.lock().unwrap().push(css_text.to_owned());
                                        }
                                    }

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

                    let t2 = Instant::now();

                    // Feed buffered CSS into the streaming parser.
                    let chunks = css_chunks.lock().unwrap().clone();
                    if !chunks.is_empty() {
                        let styler_css = self.styler.clone();
                        let mut css_parser = CssParser::new(
                            move |rule: ParsedRule| styler_css.add_rule(rule),
                            css_interner,
                        );

                        for chunk in chunks {
                            css_parser.push_chunk(&chunk).await;
                        }

                        css_parser.finish().await;
                    }

                    let t3 = Instant::now();

                    // Flush low-confidence rules now that all stylesheets are loaded.
                    self.styler.flush();

                    let t4 = Instant::now();

                    // Timing output for debugging (only if slow)
                    let total = t4 - t0;
                    if total.as_secs() >= 1 {
                        eprintln!("      [load_html internal breakdown]");
                        eprintln!("        UA stylesheet parse:  {:>8.2?}", t1 - t0);
                        eprintln!("        HTML parse + styling: {:>8.2?}", t2 - t1);
                        eprintln!("        <style> CSS parse:    {:>8.2?}", t3 - t2);
                        eprintln!("        styler.flush():       {:>8.2?}", t4 - t3);
                    }
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

/// Check if a node is a `<style>` element.
fn is_style_element(tree: &DomTree, node: NodeId) -> bool {
    match tree.get_node(node) {
        Some(NodeData::Element { tag, .. }) => tree.interner.resolve(tag) == "style",
        _ => false,
    }
}

/// Subscriber wrapper that feeds CSS property notifications into the `Database`.
struct DatabaseSubscriber(Arc<Database>);

impl rewrite_core::Subscriber for DatabaseSubscriber {
    fn on_property(&self, node: rewrite_core::NodeId, property: &rewrite_css::Property<'static>) {
        // Don't store properties at their CSS initial value — the formula
        // system treats missing properties as having their default. Skipping
        // storage keeps the sparse trees genuinely sparse.
        if rewrite_core::is_css_initial_value(property) {
            return;
        }
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
