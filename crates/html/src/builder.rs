//! TreeBuilder for streaming DOM construction via html5ever's TreeSink.

use crate::types::{DomUpdate, NodeData};
use html5ever::tree_builder::{NodeOrText, QuirksMode, TreeSink};
use html5ever::{Attribute, QualName};
use lasso::ThreadedRodeo;
use rewrite_core::NodeId;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use tendril::StrTendril;

/// Builder for streaming DOM construction - implements TreeSink and emits DomUpdate events.
pub struct TreeBuilder<F: Fn(DomUpdate) -> NodeId> {
    callback: F,
    interner: Arc<ThreadedRodeo>,
    element_names: RefCell<HashMap<NodeId, Box<QualName>>>,
}

impl<F: Fn(DomUpdate) -> NodeId> TreeBuilder<F> {
    pub fn new(callback: F, interner: Arc<ThreadedRodeo>) -> Self {
        // Create document node
        callback(DomUpdate::CreateNode(NodeData::Document));

        Self {
            callback,
            interner,
            element_names: RefCell::new(HashMap::new()),
        }
    }

    fn emit(&self, update: DomUpdate) -> NodeId {
        (self.callback)(update)
    }
}

impl<F: Fn(DomUpdate) -> NodeId> TreeSink for TreeBuilder<F> {
    type Handle = NodeId;
    type Output = ();
    type ElemName<'a>
        = &'a QualName
    where
        F: 'a;

    fn finish(self) -> Self::Output {}

    fn parse_error(&self, _msg: std::borrow::Cow<'static, str>) {}

    fn get_document(&self) -> Self::Handle {
        NodeId::ROOT
    }

    fn elem_name<'a>(&'a self, target: &'a Self::Handle) -> Self::ElemName<'a> {
        let names = self.element_names.borrow();
        let name = names
            .get(target)
            .expect("elem_name called on non-element node");
        let ptr: *const QualName = &**name;
        unsafe { &*ptr }
    }

    fn create_element(
        &self,
        name: QualName,
        attrs: Vec<Attribute>,
        _flags: html5ever::tree_builder::ElementFlags,
    ) -> Self::Handle {
        let tag = self.interner.get_or_intern(name.local.as_ref());
        let mut attributes = HashMap::new();
        for attr in &attrs {
            let key = self.interner.get_or_intern(attr.name.local.as_ref());
            attributes.insert(key, attr.value.to_string().into_boxed_str());
        }

        let node = self.emit(DomUpdate::CreateNode(NodeData::Element { tag, attributes }));
        self.element_names.borrow_mut().insert(node, Box::new(name));
        node
    }

    fn create_comment(&self, text: StrTendril) -> Self::Handle {
        self.emit(DomUpdate::CreateNode(NodeData::Comment(
            text.to_string().into_boxed_str(),
        )))
    }

    fn create_pi(&self, _target: StrTendril, _data: StrTendril) -> Self::Handle {
        self.emit(DomUpdate::CreateNode(NodeData::Comment(Box::from(""))))
    }

    fn append(&self, parent: &Self::Handle, child: NodeOrText<Self::Handle>) {
        match child {
            NodeOrText::AppendNode(node) => {
                self.emit(DomUpdate::AppendChild {
                    parent: *parent,
                    child: node,
                });
            }
            NodeOrText::AppendText(text) => {
                let text_node = self.emit(DomUpdate::CreateNode(NodeData::Text(
                    text.to_string().into_boxed_str(),
                )));
                self.emit(DomUpdate::AppendChild {
                    parent: *parent,
                    child: text_node,
                });
            }
        }
    }

    fn append_based_on_parent_node(
        &self,
        element: &Self::Handle,
        _prev_element: &Self::Handle,
        child: NodeOrText<Self::Handle>,
    ) {
        self.append(element, child);
    }

    fn append_doctype_to_document(
        &self,
        _name: StrTendril,
        _public_id: StrTendril,
        _system_id: StrTendril,
    ) {
    }

    fn get_template_contents(&self, target: &Self::Handle) -> Self::Handle {
        *target
    }

    fn same_node(&self, x: &Self::Handle, y: &Self::Handle) -> bool {
        x == y
    }

    fn set_quirks_mode(&self, _mode: QuirksMode) {}

    fn append_before_sibling(&self, _sibling: &Self::Handle, _new_node: NodeOrText<Self::Handle>) {}

    fn add_attrs_if_missing(&self, _target: &Self::Handle, _attrs: Vec<Attribute>) {}

    fn remove_from_parent(&self, _target: &Self::Handle) {}

    fn reparent_children(&self, _node: &Self::Handle, _new_parent: &Self::Handle) {}
}
