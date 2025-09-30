use core::sync::atomic::Ordering;
use std::collections::HashSet;

use crate::dom_index::DomIndexState;
use crate::{DOMUpdate, NodeKey};

use super::values::JSError;
use super::HostContext;

/// Emit a text node into the DOM tree.
///
/// # Errors
/// Returns an error if node creation fails or mutex is poisoned.
fn emit_text(
    context: &HostContext,
    idx: &mut DomIndexState,
    parent_stack: &[NodeKey],
    updates: &mut Vec<DOMUpdate>,
    text: &str,
) -> Result<(), JSError> {
    let local_id = context.js_local_id_counter.fetch_add(1, Ordering::Relaxed) + 1;
    let text_key = {
        let mut mgr = context
            .js_node_keys
            .lock()
            .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
        mgr.key_of(local_id)
    };
    if let Ok(mut map) = context.js_created_nodes.lock() {
        map.insert(
            text_key,
            super::CreatedNodeInfo {
                kind: super::CreatedNodeKind::Text {
                    text: text.to_owned(),
                },
            },
        );
    }
    let parent_now = *parent_stack
        .last()
        .ok_or_else(|| JSError::InternalError(String::from("parent stack is empty")))?;
    idx.children_by_parent
        .entry(parent_now)
        .or_default()
        .push(text_key);
    idx.parent_by_child.insert(text_key, parent_now);
    idx.text_by_key.insert(text_key, text.to_owned());
    updates.push(DOMUpdate::InsertText {
        parent: parent_now,
        node: text_key,
        text: text.to_owned(),
        pos: usize::MAX,
    });
    Ok(())
}

/// Escape special HTML characters in text content.
fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for char_val in text.chars() {
        match char_val {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(char_val),
        }
    }
    out
}

/// Serialize a DOM node and its children to an HTML string.
pub fn serialize_node(idx: &DomIndexState, node: NodeKey) -> String {
    let mut html = String::new();
    if let Some(tag) = idx.tag_by_key.get(&node) {
        html.push('<');
        html.push_str(tag);
        if let Some(idv) = idx
            .id_by_key
            .get(&node)
            .filter(|string_val| !string_val.is_empty())
        {
            html.push_str(" id=\"");
            html.push_str(idv);
            html.push('"');
        }
        if let Some(classes) = idx.classes_by_key.get(&node).filter(|set| !set.is_empty()) {
            let mut list: Vec<&String> = classes.iter().collect();
            list.sort();
            html.push_str(" class=\"");
            let cls_vec: Vec<String> = list.into_iter().cloned().collect();
            html.push_str(&cls_vec.join(" "));
            html.push('"');
        }
        html.push('>');
    }
    if let Some(text) = idx.text_by_key.get(&node) {
        html.push_str(&escape_text(text));
    } else if let Some(children) = idx.children_by_parent.get(&node) {
        for child in children {
            html.push_str(&serialize_node(idx, *child));
        }
    }
    if let Some(tag) = idx.tag_by_key.get(&node) {
        html.push_str("</");
        html.push_str(tag);
        html.push('>');
    }
    html
}

/// Remove a node from the class index for a given class name.
fn remove_from_class_index(idx: &mut DomIndexState, class_name: &str, node: NodeKey) {
    if let Some(list) = idx.class_index.get_mut(class_name) {
        list.retain(|key| *key != node);
    }
}

/// Set an attribute in the DOM index synchronously (for id and class attributes).
pub fn set_attr_index_sync(idx: &mut DomIndexState, node: NodeKey, name_lc: &str, value: &str) {
    if name_lc == "id" {
        if let Some(old) = idx.id_by_key.insert(node, value.to_owned()) {
            if idx.id_index.get(&old).copied() == Some(node) {
                idx.id_index.remove(&old);
            }
        }
        if value.is_empty() {
            idx.id_by_key.insert(node, value.to_owned());
            idx.id_index.insert(value.to_owned(), node);
        }
        return;
    }
    if name_lc == "class" {
        if let Some(prev) = idx.classes_by_key.get(&node).cloned() {
            let mut sorted_classes: Vec<String> = prev.into_iter().collect();
            sorted_classes.sort();
            for class_name in sorted_classes {
                remove_from_class_index(idx, &class_name, node);
            }
        }
        let mut set: HashSet<String> = HashSet::new();
        for token in value.split(|char_val: char| char_val.is_whitespace()) {
            let trimmed = token.trim();
            if trimmed.is_empty() {
                continue;
            }
            let lowercase = trimmed.to_ascii_lowercase();
            set.insert(lowercase.clone());
            idx.class_index.entry(lowercase).or_default().push(node);
        }
        if set.is_empty() {
            idx.classes_by_key.remove(&node);
        } else {
            idx.classes_by_key.insert(node, set);
        }
    }
}

/// Remove an attribute from the DOM index synchronously.
pub fn remove_attr_index_sync(idx: &mut DomIndexState, node: NodeKey, name_lc: &str) {
    if name_lc == "id" {
        if let Some(old) = idx.id_by_key.remove(&node) {
            if idx.id_index.get(&old).copied() == Some(node) {
                idx.id_index.remove(&old);
            }
        }
        return;
    }
    if name_lc == "class" {
        if let Some(prev) = idx.classes_by_key.remove(&node) {
            let mut sorted_classes: Vec<String> = prev.into_iter().collect();
            sorted_classes.sort();
            for class_name in sorted_classes {
                remove_from_class_index(idx, &class_name, node);
            }
        }
    }
}

/// Reparent a child node to a new parent at the specified position.
pub fn reparent_child(
    idx: &mut DomIndexState,
    child_key: NodeKey,
    parent_key: NodeKey,
    position: usize,
) {
    if let Some(prev_parent) = idx.parent_by_child.get(&child_key).copied() {
        if let Some(children) = idx.children_by_parent.get_mut(&prev_parent) {
            children.retain(|key| *key != child_key);
        }
    }
    let entry = idx.children_by_parent.entry(parent_key).or_default();
    if position == usize::MAX || position >= entry.len() {
        if !entry.contains(&child_key) {
            entry.push(child_key);
        }
    } else if !entry.contains(&child_key) {
        entry.insert(position, child_key);
    }
    idx.parent_by_child.insert(child_key, parent_key);
}

/// Specification for creating an HTML element with optional id and class attributes.
struct ElementSpec<'element> {
    /// Tag name of the element.
    tag: &'element str,
    /// Optional id attribute value.
    id_attr: Option<String>,
    /// Optional class attribute value.
    class_attr: Option<String>,
}

/// Insert an element into the DOM tree with the given spec.
///
/// # Errors
/// Returns an error if node creation fails, mutex is poisoned, or parent stack is empty.
///
/// # Panics
/// This function does not panic as all potential panics are converted to errors.
fn insert_element(
    context: &HostContext,
    idx: &mut DomIndexState,
    parent_stack: &mut Vec<NodeKey>,
    spec: ElementSpec<'_>,
    updates: &mut Vec<DOMUpdate>,
) -> Result<(), JSError> {
    let local_id = context.js_local_id_counter.fetch_add(1, Ordering::Relaxed) + 1;
    let node_key = {
        let mut mgr = context
            .js_node_keys
            .lock()
            .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
        mgr.key_of(local_id)
    };
    if let Ok(mut map) = context.js_created_nodes.lock() {
        map.insert(
            node_key,
            super::CreatedNodeInfo {
                kind: super::CreatedNodeKind::Element {
                    tag: spec.tag.to_owned(),
                },
            },
        );
    }
    let parent_now = *parent_stack
        .last()
        .ok_or_else(|| JSError::InternalError(String::from("parent stack is empty")))?;
    idx.children_by_parent
        .entry(parent_now)
        .or_default()
        .push(node_key);
    idx.parent_by_child.insert(node_key, parent_now);
    idx.tag_by_key.insert(node_key, spec.tag.to_owned());
    idx.tag_index
        .entry(spec.tag.to_owned())
        .or_default()
        .push(node_key);
    if let Some(idv) = spec.id_attr.clone() {
        if !idv.is_empty() {
            if let Some(old) = idx.id_by_key.insert(node_key, idv.clone()) {
                if idx.id_index.get(&old).copied() == Some(node_key) {
                    idx.id_index.remove(&old);
                }
            }
            idx.id_index.insert(idv, node_key);
        }
    }
    if let Some(cls) = spec.class_attr.clone() {
        let mut set: HashSet<String> = HashSet::new();
        for token in cls.split_whitespace() {
            let trimmed = token.trim();
            if trimmed.is_empty() {
                continue;
            }
            let lowercase = trimmed.to_ascii_lowercase();
            set.insert(lowercase.clone());
            idx.class_index.entry(lowercase).or_default().push(node_key);
        }
        if set.is_empty() {
            idx.classes_by_key.remove(&node_key);
        } else {
            idx.classes_by_key.insert(node_key, set);
        }
    }
    updates.push(DOMUpdate::InsertElement {
        parent: parent_now,
        node: node_key,
        tag: spec.tag.to_owned(),
        pos: usize::MAX,
    });
    if let Some(idv) = spec.id_attr {
        if !idv.is_empty() {
            updates.push(DOMUpdate::SetAttr {
                node: node_key,
                name: String::from("id"),
                value: idv,
            });
        }
    }
    if let Some(cls) = spec.class_attr {
        updates.push(DOMUpdate::SetAttr {
            node: node_key,
            name: String::from("class"),
            value: cls,
        });
    }
    parent_stack.push(node_key);
    Ok(())
}

/// Parse simple HTML attributes (id and class only) from an attribute string.
/// Returns (`id_attr`, `class_attr`) as optional strings.
fn parse_attrs(attrs_str: &str) -> (Option<String>, Option<String>) {
    let mut id_attr: Option<String> = None;
    let mut class_attr: Option<String> = None;
    let mut attr_index = 0usize;
    let attr_bytes = attrs_str.as_bytes();
    while attr_index < attr_bytes.len() {
        while attr_index < attr_bytes.len() && attr_bytes[attr_index].is_ascii_whitespace() {
            attr_index += 1;
        }
        if attr_index >= attr_bytes.len() {
            break;
        }
        let start = attr_index;
        while attr_index < attr_bytes.len()
            && !attr_bytes[attr_index].is_ascii_whitespace()
            && attr_bytes[attr_index] != b'='
        {
            attr_index += 1;
        }
        let name = attrs_str.get(start..attr_index).unwrap_or("");
        while attr_index < attr_bytes.len() && attr_bytes[attr_index].is_ascii_whitespace() {
            attr_index += 1;
        }
        if attr_index >= attr_bytes.len() || attr_bytes[attr_index] != b'=' {
            continue;
        }
        attr_index += 1;
        while attr_index < attr_bytes.len() && attr_bytes[attr_index].is_ascii_whitespace() {
            attr_index += 1;
        }
        if attr_index >= attr_bytes.len() {
            break;
        }
        let quote = attr_bytes[attr_index];
        let value: String;
        if quote == b'"' || quote == b'\'' {
            attr_index += 1;
            let val_start = attr_index;
            while attr_index < attr_bytes.len() && attr_bytes[attr_index] != quote {
                attr_index += 1;
            }
            value = attrs_str
                .get(val_start..attr_index)
                .unwrap_or("")
                .to_owned();
            if attr_index < attr_bytes.len() {
                attr_index += 1;
            }
        } else {
            let val_start = attr_index;
            while attr_index < attr_bytes.len() && !attr_bytes[attr_index].is_ascii_whitespace() {
                attr_index += 1;
            }
            value = attrs_str
                .get(val_start..attr_index)
                .unwrap_or("")
                .to_owned();
        }
        let name_lc = name.to_ascii_lowercase();
        if name_lc == "id" {
            id_attr = Some(value);
        } else if name_lc == "class" {
            class_attr = Some(value);
        }
    }
    (id_attr, class_attr)
}

/// Apply innerHTML by parsing a simple HTML fragment and emitting DOM updates.
/// Supports basic element tags with id and class attributes.
///
/// # Errors
/// Returns an error if node creation fails or mutex is poisoned.
pub fn apply_inner_html(
    context: &HostContext,
    idx: &mut DomIndexState,
    parent_key: NodeKey,
    html: &str,
    updates: &mut Vec<DOMUpdate>,
) -> Result<(), JSError> {
    let mut parent_stack: Vec<NodeKey> = vec![parent_key];
    let mut index = 0usize;
    while index < html.len() {
        if let Some(lt_rel) = html.get(index..).and_then(|slice| slice.find('<')) {
            if lt_rel > 0 {
                let text = html.get(index..index + lt_rel).unwrap_or("");
                if !text.is_empty() {
                    emit_text(context, idx, &parent_stack, updates, text)?;
                }
                index += lt_rel;
                continue;
            }
            let Some(gt_rel) = html.get(index..).and_then(|slice| slice.find('>')) else {
                break;
            };
            let inside = html.get(index + 1..index + gt_rel).unwrap_or("");
            if inside.starts_with('/') {
                parent_stack.pop();
                index += gt_rel + 1;
                continue;
            }
            let tag_end = inside
                .find(|char_val: char| char_val.is_whitespace())
                .unwrap_or(inside.len());
            let tag = inside.get(..tag_end).unwrap_or("").to_ascii_lowercase();
            let attrs_str = inside.get(tag_end..).unwrap_or("").trim();
            let (id_attr, class_attr) = parse_attrs(attrs_str);
            let spec = ElementSpec {
                tag: &tag,
                id_attr,
                class_attr,
            };
            insert_element(context, idx, &mut parent_stack, spec, updates)?;
            index += gt_rel + 1;
        } else {
            let text = html.get(index..).unwrap_or("");
            if text.is_empty() {
                break;
            }
            emit_text(context, idx, &parent_stack, updates, text)?;
            break;
        }
    }
    Ok(())
}
