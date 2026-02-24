//! Shared browser infrastructure.

use crate::Page;
use lasso::ThreadedRodeo;
use rewrite_core::Subscriptions;
use rewrite_css::Styler;
use rewrite_html::DomTree;
use rewrite_renderer::Renderer;
use std::sync::Arc;
use tokio::runtime::Runtime;

/// Shared browser infrastructure (reused across navigations).
pub struct Browser {
    interner: Arc<ThreadedRodeo>,
    runtime: Runtime,
    subscriptions: Arc<Subscriptions>,
}

impl Default for Browser {
    fn default() -> Self {
        Self {
            interner: Arc::default(),
            runtime: Runtime::new().expect("failed to create tokio runtime"),
            subscriptions: Arc::new(Subscriptions::new()),
        }
    }
}

impl Browser {
    /// Create a new browser with the given subscriptions.
    pub fn new(subscriptions: Arc<Subscriptions>) -> Self {
        Self {
            interner: Arc::default(),
            runtime: Runtime::new().expect("failed to create tokio runtime"),
            subscriptions,
        }
    }

    /// Get the subscriptions for this browser.
    pub fn subscriptions(&self) -> &Arc<Subscriptions> {
        &self.subscriptions
    }

    /// Create a new page with its renderer.
    /// The renderer is automatically registered as a subscriber.
    pub fn new_page(&self) -> (Page<'_>, Arc<Renderer>) {
        let tree = Arc::new(DomTree::new(self.interner.clone()));
        let styler = Arc::new(Styler::new(tree.clone(), self.subscriptions.clone()));
        let page = Page::new(&self.runtime, tree, styler, self.subscriptions.clone());

        let renderer = Arc::new(Renderer::new(page.styler.clone(), page.db.clone()));

        // Register renderer as subscriber
        self.subscriptions
            .add_subscriber(Box::new(RendererSubscriber(renderer.clone())));

        (page, renderer)
    }

    /// Create a new page without a renderer.
    /// The caller can register their own subscriber via `subscriptions()`.
    pub fn new_page_headless(&self) -> Page<'_> {
        let tree = Arc::new(DomTree::new(self.interner.clone()));
        let styler = Arc::new(Styler::new(tree.clone(), self.subscriptions.clone()));
        Page::new(&self.runtime, tree, styler, self.subscriptions.clone())
    }
}

/// Wrapper to implement Subscriber for Arc<Renderer>.
struct RendererSubscriber(Arc<Renderer>);

impl rewrite_core::Subscriber for RendererSubscriber {
    fn on_property(&self, node: rewrite_core::NodeId, property: &rewrite_css::Property<'static>) {
        self.0.on_property(node, property);
    }

    fn on_dom(&self, update: rewrite_core::DomBroadcast) {
        self.0.on_dom(update);
    }
}
