use crate::parser::ParserDOMMirror;
use crate::parser::sink::ValorSink;
use anyhow::Error;
use core::cell::RefMut;
use html5ever::parse_document;
use html5ever::tendril::StrTendril;
use html5ever::tendril::TendrilSink as _;
use html5ever::{ParseOpts, Parser};
use js::{DOMMirror, DOMUpdate};
use tokio::sync::mpsc::UnboundedSender;
use url::Url;

use super::ScriptJob;

/// HTML5 parser engine using html5ever.
pub struct Html5everEngine {
    /// The underlying html5ever parser.
    parser: Parser<ValorSink>,
}

impl Html5everEngine {
    /// Gets a mutable reference to the DOM mirror.
    fn mirror_mut(&self) -> RefMut<'_, DOMMirror<ParserDOMMirror>> {
        self.parser.tokenizer.sink.sink.dom().borrow_mut()
    }

    /// Creates a new HTML5 parser engine.
    pub fn new(
        dom: DOMMirror<ParserDOMMirror>,
        script_tx: UnboundedSender<ScriptJob>,
        base_url: Url,
    ) -> Self {
        let sink = ValorSink::new(dom, script_tx, base_url);
        let parser = parse_document(sink, ParseOpts::default());
        Self { parser }
    }

    /// Try to synchronously update the DOM mirror.
    ///
    /// # Errors
    /// Returns an error if the update fails.
    pub fn try_update_sync(&self) -> Result<(), Error> {
        self.mirror_mut().try_update_sync()
    }

    /// Push a chunk of HTML to the parser.
    pub fn push(&mut self, chunk: &str) {
        {
            let mut dom = self.mirror_mut();
            dom.mirror_mut().prepare_for_update();
        };
        self.parser.process(StrTendril::from(chunk));
        {
            let mut dom = self.mirror_mut();
            drop(dom.mirror_mut().finish_update());
        }
    }

    /// Finalize the parser and flush any pending updates.
    pub fn finalize(&self) {
        {
            let mut dom = self.mirror_mut();
            dom.mirror_mut().prepare_for_update();
        };
        // Flush any deferred classic scripts before signaling EndOfDocument so they run before DOMContentLoaded
        self.parser.tokenizer.sink.sink.flush_deferred();
        self.parser.tokenizer.end();
        {
            let mut dom = self.mirror_mut();
            dom.mirror_mut().push_update(DOMUpdate::EndOfDocument);
            drop(dom.mirror_mut().finish_update());
        }
    }
}
