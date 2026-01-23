//! Wrapper that implements the selectors crate's Element trait for our DOM.

use rewrite_core::{Database, DependencyContext, NodeId, Relationship};
use rewrite_html::{AttributeQuery, ChildrenQuery, TagNameQuery};
use selectors::OpaqueElement;
use selectors::attr::{AttrSelectorOperation, CaseSensitivity, NamespaceConstraint};
use std::cell::RefCell;

/// Wrapper around a NodeId that implements selectors::Element trait.
#[derive(Clone)]
pub struct ElementWrapper<'a> {
    pub node: NodeId,
    pub db: &'a Database,
    pub ctx: &'a RefCell<DependencyContext>,
}

impl<'a> std::fmt::Debug for ElementWrapper<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ElementWrapper")
            .field("node", &self.node)
            .finish_non_exhaustive()
    }
}

impl<'a> ElementWrapper<'a> {
    pub fn new(node: NodeId, db: &'a Database, ctx: &'a RefCell<DependencyContext>) -> Self {
        Self { node, db, ctx }
    }

    fn get_tag_name(&self) -> Option<String> {
        self.db
            .query::<TagNameQuery>(self.node, &mut self.ctx.borrow_mut())
    }

    fn get_attribute(&self, name: &str) -> Option<String> {
        self.db
            .query::<AttributeQuery>((self.node, name.to_string()), &mut self.ctx.borrow_mut())
    }

    fn get_parent(&self) -> Option<NodeId> {
        self.db
            .resolve_relationship(self.node, Relationship::Parent)
            .first()
            .copied()
    }

    fn get_children(&self) -> Vec<NodeId> {
        self.db
            .query::<ChildrenQuery>(self.node, &mut self.ctx.borrow_mut())
    }

    fn is_element(&self) -> bool {
        self.get_tag_name().is_some()
    }
}

impl<'a> selectors::Element for ElementWrapper<'a> {
    type Impl = SelectorImpl;

    fn opaque(&self) -> OpaqueElement {
        OpaqueElement::new(&self.node)
    }

    fn parent_element(&self) -> Option<Self> {
        let mut parent = self.get_parent()?;
        loop {
            let wrapper = ElementWrapper::new(parent, self.db, self.ctx);
            if wrapper.is_element() {
                return Some(wrapper);
            }
            parent = wrapper.get_parent()?;
        }
    }

    fn parent_node_is_shadow_root(&self) -> bool {
        false
    }

    fn containing_shadow_host(&self) -> Option<Self> {
        None
    }

    fn is_pseudo_element(&self) -> bool {
        false
    }

    fn prev_sibling_element(&self) -> Option<Self> {
        let parent = self.get_parent()?;
        let siblings = ElementWrapper::new(parent, self.db, self.ctx).get_children();
        let mut prev_element = None;
        for &sibling in &siblings {
            if sibling == self.node {
                return prev_element;
            }
            let wrapper = ElementWrapper::new(sibling, self.db, self.ctx);
            if wrapper.is_element() {
                prev_element = Some(wrapper);
            }
        }
        None
    }

    fn next_sibling_element(&self) -> Option<Self> {
        let parent = self.get_parent()?;
        let siblings = ElementWrapper::new(parent, self.db, self.ctx).get_children();
        let mut found_self = false;
        for &sibling in &siblings {
            if found_self {
                let wrapper = ElementWrapper::new(sibling, self.db, self.ctx);
                if wrapper.is_element() {
                    return Some(wrapper);
                }
            }
            if sibling == self.node {
                found_self = true;
            }
        }
        None
    }

    fn first_element_child(&self) -> Option<Self> {
        for &child in &self.get_children() {
            let wrapper = ElementWrapper::new(child, self.db, self.ctx);
            if wrapper.is_element() {
                return Some(wrapper);
            }
        }
        None
    }

    fn is_html_element_in_html_document(&self) -> bool {
        true
    }

    fn has_local_name(&self, local_name: &str) -> bool {
        self.get_tag_name()
            .as_ref()
            .map_or(false, |name| name == local_name)
    }

    fn has_namespace(&self, _ns: &()) -> bool {
        true
    }

    fn is_same_type(&self, other: &Self) -> bool {
        self.get_tag_name() == other.get_tag_name()
    }

    fn attr_matches(
        &self,
        ns: &NamespaceConstraint<&()>,
        local_name: &AttrString,
        operation: &AttrSelectorOperation<&AttrString>,
    ) -> bool {
        if !matches!(ns, NamespaceConstraint::Specific(())) {
            return false;
        }

        let attr_value = match self.get_attribute(&local_name.0) {
            Some(v) => AttrString(v),
            None => return false,
        };

        match operation {
            AttrSelectorOperation::Exists => true,
            AttrSelectorOperation::WithValue {
                operator,
                case_sensitivity,
                value,
            } => {
                use selectors::attr::AttrSelectorOperator;
                let matches = match operator {
                    AttrSelectorOperator::Equal => &attr_value == *value,
                    AttrSelectorOperator::Includes => {
                        attr_value.0.split_whitespace().any(|part| part == value.0)
                    }
                    AttrSelectorOperator::DashMatch => {
                        attr_value.0 == value.0
                            || attr_value.0.starts_with(&format!("{}-", value.0))
                    }
                    AttrSelectorOperator::Prefix => {
                        !value.0.is_empty() && attr_value.0.starts_with(&value.0)
                    }
                    AttrSelectorOperator::Suffix => {
                        !value.0.is_empty() && attr_value.0.ends_with(&value.0)
                    }
                    AttrSelectorOperator::Substring => {
                        !value.0.is_empty() && attr_value.0.contains(&value.0)
                    }
                };

                if *case_sensitivity == CaseSensitivity::CaseSensitive {
                    matches
                } else {
                    match operator {
                        AttrSelectorOperator::Equal => attr_value.0.eq_ignore_ascii_case(&value.0),
                        AttrSelectorOperator::Includes => attr_value
                            .0
                            .split_whitespace()
                            .any(|part| part.eq_ignore_ascii_case(&value.0)),
                        AttrSelectorOperator::DashMatch => {
                            attr_value.0.eq_ignore_ascii_case(&value.0)
                                || attr_value
                                    .0
                                    .to_ascii_lowercase()
                                    .starts_with(&format!("{}-", value.0.to_ascii_lowercase()))
                        }
                        AttrSelectorOperator::Prefix => {
                            !value.0.is_empty()
                                && attr_value
                                    .0
                                    .to_ascii_lowercase()
                                    .starts_with(&value.0.to_ascii_lowercase())
                        }
                        AttrSelectorOperator::Suffix => {
                            !value.0.is_empty()
                                && attr_value
                                    .0
                                    .to_ascii_lowercase()
                                    .ends_with(&value.0.to_ascii_lowercase())
                        }
                        AttrSelectorOperator::Substring => {
                            !value.0.is_empty()
                                && attr_value
                                    .0
                                    .to_ascii_lowercase()
                                    .contains(&value.0.to_ascii_lowercase())
                        }
                    }
                }
            }
        }
    }

    fn match_non_ts_pseudo_class(
        &self,
        _pc: &NonTSPseudoClass,
        _context: &mut selectors::matching::MatchingContext<Self::Impl>,
    ) -> bool {
        false
    }

    fn match_pseudo_element(
        &self,
        _pe: &PseudoElement,
        _context: &mut selectors::matching::MatchingContext<Self::Impl>,
    ) -> bool {
        false
    }

    fn apply_selector_flags(&self, _flags: selectors::matching::ElementSelectorFlags) {
        // No-op: we don't track selector flags
    }

    fn is_link(&self) -> bool {
        self.get_tag_name()
            .as_ref()
            .map_or(false, |name| name == "a" || name == "area")
            && self.get_attribute("href").is_some()
    }

    fn is_html_slot_element(&self) -> bool {
        self.get_tag_name()
            .as_ref()
            .map_or(false, |name| name == "slot")
    }

    fn has_id(&self, id: &AttrString, case_sensitivity: CaseSensitivity) -> bool {
        self.get_attribute("id")
            .map_or(false, |attr_id| match case_sensitivity {
                CaseSensitivity::CaseSensitive => &attr_id == &id.0,
                CaseSensitivity::AsciiCaseInsensitive => attr_id.eq_ignore_ascii_case(&id.0),
            })
    }

    fn has_class(&self, name: &AttrString, case_sensitivity: CaseSensitivity) -> bool {
        self.get_attribute("class").map_or(false, |classes| {
            classes
                .split_whitespace()
                .any(|class| match case_sensitivity {
                    CaseSensitivity::CaseSensitive => class == name.0,
                    CaseSensitivity::AsciiCaseInsensitive => class.eq_ignore_ascii_case(&name.0),
                })
        })
    }

    fn imported_part(&self, _name: &AttrString) -> Option<AttrString> {
        None
    }

    fn is_part(&self, _name: &AttrString) -> bool {
        false
    }

    fn is_empty(&self) -> bool {
        for &child in &self.get_children() {
            let wrapper = ElementWrapper::new(child, self.db, self.ctx);
            if wrapper.is_element() {
                return false;
            }
            if let Some(text) = self
                .db
                .query::<rewrite_html::TextContentQuery>(child, &mut self.ctx.borrow_mut())
            {
                if !text.trim().is_empty() {
                    return false;
                }
            }
        }
        true
    }

    fn is_root(&self) -> bool {
        self.get_tag_name()
            .as_ref()
            .map_or(false, |name| name == "html")
    }
}

/// String wrapper that implements ToCss
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct AttrString(pub String);

impl From<&str> for AttrString {
    fn from(s: &str) -> Self {
        AttrString(s.to_string())
    }
}

impl std::borrow::Borrow<str> for AttrString {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl cssparser::ToCss for AttrString {
    fn to_css<W>(&self, dest: &mut W) -> std::fmt::Result
    where
        W: std::fmt::Write,
    {
        cssparser::serialize_string(&self.0, dest)
    }
}

/// Selector implementation types
#[derive(Debug, Clone, Copy)]
pub struct SelectorImpl;

impl selectors::SelectorImpl for SelectorImpl {
    type ExtraMatchingData<'a> = ();
    type AttrValue = AttrString;
    type Identifier = AttrString;
    type LocalName = AttrString;
    type NamespacePrefix = AttrString;
    type NamespaceUrl = ();
    type BorrowedLocalName = str;
    type BorrowedNamespaceUrl = ();
    type NonTSPseudoClass = NonTSPseudoClass;
    type PseudoElement = PseudoElement;
}

/// Non-tree-structural pseudo-classes
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NonTSPseudoClass {}

impl selectors::parser::NonTSPseudoClass for NonTSPseudoClass {
    type Impl = SelectorImpl;

    fn is_active_or_hover(&self) -> bool {
        match *self {}
    }

    fn is_user_action_state(&self) -> bool {
        match *self {}
    }
}

impl cssparser::ToCss for NonTSPseudoClass {
    fn to_css<W>(&self, _dest: &mut W) -> std::fmt::Result
    where
        W: std::fmt::Write,
    {
        match *self {}
    }
}

/// Pseudo-elements
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PseudoElement {}

impl selectors::parser::PseudoElement for PseudoElement {
    type Impl = SelectorImpl;
}

impl cssparser::ToCss for PseudoElement {
    fn to_css<W>(&self, _dest: &mut W) -> std::fmt::Result
    where
        W: std::fmt::Write,
    {
        match *self {}
    }
}
