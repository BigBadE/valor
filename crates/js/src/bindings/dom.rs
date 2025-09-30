use std::collections::HashSet;
use std::sync::atomic::Ordering;

use crate::dom_index::DomIndexState;
use crate::{DOMUpdate, NodeKey};

use super::values::JSError;
use super::HostContext;

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
                    text: text.to_string(),
                },
            },
        );
    }
    let parent_now = *parent_stack.last().unwrap();
    idx.children_by_parent
        .entry(parent_now)
        .or_default()
        .push(text_key);
    idx.parent_by_child.insert(text_key, parent_now);
    idx.text_by_key.insert(text_key, text.to_string());
    updates.push(DOMUpdate::InsertText {
        parent: parent_now,
        node: text_key,
        text: text.to_string(),
        pos: usize::MAX,
    });
    Ok(())
}

fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

pub fn serialize_node(idx: &DomIndexState, node: NodeKey) -> String {
    let mut html = String::new();
    if let Some(tag) = idx.tag_by_key.get(&node) {
        html.push('<');
        html.push_str(tag);
        if let Some(idv) = idx.id_by_key.get(&node).filter(|s| !s.is_empty()) {
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

fn remove_from_class_index(idx: &mut DomIndexState, class_name: &str, node: NodeKey) {
    if let Some(list) = idx.class_index.get_mut(class_name) {
        list.retain(|k| *k != node);
    }
}

pub fn set_attr_index_sync(idx: &mut DomIndexState, node: NodeKey, name_lc: &str, value: &str) {
    if name_lc == "id" {
        if let Some(old) = idx.id_by_key.insert(node, value.to_string()) {
            if idx.id_index.get(&old).copied() == Some(node) {
                idx.id_index.remove(&old);
            }
        }
        if value.is_empty() {
            idx.id_by_key.remove(&node);
        } else {
            idx.id_index.insert(value.to_string(), node);
        }
        return;
    }
    if name_lc == "class" {
        if let Some(prev) = idx.classes_by_key.get(&node).cloned() {
            for c in prev {
                remove_from_class_index(idx, &c, node);
            }
        }
        let mut set: HashSet<String> = HashSet::new();
        for token in value.split(|ch: char| ch.is_whitespace()) {
            let t = token.trim();
            if t.is_empty() {
                continue;
            }
            let lc = t.to_ascii_lowercase();
            set.insert(lc.clone());
            idx.class_index.entry(lc).or_default().push(node);
        }
        if set.is_empty() {
            idx.classes_by_key.remove(&node);
        } else {
            idx.classes_by_key.insert(node, set);
        }
    }
}

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
            for c in prev {
                remove_from_class_index(idx, &c, node);
            }
        }
    }
}

pub fn reparent_child(
    idx: &mut DomIndexState,
    child_key: NodeKey,
    parent_key: NodeKey,
    position: usize,
) {
    if let Some(prev_parent) = idx.parent_by_child.get(&child_key).copied() {
        if let Some(v) = idx.children_by_parent.get_mut(&prev_parent) {
            v.retain(|k| *k != child_key);
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

struct ElementSpec<'element> {
    tag: &'element str,
    id_attr: Option<String>,
    class_attr: Option<String>,
}

fn insert_element(
    context: &HostContext,
    idx: &mut DomIndexState,
    parent_stack: &mut Vec<NodeKey>,
    spec: ElementSpec,
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
                    tag: spec.tag.to_string(),
                },
            },
        );
    }
    let parent_now = *parent_stack.last().unwrap();
    idx.children_by_parent
        .entry(parent_now)
        .or_default()
        .push(node_key);
    idx.parent_by_child.insert(node_key, parent_now);
    idx.tag_by_key.insert(node_key, spec.tag.to_string());
    idx.tag_index
        .entry(spec.tag.to_string())
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
            let t = token.trim();
            if t.is_empty() {
                continue;
            }
            let lc = t.to_ascii_lowercase();
            set.insert(lc.clone());
            idx.class_index.entry(lc).or_default().push(node_key);
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
        tag: spec.tag.to_string(),
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

fn parse_attrs(attrs_str: &str) -> (Option<String>, Option<String>) {
    let mut id_attr: Option<String> = None;
    let mut class_attr: Option<String> = None;
    let mut ai = 0usize;
    let ab = attrs_str.as_bytes();
    while ai < ab.len() {
        while ai < ab.len() && ab[ai].is_ascii_whitespace() {
            ai += 1;
        }
        if ai >= ab.len() {
            break;
        }
        let start = ai;
        while ai < ab.len() && !ab[ai].is_ascii_whitespace() && ab[ai] != b'=' {
            ai += 1;
        }
        let name = &attrs_str[start..ai];
        while ai < ab.len() && ab[ai].is_ascii_whitespace() {
            ai += 1;
        }
        if ai >= ab.len() || ab[ai] != b'=' {
            continue;
        }
        ai += 1;
        while ai < ab.len() && ab[ai].is_ascii_whitespace() {
            ai += 1;
        }
        if ai >= ab.len() {
            break;
        }
        let quote = ab[ai];
        let value: String;
        if quote == b'"' || quote == b'\'' {
            ai += 1;
            let val_start = ai;
            while ai < ab.len() && ab[ai] != quote {
                ai += 1;
            }
            value = attrs_str[val_start..ai].to_string();
            if ai < ab.len() {
                ai += 1;
            }
        } else {
            let val_start = ai;
            while ai < ab.len() && !ab[ai].is_ascii_whitespace() {
                ai += 1;
            }
            value = attrs_str[val_start..ai].to_string();
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

pub fn apply_inner_html(
    context: &HostContext,
    idx: &mut DomIndexState,
    parent_key: NodeKey,
    html: &str,
    updates: &mut Vec<DOMUpdate>,
) -> Result<(), JSError> {
    let mut parent_stack: Vec<NodeKey> = vec![parent_key];
    let mut i = 0usize;
    while i < html.len() {
        if let Some(lt_rel) = html[i..].find('<') {
            if lt_rel > 0 {
                let text = &html[i..i + lt_rel];
                if !text.is_empty() {
                    emit_text(context, idx, &parent_stack, updates, text)?;
                }
                i += lt_rel;
                continue;
            }
            let Some(gt_rel) = html[i..].find('>') else {
                break;
            };
            let inside = &html[i + 1..i + gt_rel];
            if inside.starts_with('/') {
                parent_stack.pop();
                i += gt_rel + 1;
                continue;
            }
            let tag_end = inside
                .find(|c: char| c.is_whitespace())
                .unwrap_or(inside.len());
            let tag = inside[..tag_end].to_ascii_lowercase();
            let attrs_str = inside[tag_end..].trim();
            let (id_attr, class_attr) = parse_attrs(attrs_str);
            let spec = ElementSpec {
                tag: &tag,
                id_attr,
                class_attr,
            };
            insert_element(context, idx, &mut parent_stack, spec, updates)?;
            i += gt_rel + 1;
        } else {
            let text = &html[i..];
            if text.is_empty() {
                break;
            }
            emit_text(context, idx, &parent_stack, updates, text)?;
            break;
        }
    }
    Ok(())
}
