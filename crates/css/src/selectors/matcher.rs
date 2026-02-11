//! Direct selector matching for lightningcss selectors.
//!
//! This module matches lightningcss's parsed selectors directly against DOM elements,
//! avoiding the need to re-parse selectors from strings.

use lasso::Spur;
use lightningcss::selector::{Component, PseudoClass, Selector, SelectorList};
use parcel_selectors::attr::{
    AttrSelectorOperator, ParsedAttrSelectorOperation, ParsedCaseSensitivity,
};
use parcel_selectors::parser::{Combinator, NthSelectorData, NthType};
use rewrite_core::NodeId;
use rewrite_html::{DomTree, NodeData};

/// Check if any selector in the list matches the element.
pub fn matches_selector_list(
    tree: &DomTree,
    node_id: NodeId,
    selectors: &SelectorList<'_>,
) -> bool {
    selectors
        .0
        .iter()
        .any(|sel| matches_selector(tree, node_id, sel))
}

/// Check if a single selector matches an element.
fn matches_selector(tree: &DomTree, node_id: NodeId, selector: &Selector<'_>) -> bool {
    // Selectors are stored in matching order (right-to-left).
    // We need to match from the rightmost compound selector first.
    let mut current_node = node_id;
    let mut iter = selector.iter_raw_match_order().peekable();

    // Match the first (rightmost) compound selector
    loop {
        match iter.peek() {
            None => return true, // Matched everything
            Some(Component::Combinator(comb)) => {
                // Move past combinator and find next element based on combinator type
                let comb = *comb;
                iter.next();
                match find_next_element(tree, current_node, comb) {
                    Some(next) => current_node = next,
                    None => return false,
                }
            }
            Some(_) => {
                // Match simple selector component
                let component = iter.next().unwrap();
                if !matches_component(tree, current_node, component) {
                    return false;
                }
            }
        }
    }
}

/// Find the next element to match based on the combinator.
fn find_next_element(tree: &DomTree, node_id: NodeId, combinator: Combinator) -> Option<NodeId> {
    match combinator {
        Combinator::Child => parent_element(tree, node_id),
        Combinator::Descendant => parent_element(tree, node_id),
        Combinator::NextSibling => prev_sibling_element(tree, node_id),
        Combinator::LaterSibling => prev_sibling_element(tree, node_id),
        Combinator::PseudoElement | Combinator::SlotAssignment | Combinator::Part => None,
        // Deep combinators (deprecated shadow DOM piercing)
        Combinator::DeepDescendant | Combinator::Deep => parent_element(tree, node_id),
    }
}

/// Match a single component against an element.
fn matches_component(tree: &DomTree, node_id: NodeId, component: &Component<'_>) -> bool {
    let node = &tree.nodes[node_id.0 as usize];
    let NodeData::Element { tag, attributes } = node else {
        return false;
    };

    match component {
        Component::LocalName(local) => {
            let tag_str = tree.interner.resolve(tag);
            tag_str.eq_ignore_ascii_case(local.name.0.as_ref())
        }

        Component::ID(id) => attributes
            .get(&tree.interner.get("id").unwrap_or(Spur::default()))
            .is_some_and(|v| v.as_ref() == id.0.as_ref()),

        Component::Class(class) => attributes
            .get(&tree.interner.get("class").unwrap_or(Spur::default()))
            .is_some_and(|v| v.split_whitespace().any(|c| c == class.0.as_ref())),

        Component::ExplicitUniversalType | Component::ExplicitAnyNamespace => true,

        Component::ExplicitNoNamespace => true, // HTML has no namespaces

        Component::DefaultNamespace(_) | Component::Namespace(_, _) => true,

        Component::AttributeInNoNamespaceExists {
            local_name,
            local_name_lower,
        } => {
            let name = if is_html_element(tree, node_id) {
                local_name_lower.0.as_ref()
            } else {
                local_name.0.as_ref()
            };
            tree.interner
                .get(name)
                .and_then(|key| attributes.get(&key))
                .is_some()
        }

        Component::AttributeInNoNamespace {
            local_name,
            operator,
            value,
            case_sensitivity,
            never_matches,
        } => {
            if *never_matches {
                return false;
            }
            let Some(attr_value) = tree
                .interner
                .get(local_name.0.as_ref())
                .and_then(|key| attributes.get(&key))
            else {
                return false;
            };
            match_attr_value(
                attr_value.as_ref(),
                *operator,
                value.0.as_ref(),
                *case_sensitivity,
            )
        }

        Component::AttributeOther(attr) => {
            // Handle namespaced attributes (rare in HTML)
            let local_name = if is_html_element(tree, node_id) {
                &attr.local_name_lower
            } else {
                &attr.local_name
            };
            let Some(attr_value) = tree
                .interner
                .get(local_name.0.as_ref())
                .and_then(|key| attributes.get(&key))
            else {
                return matches!(attr.operation, ParsedAttrSelectorOperation::Exists);
            };
            match &attr.operation {
                ParsedAttrSelectorOperation::Exists => true,
                ParsedAttrSelectorOperation::WithValue {
                    operator,
                    expected_value,
                    case_sensitivity,
                } => match_attr_value(
                    attr_value.as_ref(),
                    *operator,
                    expected_value.0.as_ref(),
                    *case_sensitivity,
                ),
            }
        }

        // Tree-structural pseudo-classes
        Component::Root => parent_element(tree, node_id).is_none(),
        Component::Empty => is_empty(tree, node_id),
        Component::Scope => true, // Default scope is the root

        Component::Nth(data) => matches_nth(tree, node_id, data),

        // Non-tree-structural pseudo-classes from lightningcss
        Component::NonTSPseudoClass(pc) => matches_pseudo_class(tree, node_id, pc),

        // Pseudo-elements - we don't match these in normal element matching
        Component::PseudoElement(_) => false,

        // Negation
        Component::Negation(selectors) => {
            !selectors.iter().any(|s| matches_selector(tree, node_id, s))
        }

        // :is() / :where() - matches if any selector matches
        Component::Is(selectors) | Component::Where(selectors) | Component::Any(_, selectors) => {
            selectors.iter().any(|s| matches_selector(tree, node_id, s))
        }

        // :has() - matches if any relative selector matches
        Component::Has(selectors) => selectors.iter().any(|s| {
            // :has() checks descendants/siblings based on the selector
            matches_has_selector(tree, node_id, s)
        }),

        // Not supported in our DOM model
        Component::Slotted(_)
        | Component::Part(_)
        | Component::Host(_)
        | Component::NthOf(_)
        | Component::Nesting => false,

        Component::Combinator(_) => {
            // Combinators are handled in the main loop
            true
        }
    }
}

/// Match attribute value with operator.
fn match_attr_value(
    attr_value: &str,
    operator: AttrSelectorOperator,
    expected: &str,
    case_sensitivity: ParsedCaseSensitivity,
) -> bool {
    let case_insensitive = matches!(
        case_sensitivity,
        ParsedCaseSensitivity::AsciiCaseInsensitive
            | ParsedCaseSensitivity::AsciiCaseInsensitiveIfInHtmlElementInHtmlDocument
    );

    let cmp = |a: &str, b: &str| {
        if case_insensitive {
            a.eq_ignore_ascii_case(b)
        } else {
            a == b
        }
    };

    match operator {
        AttrSelectorOperator::Equal => cmp(attr_value, expected),
        AttrSelectorOperator::Includes => attr_value.split_whitespace().any(|w| cmp(w, expected)),
        AttrSelectorOperator::DashMatch => {
            cmp(attr_value, expected)
                || (attr_value.len() > expected.len()
                    && cmp(&attr_value[..expected.len()], expected)
                    && attr_value.as_bytes().get(expected.len()) == Some(&b'-'))
        }
        AttrSelectorOperator::Prefix => {
            !expected.is_empty()
                && attr_value.len() >= expected.len()
                && cmp(&attr_value[..expected.len()], expected)
        }
        AttrSelectorOperator::Suffix => {
            !expected.is_empty()
                && attr_value.len() >= expected.len()
                && cmp(&attr_value[attr_value.len() - expected.len()..], expected)
        }
        AttrSelectorOperator::Substring => {
            if expected.is_empty() {
                return false;
            }
            if case_insensitive {
                attr_value
                    .to_ascii_lowercase()
                    .contains(&expected.to_ascii_lowercase())
            } else {
                attr_value.contains(expected)
            }
        }
    }
}

/// Match :nth-child, :nth-last-child, :nth-of-type, :nth-last-of-type, etc.
fn matches_nth(tree: &DomTree, node_id: NodeId, data: &NthSelectorData) -> bool {
    let index = match data.ty {
        NthType::Child | NthType::LastChild | NthType::OnlyChild => {
            get_sibling_index(tree, node_id, data.ty, false)
        }
        NthType::OfType | NthType::LastOfType | NthType::OnlyOfType => {
            get_sibling_index(tree, node_id, data.ty, true)
        }
        // Table column pseudo-classes - not supported
        NthType::Col | NthType::LastCol => return false,
    };

    let Some(index) = index else {
        return false;
    };

    // Check :only-child / :only-of-type
    if matches!(data.ty, NthType::OnlyChild | NthType::OnlyOfType) {
        return index == 1
            && is_only_sibling(tree, node_id, matches!(data.ty, NthType::OnlyOfType));
    }

    // Check :first-child / :first-of-type (a=0, b=1)
    if data.a == 0 {
        return index == data.b;
    }

    // General case: an+b
    // index = a*n + b for some non-negative integer n
    // n = (index - b) / a
    let diff = index - data.b;
    if data.a == 0 {
        diff == 0
    } else if (data.a > 0 && diff >= 0) || (data.a < 0 && diff <= 0) {
        diff % data.a == 0 && diff / data.a >= 0
    } else {
        false
    }
}

/// Get the 1-based sibling index.
fn get_sibling_index(tree: &DomTree, node_id: NodeId, ty: NthType, same_type: bool) -> Option<i32> {
    let from_end = matches!(ty, NthType::LastChild | NthType::LastOfType);
    let tag = if same_type {
        if let NodeData::Element { tag, .. } = &tree.nodes[node_id.0 as usize] {
            Some(*tag)
        } else {
            return None;
        }
    } else {
        None
    };

    let parent = parent_element(tree, node_id)?;

    // Collect siblings using tree's public children iterator
    let mut siblings = Vec::new();
    for current in tree.children(parent) {
        if let NodeData::Element { tag: elem_tag, .. } = &tree.nodes[current.0 as usize] {
            if same_type {
                if Some(*elem_tag) == tag {
                    siblings.push(current);
                }
            } else {
                siblings.push(current);
            }
        }
    }

    let pos = siblings.iter().position(|&id| id == node_id)?;
    let index = if from_end {
        siblings.len() - pos
    } else {
        pos + 1
    };
    Some(index as i32)
}

/// Check if the element is the only sibling (optionally of same type).
fn is_only_sibling(tree: &DomTree, node_id: NodeId, same_type: bool) -> bool {
    let tag = if same_type {
        if let NodeData::Element { tag, .. } = &tree.nodes[node_id.0 as usize] {
            Some(*tag)
        } else {
            return false;
        }
    } else {
        None
    };

    let Some(parent) = parent_element(tree, node_id) else {
        return true;
    };

    let mut count = 0;
    for current in tree.children(parent) {
        if let NodeData::Element { tag: elem_tag, .. } = &tree.nodes[current.0 as usize] {
            if same_type {
                if Some(*elem_tag) == tag {
                    count += 1;
                }
            } else {
                count += 1;
            }
            if count > 1 {
                return false;
            }
        }
    }
    count == 1
}

/// Match lightningcss pseudo-classes.
fn matches_pseudo_class(tree: &DomTree, node_id: NodeId, pc: &PseudoClass<'_>) -> bool {
    let node = &tree.nodes[node_id.0 as usize];
    let NodeData::Element { tag, attributes } = node else {
        return false;
    };
    let tag_str = tree.interner.resolve(tag);

    match pc {
        PseudoClass::Link | PseudoClass::AnyLink(_) => {
            (tag_str.eq_ignore_ascii_case("a") || tag_str.eq_ignore_ascii_case("area"))
                && tree
                    .interner
                    .get("href")
                    .and_then(|k| attributes.get(&k))
                    .is_some()
        }
        PseudoClass::Visited => false, // Never match :visited for privacy
        PseudoClass::Hover
        | PseudoClass::Active
        | PseudoClass::Focus
        | PseudoClass::FocusVisible
        | PseudoClass::FocusWithin => {
            false // Dynamic state not tracked
        }
        PseudoClass::Enabled => !is_disabled(tree, node_id),
        PseudoClass::Disabled => is_disabled(tree, node_id),
        PseudoClass::Checked => tree
            .interner
            .get("checked")
            .and_then(|k| attributes.get(&k))
            .is_some(),
        PseudoClass::Indeterminate => false,
        PseudoClass::ReadOnly(_) => {
            // Form elements with readonly attribute, or non-editable elements
            let is_form = matches!(tag_str.to_ascii_lowercase().as_str(), "input" | "textarea");
            !is_form
                || tree
                    .interner
                    .get("readonly")
                    .and_then(|k| attributes.get(&k))
                    .is_some()
        }
        PseudoClass::ReadWrite(_) => {
            let is_form = matches!(tag_str.to_ascii_lowercase().as_str(), "input" | "textarea");
            is_form
                && tree
                    .interner
                    .get("readonly")
                    .and_then(|k| attributes.get(&k))
                    .is_none()
        }
        PseudoClass::Required => tree
            .interner
            .get("required")
            .and_then(|k| attributes.get(&k))
            .is_some(),
        PseudoClass::Optional => tree
            .interner
            .get("required")
            .and_then(|k| attributes.get(&k))
            .is_none(),
        PseudoClass::Default => {
            // :default matches the default button in a form or default option
            tree.interner
                .get("checked")
                .and_then(|k| attributes.get(&k))
                .is_some()
                || tree
                    .interner
                    .get("selected")
                    .and_then(|k| attributes.get(&k))
                    .is_some()
        }
        PseudoClass::Valid
        | PseudoClass::Invalid
        | PseudoClass::InRange
        | PseudoClass::OutOfRange => {
            false // Form validation state not tracked
        }
        PseudoClass::PlaceholderShown(_) => {
            // Check if input/textarea has placeholder and is empty
            tree.interner
                .get("placeholder")
                .and_then(|k| attributes.get(&k))
                .is_some()
        }
        PseudoClass::Autofill(_) => false,
        PseudoClass::Target | PseudoClass::TargetWithin => false, // No URL tracking
        PseudoClass::Defined => true,                             // All HTML elements are defined
        PseudoClass::Blank => is_empty(tree, node_id),
        PseudoClass::LocalLink => false,
        PseudoClass::Fullscreen(_) | PseudoClass::Modal | PseudoClass::PictureInPicture => false,
        PseudoClass::Open | PseudoClass::Closed => {
            // :open/:closed for details/dialog
            let is_openable = matches!(tag_str.to_ascii_lowercase().as_str(), "details" | "dialog");
            if !is_openable {
                return false;
            }
            let is_open = tree
                .interner
                .get("open")
                .and_then(|k| attributes.get(&k))
                .is_some();
            matches!(pc, PseudoClass::Open) == is_open
        }
        PseudoClass::PopoverOpen => tree
            .interner
            .get("popover")
            .and_then(|k| attributes.get(&k))
            .is_some(),
        // Language pseudo-classes
        PseudoClass::Lang { languages } => {
            // Check lang attribute
            let lang_attr = tree.interner.get("lang").and_then(|k| attributes.get(&k));
            if let Some(lang) = lang_attr {
                languages.iter().any(|l| {
                    lang.eq_ignore_ascii_case(l.as_ref())
                        || (lang.len() > l.len()
                            && lang[..l.len()].eq_ignore_ascii_case(l.as_ref())
                            && lang.as_bytes().get(l.len()) == Some(&b'-'))
                })
            } else {
                false
            }
        }
        PseudoClass::Dir { direction } => {
            let dir_attr = tree.interner.get("dir").and_then(|k| attributes.get(&k));
            match direction {
                lightningcss::selector::Direction::Ltr => {
                    dir_attr.is_none_or(|d| d.eq_ignore_ascii_case("ltr"))
                }
                lightningcss::selector::Direction::Rtl => {
                    dir_attr.is_some_and(|d| d.eq_ignore_ascii_case("rtl"))
                }
            }
        }
        // Media pseudo-classes - not applicable to element matching
        PseudoClass::Current
        | PseudoClass::Past
        | PseudoClass::Future
        | PseudoClass::Playing
        | PseudoClass::Paused
        | PseudoClass::Seeking
        | PseudoClass::Buffering
        | PseudoClass::Stalled
        | PseudoClass::Muted
        | PseudoClass::VolumeLocked => false,
        // Webkit scrollbar pseudo-classes
        PseudoClass::WebKitScrollbar(_) => false,
        // User action states
        PseudoClass::UserValid | PseudoClass::UserInvalid => false,
        // View transitions
        PseudoClass::ActiveViewTransition | PseudoClass::ActiveViewTransitionType { .. } => false,
        // Custom element state
        PseudoClass::State { .. } => false,
        // CSS modules
        PseudoClass::Local { selector } | PseudoClass::Global { selector } => {
            matches_selector(tree, node_id, selector)
        }
        // Unknown pseudo-classes
        PseudoClass::Custom { .. } | PseudoClass::CustomFunction { .. } => false,
    }
}

/// Match :has() selector by checking descendants/siblings.
fn matches_has_selector(tree: &DomTree, node_id: NodeId, selector: &Selector<'_>) -> bool {
    // :has() can use relative selectors like :has(> child) or :has(+ sibling)
    // For now, we check all descendants
    let mut stack = vec![node_id];
    let mut visited = std::collections::HashSet::new();
    visited.insert(node_id);

    while let Some(current) = stack.pop() {
        for child in tree.children(current) {
            if visited.insert(child) {
                if matches!(&tree.nodes[child.0 as usize], NodeData::Element { .. }) {
                    if matches_selector(tree, child, selector) {
                        return true;
                    }
                    stack.push(child);
                }
            }
        }
    }
    false
}

// Helper functions

fn is_html_element(_tree: &DomTree, _node_id: NodeId) -> bool {
    true // We only handle HTML
}

fn is_disabled(tree: &DomTree, node_id: NodeId) -> bool {
    let NodeData::Element { tag, attributes } = &tree.nodes[node_id.0 as usize] else {
        return false;
    };
    let tag_str = tree.interner.resolve(tag);
    let is_form = matches!(
        tag_str.to_ascii_lowercase().as_str(),
        "button" | "input" | "select" | "textarea"
    );
    is_form
        && tree
            .interner
            .get("disabled")
            .and_then(|k| attributes.get(&k))
            .is_some()
}

fn is_empty(tree: &DomTree, node_id: NodeId) -> bool {
    for current in tree.children(node_id) {
        match &tree.nodes[current.0 as usize] {
            NodeData::Element { .. } => return false,
            NodeData::Text(text) if !text.trim().is_empty() => return false,
            _ => {}
        }
    }
    true
}

fn parent_element(tree: &DomTree, node_id: NodeId) -> Option<NodeId> {
    let parent_id = tree.parent(node_id)?;
    if matches!(&tree.nodes[parent_id.0 as usize], NodeData::Element { .. }) {
        Some(parent_id)
    } else {
        None
    }
}

fn prev_sibling_element(tree: &DomTree, node_id: NodeId) -> Option<NodeId> {
    let parent = parent_element(tree, node_id)?;

    let mut prev: Option<NodeId> = None;
    for current in tree.children(parent) {
        if current == node_id {
            break;
        }
        if matches!(&tree.nodes[current.0 as usize], NodeData::Element { .. }) {
            prev = Some(current);
        }
    }
    prev
}
