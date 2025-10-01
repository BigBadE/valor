//! Core DOM creation and manipulation functions.

use crate::bindings::document::{parse_key, parse_string, parse_usize};
use crate::bindings::document_helpers::{
    create_node_key, find_head_element, register_element_node, register_text_node,
};
use crate::bindings::dom::{apply_inner_html, serialize_node};
use crate::bindings::values::{JSError, JSValue};
use crate::bindings::{CreatedNodeInfo, CreatedNodeKind, HostContext, HostFnSync};
use crate::{DOMUpdate, NodeKey};
use core::sync::atomic::Ordering;
use std::sync::Arc;
/// Build createElement(tag) function.
pub fn build_create_element() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.is_empty() {
                return Err(JSError::TypeError(String::from(
                    "createElement(tag) requires exactly 1 argument",
                )));
            }
            let tag = parse_string(&args[0], "tag")?;
            let local_id = context.js_local_id_counter.fetch_add(1, Ordering::Relaxed) + 1;
            let node_key = {
                let mut mgr = context
                    .js_node_keys
                    .lock()
                    .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
                mgr.key_of(local_id)
            };
            // Track kind meta
            if let Ok(mut map) = context.js_created_nodes.lock() {
                map.insert(
                    node_key,
                    CreatedNodeInfo {
                        kind: CreatedNodeKind::Element { tag: tag.clone() },
                    },
                );
            }
            // Immediately insert under root at end so it exists in the DOM; user can reparent later.
            let update = DOMUpdate::InsertElement {
                parent: NodeKey::ROOT,
                node: node_key,
                tag: tag.clone(),
                pos: usize::MAX,
            };
            context.dom_sender.try_send(vec![update]).map_err(|error| {
                JSError::InternalError(format!("failed to send DOM update: {error}"))
            })?;
            // Synchronously update DomIndex for immediate queries
            if let Ok(mut idx) = context.dom_index.lock() {
                let entry = idx.children_by_parent.entry(NodeKey::ROOT).or_default();
                if !entry.contains(&node_key) {
                    entry.push(node_key);
                }
                idx.parent_by_child.insert(node_key, NodeKey::ROOT);
                let lowercase_tag = tag.to_ascii_lowercase();
                idx.tag_by_key.insert(node_key, lowercase_tag.clone());
                let list = idx.tag_index.entry(lowercase_tag).or_default();
                if !list.contains(&node_key) {
                    list.push(node_key);
                }
            }
            Ok(JSValue::String(node_key.0.to_string()))
        },
    )
}

/// Build createTextNode(text) function.
pub fn build_create_text_node() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.is_empty() {
                return Err(JSError::TypeError(String::from(
                    "createTextNode(text) requires 1 argument",
                )));
            }
            let text = parse_string(&args[0], "text")?;
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
                    CreatedNodeInfo {
                        kind: CreatedNodeKind::Text { text },
                    },
                );
            }
            Ok(JSValue::String(node_key.0.to_string()))
        },
    )
}

/// Build appendStyleText(cssText) function - creates `<style>` element under `<head>`.
pub fn build_append_style_text() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.is_empty() {
                return Err(JSError::TypeError(String::from(
                    "appendStyleText(cssText) requires 1 argument",
                )));
            }
            let css_text = match &args[0] {
                JSValue::String(string_value) => string_value.clone(),
                _ => return Err(JSError::TypeError(String::from("cssText must be string"))),
            };

            let parent_key = find_head_element(context)?.unwrap_or(NodeKey::ROOT);
            let style_key = create_node_key(context)?;
            register_element_node(context, style_key, String::from("style"));
            let text_key = create_node_key(context)?;
            register_text_node(context, text_key, css_text.clone());

            // Synchronously update DomIndex
            if let Ok(mut idx) = context.dom_index.lock() {
                use crate::bindings::dom;
                dom::reparent_child(&mut idx, style_key, parent_key, usize::MAX);
                idx.tag_by_key.insert(style_key, String::from("style"));
                dom::reparent_child(&mut idx, text_key, style_key, 0);
                idx.text_by_key.insert(text_key, css_text.clone());
            }

            let updates: Vec<DOMUpdate> = vec![
                DOMUpdate::InsertElement {
                    parent: parent_key,
                    node: style_key,
                    tag: String::from("style"),
                    pos: usize::MAX,
                },
                DOMUpdate::SetAttr {
                    node: style_key,
                    name: String::from("data-valor-test-reset"),
                    value: String::from("1"),
                },
                DOMUpdate::InsertText {
                    parent: style_key,
                    node: text_key,
                    text: css_text,
                    pos: 0,
                },
            ];
            context.dom_sender.try_send(updates).map_err(|error| {
                JSError::InternalError(format!("failed to send DOM update: {error}"))
            })?;
            Ok(JSValue::Undefined)
        },
    )
}

/// Build createTextNodeRoot(text) function - creates text node directly under ROOT.
pub fn build_create_text_node_root() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.is_empty() {
                return Err(JSError::TypeError(String::from(
                    "createTextNodeRoot(text) requires 1 argument",
                )));
            }
            let text = parse_string(&args[0], "text")?;
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
                    CreatedNodeInfo {
                        kind: CreatedNodeKind::Text { text: text.clone() },
                    },
                );
            }
            // Immediately insert under ROOT at end
            let update = DOMUpdate::InsertText {
                parent: NodeKey::ROOT,
                node: node_key,
                text: text.clone(),
                pos: usize::MAX,
            };
            context.dom_sender.try_send(vec![update]).map_err(|error| {
                JSError::InternalError(format!("failed to send DOM update: {error}"))
            })?;
            // Synchronously update DomIndex for immediate queries
            if let Ok(mut idx) = context.dom_index.lock() {
                let entry = idx.children_by_parent.entry(NodeKey::ROOT).or_default();
                if !entry.contains(&node_key) {
                    entry.push(node_key);
                }
                idx.parent_by_child.insert(node_key, NodeKey::ROOT);
                idx.text_by_key.insert(node_key, text);
            }
            Ok(JSValue::String(node_key.0.to_string()))
        },
    )
}

/// Build appendChild(parentKey, childKey, pos?) function.
pub fn build_append_child() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.len() < 2 {
                return Err(JSError::TypeError(String::from(
                    "appendChild(parentKey, childKey, [pos]) requires 2-3 arguments",
                )));
            }
            let parent_key = parse_key(&args[0], "parentKey")?;
            let child_key = parse_key(&args[1], "childKey")?;
            let position = args.get(2).and_then(parse_usize).unwrap_or(usize::MAX);
            // Determine what to insert based on created meta
            let meta = context
                .js_created_nodes
                .lock()
                .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
            if let Some(info) = meta.get(&child_key) {
                match &info.kind {
                    CreatedNodeKind::Element { tag } => {
                        let update = DOMUpdate::InsertElement {
                            parent: parent_key,
                            node: child_key,
                            tag: tag.clone(),
                            pos: position,
                        };
                        drop(meta);
                        context.dom_sender.try_send(vec![update]).map_err(|error| {
                            JSError::InternalError(format!("failed to send DOM update: {error}"))
                        })?;
                    }
                    CreatedNodeKind::Text { text } => {
                        let update = DOMUpdate::InsertText {
                            parent: parent_key,
                            node: child_key,
                            text: text.clone(),
                            pos: position,
                        };
                        drop(meta);
                        context.dom_sender.try_send(vec![update]).map_err(|error| {
                            JSError::InternalError(format!("failed to send DOM update: {error}"))
                        })?;
                    }
                }
                // Synchronously update DomIndex for immediate queries
                if let Ok(mut idx) = context.dom_index.lock() {
                    use crate::bindings::dom;
                    dom::reparent_child(&mut idx, child_key, parent_key, position);
                }
                Ok(JSValue::Undefined)
            } else {
                Err(JSError::TypeError(String::from(
                    "Unknown childKey; create node via document.createElement/createTextNode first",
                )))
            }
        },
    )
}

/// Build removeNode(nodeKey) function.
pub fn build_remove_node() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.is_empty() {
                return Err(JSError::TypeError(String::from(
                    "removeNode(nodeKey) requires 1 argument",
                )));
            }
            let node_key = parse_key(&args[0], "nodeKey")?;
            let update = DOMUpdate::RemoveNode { node: node_key };
            context.dom_sender.try_send(vec![update]).map_err(|error| {
                JSError::InternalError(format!("failed to send DOM update: {error}"))
            })?;
            // Synchronously update DomIndex for immediate queries
            if let Ok(mut idx) = context.dom_index.lock() {
                idx.remove_node_and_descendants(node_key);
            }
            Ok(JSValue::Undefined)
        },
    )
}

pub fn build_get_inner_html() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.is_empty() {
                return Err(JSError::TypeError(String::from(
                    "getInnerHTML(nodeKey) requires 1 argument",
                )));
            }
            let node_key = parse_key(&args[0], "nodeKey")?;
            let guard = context
                .dom_index
                .lock()
                .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
            let html = serialize_node(&guard, node_key);
            Ok(JSValue::String(html))
        },
    )
}

/// Build setInnerHTML(nodeKey, html) function.
pub fn build_set_inner_html() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.len() < 2 {
                return Err(JSError::TypeError(String::from(
                    "setInnerHTML(nodeKey, html) requires 2 arguments",
                )));
            }
            let parent_key = parse_key(&args[0], "nodeKey")?;
            let html = parse_string(&args[1], "html")?;
            // Collect existing children for removals
            let existing_children: Vec<NodeKey> = {
                let guard = context
                    .dom_index
                    .lock()
                    .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
                guard
                    .children_by_parent
                    .get(&parent_key)
                    .cloned()
                    .unwrap_or_default()
            };
            let mut updates: Vec<DOMUpdate> = Vec::new();
            // Mirror removals into DomIndex eagerly
            if let Ok(mut idx) = context.dom_index.lock() {
                for child in &existing_children {
                    idx.remove_node_and_descendants(*child);
                }
                apply_inner_html(context, &mut idx, parent_key, &html, &mut updates)?;
            }
            let mut final_updates: Vec<DOMUpdate> =
                Vec::with_capacity(existing_children.len() + updates.len());
            for child in existing_children {
                final_updates.push(DOMUpdate::RemoveNode { node: child });
            }
            final_updates.extend(updates);
            context
                .dom_sender
                .try_send(final_updates)
                .map_err(|error| {
                    JSError::InternalError(format!("failed to send DOM update: {error}"))
                })?;
            Ok(JSValue::Undefined)
        },
    )
}
