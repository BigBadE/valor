use js::{DOMMirror, DOMUpdate};
use crate::parser::ParserDOMMirror;
use anyhow::Error;
use html5ever::parse_document;
use html5ever::tendril::StrTendril;
use html5ever::tendril::TendrilSink;
use html5ever::tree_builder::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::{local_name, ns, Attribute, Parser, QualName, LocalName, Namespace};
use indextree::NodeId;
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct OwnedElemName {
    ns: Namespace,
    local: LocalName,
}

impl html5ever::tree_builder::ElemName for OwnedElemName {
    fn ns(&self) -> &Namespace { &self.ns }
    fn local_name(&self) -> &LocalName { &self.local }
}

/// TreeSink implementation that writes directly into our DOM updates via the parser mirror
pub struct ValorSink {
    pub(crate) dom: RefCell<DOMMirror<ParserDOMMirror>>,
    element_names: RefCell<HashMap<NodeId, QualName>>,
    script_tx: tokio::sync::mpsc::UnboundedSender<String>,
    // Track inline script text by node id; bool = has src (external)
    script_nodes: RefCell<HashMap<NodeId, (bool, String)>>,
}

impl ValorSink {
    pub fn new(dom: DOMMirror<ParserDOMMirror>, script_tx: tokio::sync::mpsc::UnboundedSender<String>) -> Self {
        Self { dom: RefCell::new(dom), element_names: RefCell::new(HashMap::new()), script_tx, script_nodes: RefCell::new(HashMap::new()) }
    }
}

impl TreeSink for ValorSink {
    type Handle = NodeId;
    type Output = ();
    type ElemName<'a> = OwnedElemName where Self: 'a;

    fn finish(self) -> Self::Output { () }

    fn parse_error(&self, _msg: std::borrow::Cow<'static, str>) {}

    fn get_document(&self) -> Self::Handle {
        self.dom.borrow_mut().mirror_mut().root_id()
    }

    fn elem_name<'a>(&'a self, target: &'a Self::Handle) -> Self::ElemName<'a> {
        if let Some(q) = self.element_names.borrow().get(target) {
            return OwnedElemName { ns: q.ns.clone(), local: q.local.clone() };
        }
        // Fallback to a reasonable default. In practice, we should always have a name
        // for elements created via create_element.
        OwnedElemName { ns: ns!(html), local: local_name!("div") }
    }

    fn create_element(&self, name: QualName, attrs: Vec<Attribute>, _flags: ElementFlags) -> Self::Handle {
        let id = {
            let mut dom = self.dom.borrow_mut();
            let domm = dom.mirror_mut();
            domm.new_element(name.local.to_string())
        };
        // Track the element's qualified name for correct elem_name reporting
        self.element_names.borrow_mut().insert(id, name.clone());
        let is_script = name.ns == ns!(html) && name.local.eq(&local_name!("script"));
        let mut has_src = false;
        for a in attrs {
            let local = a.name.local.to_string();
            if is_script && a.name.local.eq(&local_name!("src")) {
                has_src = true;
            }
            let mut dom = self.dom.borrow_mut();
            let domm = dom.mirror_mut();
            domm.set_attr(id, local, a.value.to_string());
        }
        if is_script {
            self.script_nodes.borrow_mut().insert(id, (has_src, String::new()));
        }
        id
    }

    fn create_comment(&self, _text: StrTendril) -> Self::Handle {
        // Ignore comment content; produce an empty text node so it doesn't affect layout
        self.dom.borrow_mut().mirror_mut().new_text(String::new())
    }

    fn create_pi(&self, _target: StrTendril, data: StrTendril) -> Self::Handle {
        // Represent processing instructions as text nodes for now
        self.dom.borrow_mut().mirror_mut().new_text(data.to_string())
    }

    fn append(&self, parent: &Self::Handle, child: NodeOrText<Self::Handle>) {
        match child {
            NodeOrText::AppendNode(node) => {
                self.dom.borrow_mut().mirror_mut().append_child(*parent, node);
            }
            NodeOrText::AppendText(text) => {
                // If appending under a <script> without src, collect the text
                if let Some(entry) = self.script_nodes.borrow_mut().get_mut(parent) {
                    let (has_src, buf) = entry;
                    if !*has_src {
                        buf.push_str(text.as_ref());
                    }
                }
                let node = self.dom.borrow_mut().mirror_mut().new_text(text.to_string());
                self.dom.borrow_mut().mirror_mut().append_child(*parent, node);
            }
        }
    }

    fn append_based_on_parent_node(&self, _element: &Self::Handle, _prev_element: &Self::Handle, new_node: NodeOrText<Self::Handle>) {
        // Simplified: append to the document root regardless of foster parenting context.
        let parent = self.get_document();
        match new_node {
            NodeOrText::AppendNode(node) => {
                self.dom.borrow_mut().mirror_mut().append_child(parent, node);
            }
            NodeOrText::AppendText(text) => {
                let node = self.dom.borrow_mut().mirror_mut().new_text(text.to_string());
                self.dom.borrow_mut().mirror_mut().append_child(parent, node);
            }
        }
    }

    fn append_doctype_to_document(&self, _name: StrTendril, _public_id: StrTendril, _system_id: StrTendril) {
        // Ignore for now
    }

    fn mark_script_already_started(&self, _node: &Self::Handle) {}

    fn pop(&self, node: &Self::Handle) {
        if let Some((has_src, buf)) = self.script_nodes.borrow_mut().remove(node) {
            if !has_src {
                let _ = self.script_tx.send(buf);
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
                self.dom.borrow_mut().mirror_mut().insert_before(*sibling, node);
            }
            NodeOrText::AppendText(text) => {
                let node = self.dom.borrow_mut().mirror_mut().new_text(text.to_string());
                self.dom.borrow_mut().mirror_mut().insert_before(*sibling, node);
            }
        }
    }

    fn add_attrs_if_missing(&self, target: &Self::Handle, attrs: Vec<Attribute>) {
        for a in attrs {
            let name = a.name.local.to_string();
            if !self.dom.borrow_mut().mirror_mut().has_attr(*target, &name) {
                self.dom.borrow_mut().mirror_mut().set_attr(*target, name.clone(), a.value.to_string());
            }
            if a.name.local.eq(&local_name!("src")) {
                if let Some((has_src, _)) = self.script_nodes.borrow_mut().get_mut(target) {
                    *has_src = true;
                }
            }
        }
    }

    fn remove_from_parent(&self, target: &Self::Handle) {
        self.dom.borrow_mut().mirror_mut().remove_from_parent(*target);
    }

    fn reparent_children(&self, node: &Self::Handle, new_parent: &Self::Handle) {
        self.dom.borrow_mut().mirror_mut().reparent_children(*node, *new_parent);
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

    pub fn new(dom: DOMMirror<ParserDOMMirror>, script_tx: tokio::sync::mpsc::UnboundedSender<String>) -> Self {
        let sink = ValorSink::new(dom, script_tx);
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
        self.parser.tokenizer.end();
        {
            let mut dom = self.mirror_mut();
            dom.mirror_mut().push_update(DOMUpdate::EndOfDocument);
            let _ = dom.mirror_mut().finish_update();
        }
    }
}
