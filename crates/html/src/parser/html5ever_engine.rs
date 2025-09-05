use crate::dom::{DOM, DOMUpdate};
use html5ever::tendril::StrTendril;
use html5ever::tendril::TendrilSink;
use html5ever::tree_builder::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::parse_document;
use html5ever::{Attribute, Parser};
use indextree::NodeId;
use markup5ever::expanded_name;
use markup5ever::{local_name, namespace_url, ns, ExpandedName, QualName};

static EXPANDED_HTML_DIV: ExpandedName = expanded_name!(html "div");

/// TreeSink implementation that writes directly into our DOM
pub struct ValorSink<'a> {
    dom: &'a mut DOM,
}

impl<'a> ValorSink<'a> {
    pub fn new(dom: &'a mut DOM) -> Self {
        Self { dom }
    }
}

impl<'a> TreeSink for ValorSink<'a> {
    type Handle = NodeId;
    type Output = ();

    fn finish(self) -> Self::Output { () }

    fn parse_error(&mut self, _msg: std::borrow::Cow<'static, str>) {}

    fn get_document(&mut self) -> Self::Handle {
        self.dom.root_id()
    }

    fn elem_name(&self, _target: &Self::Handle) -> ExpandedName {
        // We don't track atomized names per node; return a stable expanded name.
        EXPANDED_HTML_DIV
    }

    fn create_element(&mut self, name: QualName, attrs: Vec<Attribute>, _flags: ElementFlags) -> Self::Handle {
        let id = self.dom.new_element(name.local.to_string());
        for a in attrs {
            self.dom.set_attr(id, a.name.local.to_string(), a.value.to_string());
        }
        id
    }

    fn create_comment(&mut self, text: StrTendril) -> Self::Handle {
        // Represent comments as text nodes for now
        self.dom.new_text(text.to_string())
    }

    fn create_pi(&mut self, _target: StrTendril, data: StrTendril) -> Self::Handle {
        // Represent processing instructions as text nodes for now
        self.dom.new_text(data.to_string())
    }

    fn append(&mut self, parent: &Self::Handle, child: NodeOrText<Self::Handle>) {
        match child {
            NodeOrText::AppendNode(node) => {
                self.dom.append_child(*parent, node);
            }
            NodeOrText::AppendText(text) => {
                let node = self.dom.new_text(text.to_string());
                self.dom.append_child(*parent, node);
            }
        }
    }

    fn append_based_on_parent_node(&mut self, _element: &Self::Handle, _prev_element: &Self::Handle, new_node: NodeOrText<Self::Handle>) {
        // Simplified: append to the document root regardless of foster parenting context.
        let parent = self.get_document();
        match new_node {
            NodeOrText::AppendNode(node) => {
                self.dom.append_child(parent, node);
            }
            NodeOrText::AppendText(text) => {
                let node = self.dom.new_text(text.to_string());
                self.dom.append_child(parent, node);
            }
        }
    }

    fn append_doctype_to_document(&mut self, _name: StrTendril, _public_id: StrTendril, _system_id: StrTendril) {
        // Ignore for now
    }

    fn mark_script_already_started(&mut self, _node: &Self::Handle) {}

    fn pop(&mut self, _node: &Self::Handle) {}

    fn get_template_contents(&mut self, target: &Self::Handle) -> Self::Handle {
        // We don't model template contents specially; return the node itself
        *target
    }

    fn same_node(&self, x: &Self::Handle, y: &Self::Handle) -> bool {
        x == y
    }

    fn set_quirks_mode(&mut self, _mode: QuirksMode) {}

    fn append_before_sibling(&mut self, sibling: &Self::Handle, new_node: NodeOrText<Self::Handle>) {
        match new_node {
            NodeOrText::AppendNode(node) => {
                self.dom.insert_before(*sibling, node);
            }
            NodeOrText::AppendText(text) => {
                let node = self.dom.new_text(text.to_string());
                self.dom.insert_before(*sibling, node);
            }
        }
    }

    fn add_attrs_if_missing(&mut self, target: &Self::Handle, attrs: Vec<Attribute>) {
        for a in attrs {
            let name = a.name.local.to_string();
            if !self.dom.has_attr(*target, &name) {
                self.dom.set_attr(*target, name.clone(), a.value.to_string());
            }
        }
    }

    fn remove_from_parent(&mut self, target: &Self::Handle) {
        self.dom.remove_from_parent(*target);
    }

    fn reparent_children(&mut self, node: &Self::Handle, new_parent: &Self::Handle) {
        self.dom.reparent_children(*node, *new_parent);
    }

    fn is_mathml_annotation_xml_integration_point(&self, _handle: &Self::Handle) -> bool {
        false
    }
}

pub struct Html5everEngine<'a> {
    parser: Parser<ValorSink<'a>>,
}

impl<'a> Html5everEngine<'a> {
    fn get_dom(&mut self) -> &mut DOM {
        self.parser.tokenizer.sink.sink.dom
    }
    
    pub fn new(dom: &'a mut DOM) -> Self {
        let sink = ValorSink::new(dom);
        let parser = parse_document(sink, Default::default());
        Self { parser }
    }

    pub fn push(&mut self, chunk: &str) {
        self.get_dom().prepare_for_update();
        self.parser.process(StrTendril::from(chunk));
        // DOM::finish_update will broadcast this batch
        let _ = self.get_dom().finish_update();
    }

    pub fn finalize(&mut self) {
        self.get_dom().prepare_for_update();
        self.parser.tokenizer.end();
        // Ensure EndOfDocument is appended and broadcast
        self.get_dom().push_update(DOMUpdate::EndOfDocument);
        let _ = self.get_dom().finish_update();
    }
}
