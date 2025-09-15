use crate::parser::{ParserDOMMirror, ScriptJob, ScriptKind};
use anyhow::Error;
use html5ever::parse_document;
use html5ever::tendril::StrTendril;
use html5ever::tendril::TendrilSink;
use html5ever::tree_builder::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::{Attribute, LocalName, Namespace, Parser, QualName, local_name, ns};
use indextree::NodeId;
use js::{DOMMirror, DOMUpdate};
use std::cell::RefCell;
use std::collections::HashMap;
use url::Url;

#[derive(Debug, Clone)]
pub struct OwnedElemName {
    ns: Namespace,
    local: LocalName,
}

impl html5ever::tree_builder::ElemName for OwnedElemName {
    fn ns(&self) -> &Namespace {
        &self.ns
    }
    fn local_name(&self) -> &LocalName {
        &self.local
    }
}

/// TreeSink implementation that writes directly into our DOM updates via the parser mirror
pub struct ValorSink {
    pub(crate) dom: RefCell<DOMMirror<ParserDOMMirror>>,
    element_names: RefCell<HashMap<NodeId, QualName>>,
    script_tx: tokio::sync::mpsc::UnboundedSender<ScriptJob>,
    base_url: Url,
    deferred_scripts: RefCell<Vec<ScriptJob>>,
    // Track script state by node id
    script_nodes: RefCell<HashMap<NodeId, ScriptState>>,
}

#[derive(Clone, Debug)]
struct ScriptState {
    has_src: bool,
    buffer: String,
    defer: bool,
    async_attr: bool,
    src_value: Option<String>,
    is_module: bool,
}

impl ValorSink {
    pub fn new(
        dom: DOMMirror<ParserDOMMirror>,
        script_tx: tokio::sync::mpsc::UnboundedSender<ScriptJob>,
        base_url: Url,
    ) -> Self {
        Self {
            dom: RefCell::new(dom),
            element_names: RefCell::new(HashMap::new()),
            script_tx,
            base_url,
            deferred_scripts: RefCell::new(Vec::new()),
            script_nodes: RefCell::new(HashMap::new()),
        }
    }

    fn fetch_script_source(&self, src_value: &str) -> Option<String> {
        // Resolve against base URL; support file:// scheme for now.
        let resolved = self
            .base_url
            .join(src_value)
            .ok()
            .or_else(|| url::Url::parse(src_value).ok());
        if let Some(url) = resolved {
            match url.scheme() {
                "file" => {
                    if let Ok(path) = url.to_file_path()
                        && let Ok(data) = std::fs::read_to_string(path)
                    {
                        return Some(data);
                    }
                }
                _ => {
                    // TODO: implement http/https in async host path; ignore for now
                }
            }
        }
        None
    }

    fn flush_deferred(&self) {
        let mut queue = self.deferred_scripts.borrow_mut();
        let drained: Vec<ScriptJob> = std::mem::take(&mut *queue);
        drop(queue);
        for job in drained {
            let _ = self.script_tx.send(job);
        }
    }
}

impl TreeSink for ValorSink {
    type Handle = NodeId;
    type Output = ();
    type ElemName<'a>
        = OwnedElemName
    where
        Self: 'a;

    fn finish(self) -> Self::Output {}

    fn parse_error(&self, _msg: std::borrow::Cow<'static, str>) {}

    fn get_document(&self) -> Self::Handle {
        self.dom.borrow_mut().mirror_mut().root_id()
    }

    fn elem_name<'a>(&'a self, target: &'a Self::Handle) -> Self::ElemName<'a> {
        if let Some(q) = self.element_names.borrow().get(target) {
            return OwnedElemName {
                ns: q.ns.clone(),
                local: q.local.clone(),
            };
        }
        // Fallback to a reasonable default. In practice, we should always have a name
        // for elements created via create_element.
        OwnedElemName {
            ns: ns!(html),
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
        let is_script = name.ns == ns!(html) && name.local.eq(&local_name!("script"));
        let mut state = if is_script {
            Some(ScriptState {
                has_src: false,
                buffer: String::new(),
                defer: false,
                async_attr: false,
                src_value: None,
                is_module: false,
            })
        } else {
            None
        };
        for a in attrs {
            let local = a.name.local.to_string();
            if let Some(ref mut st) = state {
                if a.name.local.eq(&local_name!("src")) {
                    st.has_src = true;
                    st.src_value = Some(a.value.to_string());
                }
                if a.name.local.eq(&local_name!("defer")) {
                    st.defer = true;
                }
                if a.name.local.eq(&local_name!("async")) {
                    st.async_attr = true;
                }
                if a.name.local.eq(&local_name!("type")) && a.value.to_ascii_lowercase() == "module"
                {
                    st.is_module = true;
                }
            }
            let mut dom = self.dom.borrow_mut();
            let domm = dom.mirror_mut();
            domm.set_attr(id, local, a.value.to_string());
        }
        if let Some(st) = state {
            self.script_nodes.borrow_mut().insert(id, st);
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
        new_node: NodeOrText<Self::Handle>,
    ) {
        // Simplified: append to the document root regardless of foster parenting context.
        let parent = self.get_document();
        match new_node {
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
                if let Some(src) = state.src_value.as_deref() {
                    let resolved = self
                        .base_url
                        .join(src)
                        .ok()
                        .or_else(|| url::Url::parse(src).ok());
                    let url_s = resolved
                        .as_ref()
                        .map(|u| u.to_string())
                        .unwrap_or_else(|| src.to_string());
                    (self.fetch_script_source(src).unwrap_or_default(), url_s)
                } else {
                    (String::new(), String::from(""))
                }
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
                let _ = self.script_tx.send(job);
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
        for a in attrs {
            let name = a.name.local.to_string();
            if !self.dom.borrow_mut().mirror_mut().has_attr(*target, &name) {
                self.dom.borrow_mut().mirror_mut().set_attr(
                    *target,
                    name.clone(),
                    a.value.to_string(),
                );
            }
            if let Some(state) = self.script_nodes.borrow_mut().get_mut(target) {
                if a.name.local.eq(&local_name!("src")) {
                    state.has_src = true;
                    state.src_value = Some(a.value.to_string());
                }
                if a.name.local.eq(&local_name!("defer")) {
                    state.defer = true;
                }
                if a.name.local.eq(&local_name!("async")) {
                    state.async_attr = true;
                }
                if a.name.local.eq(&local_name!("type")) && a.value.to_ascii_lowercase() == "module"
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

pub struct Html5everEngine {
    parser: Parser<ValorSink>,
}

impl Html5everEngine {
    fn mirror_mut(&self) -> std::cell::RefMut<'_, DOMMirror<ParserDOMMirror>> {
        self.parser.tokenizer.sink.sink.dom.borrow_mut()
    }

    pub fn new(
        dom: DOMMirror<ParserDOMMirror>,
        script_tx: tokio::sync::mpsc::UnboundedSender<ScriptJob>,
        base_url: Url,
    ) -> Self {
        let sink = ValorSink::new(dom, script_tx, base_url);
        let parser = parse_document(sink, Default::default());
        Self { parser }
    }

    pub fn try_update_sync(&self) -> Result<(), Error> {
        self.mirror_mut().try_update_sync()
    }

    pub fn push(&mut self, chunk: &str) {
        {
            let mut dom = self.mirror_mut();
            dom.mirror_mut().prepare_for_update();
        }
        self.parser.process(StrTendril::from(chunk));
        {
            let mut dom = self.mirror_mut();
            let _ = dom.mirror_mut().finish_update();
        }
    }

    pub fn finalize(&mut self) {
        {
            let mut dom = self.mirror_mut();
            dom.mirror_mut().prepare_for_update();
        }
        // Flush any deferred classic scripts before signaling EndOfDocument so they run before DOMContentLoaded
        self.parser.tokenizer.sink.sink.flush_deferred();
        self.parser.tokenizer.end();
        {
            let mut dom = self.mirror_mut();
            dom.mirror_mut().push_update(DOMUpdate::EndOfDocument);
            let _ = dom.mirror_mut().finish_update();
        }
    }
}
