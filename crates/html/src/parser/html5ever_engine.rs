use crate::parser::{ParserDOMMirror, ScriptJob, ScriptKind};
use alloc::borrow::Cow;
use anyhow::Error;
use core::cell::{RefCell, RefMut};
use core::mem::take;
use html5ever::parse_document;
use html5ever::tendril::StrTendril;
use html5ever::tendril::TendrilSink as _;
use html5ever::tree_builder::{ElemName, ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::{Attribute, LocalName, Namespace, ParseOpts, Parser, QualName, local_name};
use indextree::NodeId;
use js::{DOMMirror, DOMUpdate};
use std::collections::HashMap;
use std::fs::read_to_string;
use tokio::sync::mpsc::UnboundedSender;
use url::Url;

/// Helper function to get HTML namespace.
#[inline]
fn html_namespace() -> Namespace {
    html5ever::ns!(html)
}

/// Owned element name for `TreeSink` implementation.
#[derive(Debug, Clone)]
pub struct OwnedElemName {
    /// Namespace of the element.
    namespace: Namespace,
    /// Local name of the element.
    local: LocalName,
}

impl ElemName for OwnedElemName {
    fn ns(&self) -> &Namespace {
        &self.namespace
    }
    fn local_name(&self) -> &LocalName {
        &self.local
    }
}

/// `TreeSink` implementation that writes directly into our DOM updates via the parser mirror
pub struct ValorSink {
    /// The DOM mirror for parser updates.
    dom: RefCell<DOMMirror<ParserDOMMirror>>,
    /// Mapping of node IDs to their qualified names.
    element_names: RefCell<HashMap<NodeId, QualName>>,
    /// Channel for sending script jobs to the executor.
    script_tx: UnboundedSender<ScriptJob>,
    /// Base URL for resolving relative URLs.
    base_url: Url,
    /// Queue of deferred scripts.
    deferred_scripts: RefCell<Vec<ScriptJob>>,
    /// Track script state by node id.
    script_nodes: RefCell<HashMap<NodeId, ScriptState>>,
}

/// State tracking for script elements during parsing.
#[derive(Clone, Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Script state flags directly mirror HTML5 spec attributes"
)]
struct ScriptState {
    /// Whether the script has a src attribute.
    has_src: bool,
    /// Buffer for inline script content.
    buffer: String,
    /// Whether the script has defer attribute.
    defer: bool,
    /// Whether the script has async attribute.
    async_attr: bool,
    /// Value of the src attribute if present.
    src_value: Option<String>,
    /// Whether the script is a module.
    is_module: bool,
}

impl ValorSink {
    /// Creates a new `ValorSink`.
    pub fn new(
        dom: DOMMirror<ParserDOMMirror>,
        script_tx: UnboundedSender<ScriptJob>,
        base_url: Url,
    ) -> Self {
        Self {
            element_names: RefCell::new(HashMap::new()),
            script_tx,
            base_url,
            deferred_scripts: RefCell::new(Vec::new()),
            script_nodes: RefCell::new(HashMap::new()),
            dom: RefCell::new(dom),
        }
    }

    /// Gets access to the DOM mirror (crate-visible).
    pub(crate) const fn dom(&self) -> &RefCell<DOMMirror<ParserDOMMirror>> {
        &self.dom
    }

    /// Fetches script source from a URL.
    fn fetch_script_source(&self, src_value: &str) -> Option<String> {
        // Resolve against base URL; support file:// scheme for now.
        let resolved = self
            .base_url
            .join(src_value)
            .ok()
            .or_else(|| Url::parse(src_value).ok());
        if let Some(url) = resolved {
            if url.scheme() == "file" {
                if let Ok(path) = url.to_file_path()
                    && let Ok(data) = read_to_string(path)
                {
                    return Some(data);
                }
            } else {
                // TODO: implement http/https in async host path; ignore for now
            }
        }
        None
    }

    /// Flushes all deferred scripts to the script executor.
    fn flush_deferred(&self) {
        let mut queue = self.deferred_scripts.borrow_mut();
        let drained: Vec<ScriptJob> = take(&mut *queue);
        drop(queue);
        for job in drained {
            drop(self.script_tx.send(job));
        }
    }
}

impl TreeSink for ValorSink {
    type Handle = NodeId;
    type Output = ();
    type ElemName<'elem>
        = OwnedElemName
    where
        Self: 'elem;

    fn finish(self) -> Self::Output {}

    fn parse_error(&self, _msg: Cow<'static, str>) {}

    fn get_document(&self) -> Self::Handle {
        self.dom.borrow_mut().mirror_mut().root_id()
    }

    fn elem_name<'elem>(&'elem self, target: &'elem Self::Handle) -> Self::ElemName<'elem> {
        if let Some(qual_name) = self.element_names.borrow().get(target) {
            return OwnedElemName {
                namespace: qual_name.ns.clone(),
                local: qual_name.local.clone(),
            };
        }
        // Fallback to a reasonable default. In practice, we should always have a name
        // for elements created via create_element.
        OwnedElemName {
            namespace: html_namespace!(html),
            local: local_name!("div"),
        }
    }

    fn create_element(
        &self,
        name: QualName,
        attrs: Vec<Attribute>,
        _flags: ElementFlags,
    ) -> Self::Handle {
        let id = {
            let mut dom = self.dom.borrow_mut();
            let domm = dom.mirror_mut();
            domm.new_element(name.local.to_string())
        };
        // Track the element's qualified name for correct elem_name reporting
        self.element_names.borrow_mut().insert(id, name.clone());
        let is_script = name.ns == html_namespace!(html) && name.local == local_name!("script");
        let mut state = is_script.then(|| ScriptState {
            has_src: false,
            buffer: String::new(),
            defer: false,
            async_attr: false,
            src_value: None,
            is_module: false,
        });
        for attr in attrs {
            let local = attr.name.local.to_string();
            if let Some(ref mut script_state) = state {
                if attr.name.local.eq(&local_name!("src")) {
                    script_state.has_src = true;
                    script_state.src_value = Some(attr.value.to_string());
                }
                if attr.name.local.eq(&local_name!("defer")) {
                    script_state.defer = true;
                }
                if attr.name.local.eq(&local_name!("async")) {
                    script_state.async_attr = true;
                }
                if attr.name.local.eq(&local_name!("type"))
                    && attr.value.to_ascii_lowercase() == "module"
                {
                    script_state.is_module = true;
                }
            }
            let mut dom = self.dom.borrow_mut();
            let domm = dom.mirror_mut();
            domm.set_attr(id, local, attr.value.to_string());
        }
        if let Some(script_state) = state {
            self.script_nodes.borrow_mut().insert(id, script_state);
        }
        id
    }

    fn create_comment(&self, _text: StrTendril) -> Self::Handle {
        // Ignore comment content; produce an empty text node so it doesn't affect layout
        self.dom.borrow_mut().mirror_mut().new_text(String::new())
    }

    fn create_pi(&self, _target: StrTendril, data: StrTendril) -> Self::Handle {
        // Represent processing instructions as text nodes for now
        self.dom
            .borrow_mut()
            .mirror_mut()
            .new_text(data.to_string())
    }

    fn append(&self, parent: &Self::Handle, child: NodeOrText<Self::Handle>) {
        match child {
            NodeOrText::AppendNode(node) => {
                self.dom
                    .borrow_mut()
                    .mirror_mut()
                    .append_child(*parent, node);
            }
            NodeOrText::AppendText(text) => {
                // If appending under a <script> without src, collect the text
                if let Some(entry) = self.script_nodes.borrow_mut().get_mut(parent)
                    && !entry.has_src
                {
                    entry.buffer.push_str(text.as_ref());
                }
                let node = self
                    .dom
                    .borrow_mut()
                    .mirror_mut()
                    .new_text(text.to_string());
                self.dom
                    .borrow_mut()
                    .mirror_mut()
                    .append_child(*parent, node);
            }
        }
    }

    fn append_based_on_parent_node(
        &self,
        _element: &Self::Handle,
        _prev_element: &Self::Handle,
        child: NodeOrText<Self::Handle>,
    ) {
        // Simplified: append to the document root regardless of foster parenting context.
        let parent = self.get_document();
        match child {
            NodeOrText::AppendNode(node) => {
                self.dom
                    .borrow_mut()
                    .mirror_mut()
                    .append_child(parent, node);
            }
            NodeOrText::AppendText(text) => {
                let node = self
                    .dom
                    .borrow_mut()
                    .mirror_mut()
                    .new_text(text.to_string());
                self.dom
                    .borrow_mut()
                    .mirror_mut()
                    .append_child(parent, node);
            }
        }
    }

    fn append_doctype_to_document(
        &self,
        _name: StrTendril,
        _public_id: StrTendril,
        _system_id: StrTendril,
    ) {
        // Ignore for now
    }

    fn mark_script_already_started(&self, _node: &Self::Handle) {}

    fn pop(&self, node: &Self::Handle) {
        if let Some(state) = self.script_nodes.borrow_mut().remove(node) {
            let (source, url_string) = if state.has_src {
                state.src_value.as_deref().map_or_else(
                    || (String::new(), String::new()),
                    |src| {
                        let resolved = self
                            .base_url
                            .join(src)
                            .ok()
                            .or_else(|| Url::parse(src).ok());
                        let url_s = resolved
                            .as_ref()
                            .map_or_else(|| src.to_owned(), ToString::to_string);
                        (self.fetch_script_source(src).unwrap_or_default(), url_s)
                    },
                )
            } else {
                let kind_tag = if state.is_module {
                    "inline:module"
                } else {
                    "inline:script"
                };
                (state.buffer, String::from(kind_tag))
            };
            let job = ScriptJob {
                kind: if state.is_module {
                    ScriptKind::Module
                } else {
                    ScriptKind::Classic
                },
                source,
                url: url_string,
                deferred: state.defer || state.is_module,
            };
            if job.deferred {
                self.deferred_scripts.borrow_mut().push(job);
            } else {
                drop(self.script_tx.send(job));
            }
        }
    }

    fn get_template_contents(&self, target: &Self::Handle) -> Self::Handle {
        // We don't model template contents specially; return the node itself
        *target
    }

    fn same_node(&self, x: &Self::Handle, y: &Self::Handle) -> bool {
        x == y
    }

    fn set_quirks_mode(&self, _mode: QuirksMode) {}

    fn append_before_sibling(&self, sibling: &Self::Handle, new_node: NodeOrText<Self::Handle>) {
        match new_node {
            NodeOrText::AppendNode(node) => {
                self.dom
                    .borrow_mut()
                    .mirror_mut()
                    .insert_before(*sibling, node);
            }
            NodeOrText::AppendText(text) => {
                let node = self
                    .dom
                    .borrow_mut()
                    .mirror_mut()
                    .new_text(text.to_string());
                self.dom
                    .borrow_mut()
                    .mirror_mut()
                    .insert_before(*sibling, node);
            }
        }
    }

    fn add_attrs_if_missing(&self, target: &Self::Handle, attrs: Vec<Attribute>) {
        for attr in attrs {
            let name = attr.name.local.to_string();
            if !self.dom.borrow_mut().mirror_mut().has_attr(*target, &name) {
                self.dom.borrow_mut().mirror_mut().set_attr(
                    *target,
                    name.clone(),
                    attr.value.to_string(),
                );
            }
            if let Some(state) = self.script_nodes.borrow_mut().get_mut(target) {
                if attr.name.local.eq(&local_name!("src")) {
                    state.has_src = true;
                    state.src_value = Some(attr.value.to_string());
                }
                if attr.name.local.eq(&local_name!("defer")) {
                    state.defer = true;
                }
                if attr.name.local.eq(&local_name!("async")) {
                    state.async_attr = true;
                }
                if attr.name.local.eq(&local_name!("type"))
                    && attr.value.to_ascii_lowercase() == "module"
                {
                    state.is_module = true;
                }
            }
        }
    }

    fn remove_from_parent(&self, target: &Self::Handle) {
        self.dom
            .borrow_mut()
            .mirror_mut()
            .remove_from_parent(*target);
    }

    fn reparent_children(&self, node: &Self::Handle, new_parent: &Self::Handle) {
        self.dom
            .borrow_mut()
            .mirror_mut()
            .reparent_children(*node, *new_parent);
    }

    fn is_mathml_annotation_xml_integration_point(&self, _handle: &Self::Handle) -> bool {
        false
    }
}

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
