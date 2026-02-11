//! HTML parser implementing the Parser trait.

use crate::builder::TreeBuilder;
use crate::types::DomUpdate;
use html5ever::tendril::TendrilSink;
use html5ever::{ParseOpts, parse_document};
use lasso::ThreadedRodeo;
use rewrite_core::{NodeId, Parser};
use std::sync::Arc;

/// Streaming HTML parser that emits DomUpdate events.
pub struct HtmlParser<F: Fn(DomUpdate) -> NodeId> {
    inner: html5ever::Parser<TreeBuilder<F>>,
}

impl<F: Fn(DomUpdate) -> NodeId + Send + 'static> Parser for HtmlParser<F> {
    type Output = DomUpdate;
    type Response = NodeId;
    type Callback = F;

    fn new(callback: Self::Callback, interner: Arc<ThreadedRodeo>) -> Self {
        let tree_builder = TreeBuilder::new(callback, interner);
        let inner = parse_document(tree_builder, ParseOpts::default());
        Self { inner }
    }

    fn process(&mut self, chunk: &str) {
        self.inner.process(chunk.into());
    }

    fn finish(self) {
        self.inner.finish();
    }
}
