//! Document namespace builder with DOM manipulation functions.
//!
//! This module provides the `document` namespace for JavaScript, including:
//! - DOM creation: createElement, createTextNode
//! - DOM manipulation: appendChild, removeNode
//! - Queries: getElementById, querySelector, querySelectorAll
//! - Attributes: setAttribute, getAttribute, removeAttribute
//! - Content: setTextContent, getInnerHTML, setInnerHTML
//! - Storage: localStorage, sessionStorage

use crate::bindings::document_helpers::{
    create_node_key, find_head_element, register_element_node, register_text_node,
};
use crate::bindings::dom::{
    apply_inner_html, remove_attr_index_sync, serialize_node, set_attr_index_sync,
};
use crate::bindings::values::{JSError, JSValue};
use crate::bindings::{CreatedNodeInfo, CreatedNodeKind, HostContext, HostFnSync, HostNamespace};
use crate::{DOMUpdate, NodeKey};
use core::sync::atomic::Ordering;
use std::sync::Arc;

/// Helper to parse a string argument from `JSValue`.
///
/// # Errors
/// Returns an error if the value is not a string.
#[inline]
fn parse_string(value: &JSValue, name: &str) -> Result<String, JSError> {
    match value {
        JSValue::String(string_value) => Ok(string_value.clone()),
        _ => Err(JSError::TypeError(format!("{name} must be a string"))),
    }
}

/// Helper to parse a `NodeKey` from a decimal string `JSValue`.
///
/// # Errors
/// Returns an error if the value is not a string or cannot be parsed as u64.
#[inline]
fn parse_key(value: &JSValue, name: &str) -> Result<NodeKey, JSError> {
    match value {
        JSValue::String(string_value) => {
            let parsed = string_value.parse::<u64>().map_err(|_| {
                JSError::TypeError(format!("{name} must be a decimal string (NodeKey)"))
            })?;
            Ok(NodeKey(parsed))
        }
        _ => Err(JSError::TypeError(format!(
            "{name} must be a decimal string (NodeKey)"
        ))),
    }
}

/// Helper to parse an optional usize from a number `JSValue`.
#[inline]
fn parse_usize(value: &JSValue) -> Option<usize> {
    match value {
        JSValue::Number(number) if *number >= 0.0f64 => Some(*number as usize),
        _ => None,
    }
}

/// Build the `document` namespace with all DOM manipulation functions.
#[inline]
pub fn build_document_namespace() -> HostNamespace {
    HostNamespace::new()
        .with_sync_fn("createElement", build_create_element())
        .with_sync_fn("createTextNode", build_create_text_node())
        .with_sync_fn("appendStyleText", build_append_style_text())
        .with_sync_fn("createTextNodeRoot", build_create_text_node_root())
        .with_sync_fn("appendChild", build_append_child())
        .with_sync_fn("removeNode", build_remove_node())
        .with_sync_fn("getElementById", build_get_element_by_id())
        .with_sync_fn("setTextContent", build_set_text_content())
        .with_sync_fn("setAttribute", build_set_attribute())
        .with_sync_fn("getAttribute", build_get_attribute())
        .with_sync_fn("removeAttribute", build_remove_attribute())
        .with_sync_fn("getElementsByClassName", build_get_elements_by_class_name())
        .with_sync_fn("getElementsByTagName", build_get_elements_by_tag_name())
        .with_sync_fn("querySelector", build_query_selector())
        .with_sync_fn("querySelectorAll", build_query_selector_all())
        .with_sync_fn("getChildIndex", build_get_child_index())
        .with_sync_fn("getChildrenKeys", build_get_children_keys())
        .with_sync_fn("getTagName", build_get_tag_name())
        .with_sync_fn("getParentKey", build_get_parent_key())
        .with_sync_fn("getInnerHTML", build_get_inner_html())
        .with_sync_fn("setInnerHTML", build_set_inner_html())
        .with_sync_fn("net_request", build_net_request())
        .with_sync_fn("net_requestPoll", build_net_request_poll())
        .with_sync_fn("storage_getItem", build_storage_get_item())
        .with_sync_fn("storage_setItem", build_storage_set_item())
        .with_sync_fn("storage_removeItem", build_storage_remove_item())
        .with_sync_fn("storage_clear", build_storage_clear())
        .with_sync_fn("storage_keys", build_storage_keys())
}

/// Build createElement(tag) function.
fn build_create_element() -> Arc<HostFnSync> {
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
fn build_create_text_node() -> Arc<HostFnSync> {
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
fn build_append_style_text() -> Arc<HostFnSync> {
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
fn build_create_text_node_root() -> Arc<HostFnSync> {
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
fn build_append_child() -> Arc<HostFnSync> {
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
fn build_remove_node() -> Arc<HostFnSync> {
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

/// Build getElementById(id) function.
fn build_get_element_by_id() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            use crate::bindings::values::LogLevel;
            if args.is_empty() {
                return Err(JSError::TypeError(String::from(
                    "getElementById(id) requires 1 argument",
                )));
            }
            let id = parse_string(&args[0], "id")?;
            let guard = context
                .dom_index
                .lock()
                .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
            guard.get_element_by_id(&id).map_or_else(
                || {
                    context.logger.log(
                        LogLevel::Info,
                        &format!("JS: getElementById('{id}') -> null"),
                    );
                    Ok(JSValue::Null)
                },
                |key| {
                    context.logger.log(
                        LogLevel::Info,
                        &format!("JS: getElementById('{id}') -> NodeKey={}", key.0),
                    );
                    Ok(JSValue::String(key.0.to_string()))
                },
            )
        },
    )
}

/// Build setTextContent(nodeKey, text) function.
fn build_set_text_content() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            use crate::bindings::values::LogLevel;
            if args.len() < 2 {
                return Err(JSError::TypeError(String::from(
                    "setTextContent(nodeKey, text) requires 2 arguments",
                )));
            }
            let element_key = parse_key(&args[0], "nodeKey")?;
            let text = parse_string(&args[1], "text")?;
            context.logger.log(
                LogLevel::Info,
                &format!(
                    "JS: setTextContent(nodeKey={}, text='{}')",
                    element_key.0, text
                ),
            );
            // Snapshot current children to remove them
            let children: Vec<NodeKey> = {
                let guard = context
                    .dom_index
                    .lock()
                    .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
                guard
                    .children_by_parent
                    .get(&element_key)
                    .cloned()
                    .unwrap_or_default()
            };
            // Mint a fresh text node key
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
                    CreatedNodeInfo {
                        kind: CreatedNodeKind::Text { text: text.clone() },
                    },
                );
            }
            // Synchronously update DomIndex
            if let Ok(mut guard) = context.dom_index.lock() {
                for child in &children {
                    guard.remove_node_and_descendants(*child);
                }
                guard.children_by_parent.insert(element_key, vec![text_key]);
                guard.parent_by_child.insert(text_key, element_key);
                guard.text_by_key.insert(text_key, text.clone());
            }
            // Build batch: remove existing children, then insert new text node
            let removed_count = children.len();
            let mut updates: Vec<DOMUpdate> = Vec::with_capacity(removed_count + 1);
            for child in children {
                updates.push(DOMUpdate::RemoveNode { node: child });
            }
            updates.push(DOMUpdate::InsertText {
                parent: element_key,
                node: text_key,
                text,
                pos: 0,
            });
            context.dom_sender.try_send(updates).map_err(|error| {
                JSError::InternalError(format!("failed to send DOM update: {error}"))
            })?;
            Ok(JSValue::Undefined)
        },
    )
}

/// Build setAttribute(nodeKey, name, value) function.
fn build_set_attribute() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.len() < 3 {
                return Err(JSError::TypeError(String::from(
                    "setAttribute(nodeKey, name, value) requires 3 arguments",
                )));
            }
            let node_key = parse_key(&args[0], "nodeKey")?;
            let name = parse_string(&args[1], "name")?;
            let value = parse_string(&args[2], "value")?;
            let update = DOMUpdate::SetAttr {
                node: node_key,
                name: name.clone(),
                value: value.clone(),
            };
            context.dom_sender.try_send(vec![update]).map_err(|error| {
                JSError::InternalError(format!("failed to send DOM update: {error}"))
            })?;
            // Synchronously update DomIndex for immediate queries (id/class)
            if let Ok(mut idx) = context.dom_index.lock() {
                let name_lc = name.to_ascii_lowercase();
                set_attr_index_sync(&mut idx, node_key, &name_lc, &value);
            }
            Ok(JSValue::Undefined)
        },
    )
}

/// Build getAttribute(nodeKey, name) function.
fn build_get_attribute() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.len() < 2 {
                return Err(JSError::TypeError(String::from(
                    "getAttribute(nodeKey, name) requires 2 arguments",
                )));
            }
            let node_key = parse_key(&args[0], "nodeKey")?;
            let name = parse_string(&args[1], "name")?;
            let name_lc = name.to_ascii_lowercase();
            let guard = context
                .dom_index
                .lock()
                .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
            let value = if name_lc == "id" {
                guard.id_by_key.get(&node_key).cloned().unwrap_or_default()
            } else if name_lc == "class" {
                guard
                    .classes_by_key
                    .get(&node_key)
                    .map(|set| {
                        let mut sorted_classes: Vec<&String> = set.iter().collect();
                        sorted_classes.sort();
                        sorted_classes
                            .into_iter()
                            .cloned()
                            .collect::<Vec<String>>()
                            .join(" ")
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };
            Ok(JSValue::String(value))
        },
    )
}

/// Build removeAttribute(nodeKey, name) function.
fn build_remove_attribute() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.len() < 2 {
                return Err(JSError::TypeError(String::from(
                    "removeAttribute(nodeKey, name) requires 2 arguments",
                )));
            }
            let node_key = parse_key(&args[0], "nodeKey")?;
            let name = parse_string(&args[1], "name")?;
            let update = DOMUpdate::SetAttr {
                node: node_key,
                name: name.clone(),
                value: String::new(),
            };
            context.dom_sender.try_send(vec![update]).map_err(|error| {
                JSError::InternalError(format!("failed to send DOM update: {error}"))
            })?;
            // Synchronously update DomIndex
            if let Ok(mut idx) = context.dom_index.lock() {
                let name_lc = name.to_ascii_lowercase();
                remove_attr_index_sync(&mut idx, node_key, &name_lc);
            }
            Ok(JSValue::Undefined)
        },
    )
}

/// Build getElementsByClassName(name) function.
fn build_get_elements_by_class_name() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.is_empty() {
                return Err(JSError::TypeError(String::from(
                    "getElementsByClassName(name) requires 1 argument",
                )));
            }
            let name = parse_string(&args[0], "name")?;
            let guard = context
                .dom_index
                .lock()
                .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
            let nodes = guard.get_elements_by_class_name(&name);
            let result_string = nodes
                .into_iter()
                .map(|key| key.0.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            Ok(JSValue::String(result_string))
        },
    )
}

/// Build getElementsByTagName(name) function.
fn build_get_elements_by_tag_name() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.is_empty() {
                return Err(JSError::TypeError(String::from(
                    "getElementsByTagName(name) requires 1 argument",
                )));
            }
            let name = parse_string(&args[0], "name")?;
            let guard = context
                .dom_index
                .lock()
                .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
            let nodes = guard.get_elements_by_tag_name(&name);
            let result_string = nodes
                .into_iter()
                .map(|key| key.0.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            Ok(JSValue::String(result_string))
        },
    )
}

/// Build querySelector(selector) function - basic support for #id, .class, tag.
fn build_query_selector() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.is_empty() {
                return Err(JSError::TypeError(String::from(
                    "querySelector(selector) requires 1 argument",
                )));
            }
            let selector = parse_string(&args[0], "selector")?;
            let guard = context
                .dom_index
                .lock()
                .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
            let out: Option<String> = selector.strip_prefix('#').map_or_else(
                || {
                    selector.strip_prefix('.').map_or_else(
                        || {
                            guard
                                .get_elements_by_tag_name(&selector)
                                .into_iter()
                                .next()
                                .map(|key| key.0.to_string())
                        },
                        |class| {
                            guard
                                .get_elements_by_class_name(class)
                                .into_iter()
                                .next()
                                .map(|key| key.0.to_string())
                        },
                    )
                },
                |id| guard.get_element_by_id(id).map(|key| key.0.to_string()),
            );
            out.map_or(Ok(JSValue::Null), |result_string| {
                Ok(JSValue::String(result_string))
            })
        },
    )
}

/// Build querySelectorAll(selector) function - basic support for #id, .class, tag.
fn build_query_selector_all() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.is_empty() {
                return Err(JSError::TypeError(String::from(
                    "querySelectorAll(selector) requires 1 argument",
                )));
            }
            let selector = parse_string(&args[0], "selector")?;
            let guard = context
                .dom_index
                .lock()
                .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
            let nodes = selector.strip_prefix('#').map_or_else(
                || {
                    selector.strip_prefix('.').map_or_else(
                        || guard.get_elements_by_tag_name(&selector),
                        |class| guard.get_elements_by_class_name(class),
                    )
                },
                |id| guard.get_element_by_id(id).into_iter().collect::<Vec<_>>(),
            );
            let result_string = nodes
                .into_iter()
                .map(|key| key.0.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            Ok(JSValue::String(result_string))
        },
    )
}

/// Build getChildIndex(parentKey, childKey) function.
fn build_get_child_index() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.len() < 2 {
                return Err(JSError::TypeError(String::from(
                    "getChildIndex(parentKey, childKey) requires 2 arguments",
                )));
            }
            let parent_key = parse_key(&args[0], "parentKey")?;
            let child_key = parse_key(&args[1], "childKey")?;
            let guard = context
                .dom_index
                .lock()
                .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
            let index_opt = guard
                .children_by_parent
                .get(&parent_key)
                .and_then(|children| children.iter().position(|key| *key == child_key));
            let index_number = index_opt.map_or(-1.0f64, |index| index as f64);
            Ok(JSValue::Number(index_number))
        },
    )
}

/// Build getChildrenKeys(parentKey) function.
fn build_get_children_keys() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.is_empty() {
                return Err(JSError::TypeError(String::from(
                    "getChildrenKeys(parentKey) requires 1 argument",
                )));
            }
            let parent_key = parse_key(&args[0], "parentKey")?;
            let guard = context
                .dom_index
                .lock()
                .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
            let result_string = guard
                .children_by_parent
                .get(&parent_key)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|key| key.0.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            Ok(JSValue::String(result_string))
        },
    )
}

/// Build getTagName(nodeKey) function.
fn build_get_tag_name() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.is_empty() {
                return Err(JSError::TypeError(String::from(
                    "getTagName(nodeKey) requires 1 argument",
                )));
            }
            let node_key = parse_key(&args[0], "nodeKey")?;
            let guard = context
                .dom_index
                .lock()
                .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
            let name = guard.tag_by_key.get(&node_key).cloned().unwrap_or_default();
            Ok(JSValue::String(name))
        },
    )
}

/// Build getParentKey(nodeKey) function.
fn build_get_parent_key() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.is_empty() {
                return Err(JSError::TypeError(String::from(
                    "getParentKey(nodeKey) requires 1 argument",
                )));
            }
            let node_key = parse_key(&args[0], "nodeKey")?;
            let guard = context
                .dom_index
                .lock()
                .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
            guard.parent_by_child.get(&node_key).copied().map_or_else(
                || Ok(JSValue::String(String::new())),
                |parent| Ok(JSValue::String(parent.0.to_string())),
            )
        },
    )
}

/// Build getInnerHTML(nodeKey) function.
fn build_get_inner_html() -> Arc<HostFnSync> {
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
fn build_set_inner_html() -> Arc<HostFnSync> {
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

/// Build `net_request` function - starts an async network request.
fn build_net_request() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            use crate::bindings::net::{fetch_file, fetch_http, FetchDone, FetchEntry};
            use std::env;
            use url::Url;

            let method = args.first().and_then(|value| match value {
                JSValue::String(string_val) => Some(string_val.clone()),
                _ => None,
            }).unwrap_or_else(|| String::from("GET"));
            
            if args.len() < 2 {
                return Err(JSError::TypeError(String::from(
                    "net_requestStart(method, url, [headersJson], [bodyBase64]) requires at least 2 arguments",
                )));
            }
            let url_str = parse_string(&args[1], "url")?;
            let headers_json = args.get(2).and_then(|value| match value {
                JSValue::String(string_val) => Some(string_val.clone()),
                _ => None,
            });

            let body_b64 = args.get(3).and_then(|value| match value {
                JSValue::String(string_val) => Some(string_val.clone()),
                _ => None,
            });

            let relaxed = env::var("VALOR_NET_RELAXED").ok().is_some_and(|val| val == "1" || val.eq_ignore_ascii_case("true"));
            let parsed = Url::parse(&url_str).map_err(|_| JSError::TypeError(format!("invalid URL: {url_str}")))?;
            let allowed = relaxed || matches!(parsed.scheme(), "file") || 
                (parsed.scheme() == "http" && parsed.host_str().is_some_and(|host| host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1"));
            let chrome_restricted = context.page_origin.starts_with("valor://chrome") && matches!(parsed.scheme(), "http" | "https");

            let id = {
                let mut reg = context.fetch_registry.lock().map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
                let id = reg.allocate_id();
                reg.entries.insert(id, FetchEntry::Pending);
                id
            };

            let reg_arc = Arc::clone(&context.fetch_registry);
            let (method_upper, url_clone, url_for_error) = (method.to_ascii_uppercase(), url_str.clone(), url_str);

            context.tokio_handle.spawn({
                let finalize = move |done: FetchDone| {
                    if let Ok(mut reg) = reg_arc.lock() { reg.entries.insert(id, FetchEntry::Done(done)); }
                };
                let err_resp = move |error: String| FetchDone {
                    status: 0, status_text: String::new(), is_ok: false, headers: Vec::new(),
                    body_text: String::new(), body_b64: String::new(), url: url_for_error.clone(), error: Some(error),
                };
                async move {
                    if !allowed || chrome_restricted {
                        finalize(err_resp(String::from("Disallowed by policy")));
                        return;
                    }
                    let result = match parsed.scheme() {
                        "file" => fetch_file(&parsed, url_clone.clone()).await,
                        "http" | "https" => fetch_http(&method_upper, url_clone, headers_json, body_b64).await,
                        scheme => Err(format!("Unsupported scheme: {scheme}")),
                    };
                    finalize(result.unwrap_or_else(err_resp));
                }
            });

            Ok(JSValue::String(id.to_string()))
        },
    )
}

/// Build `net_requestPoll` function - polls the status of an async network request.
fn build_net_request_poll() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            use crate::bindings::net::FetchEntry;

            if args.is_empty() {
                return Err(JSError::TypeError(String::from(
                    "net_requestPoll(id) requires 1 argument",
                )));
            }
            let id: u64 = match &args[0] {
                JSValue::String(string_value) => string_value
                    .parse::<u64>()
                    .map_err(|_| JSError::TypeError(String::from("invalid id")))?,
                _ => return Err(JSError::TypeError(String::from("id must be string"))),
            };
            let reg = context
                .fetch_registry
                .lock()
                .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
            let json = match reg.entries.get(&id) {
                None => serde_json::json!({"state":"error","error":"unknown id"}).to_string(),
                Some(FetchEntry::Pending) => serde_json::json!({"state":"pending"}).to_string(),
                Some(FetchEntry::Done(done)) => serde_json::json!({
                    "state":"done",
                    "status": done.status,
                    "statusText": done.status_text,
                    "ok": done.is_ok,
                    "headers": done.headers,
                    "bodyText": done.body_text,
                    "bodyBase64": done.body_b64,
                    "url": done.url,
                    "error": done.error
                })
                .to_string(),
            };
            Ok(JSValue::String(json))
        },
    )
}

/// Build `storage_getItem` function.
fn build_storage_get_item() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.len() < 2 {
                return Err(JSError::TypeError(String::from(
                    "storage_getItem(kind, key) requires 2 arguments",
                )));
            }
            let kind = match &args[0] {
                JSValue::String(string_value) => string_value.as_str(),
                _ => return Err(JSError::TypeError(String::from("kind must be string"))),
            };
            let key = match &args[1] {
                JSValue::String(string_value) => string_value.clone(),
                _ => return Err(JSError::TypeError(String::from("key must be string"))),
            };
            let origin = context.page_origin.clone();
            let value = match kind {
                "local" => context
                    .storage_local
                    .lock()
                    .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?
                    .get_bucket(&origin)
                    .and_then(|bucket| bucket.get(&key).cloned())
                    .unwrap_or_default(),
                "session" => context
                    .storage_session
                    .lock()
                    .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?
                    .get_bucket(&origin)
                    .and_then(|bucket| bucket.get(&key).cloned())
                    .unwrap_or_default(),
                _ => String::new(),
            };
            Ok(JSValue::String(value))
        },
    )
}

/// Build `storage_setItem` function.
fn build_storage_set_item() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.len() < 3 {
                return Err(JSError::TypeError(String::from(
                    "storage_setItem(kind, key, value) requires 3 arguments",
                )));
            }
            let kind = match &args[0] {
                JSValue::String(string_value) => string_value.as_str(),
                _ => return Err(JSError::TypeError(String::from("kind must be string"))),
            };
            let key = match &args[1] {
                JSValue::String(string_value) => string_value.clone(),
                _ => return Err(JSError::TypeError(String::from("key must be string"))),
            };
            let value = match &args[2] {
                JSValue::String(string_value) => string_value.clone(),
                _ => return Err(JSError::TypeError(String::from("value must be string"))),
            };
            let origin = context.page_origin.clone();
            match kind {
                "local" => {
                    let mut reg = context
                        .storage_local
                        .lock()
                        .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
                    reg.get_bucket_mut(&origin).insert(key, value);
                }
                "session" => {
                    let mut reg = context
                        .storage_session
                        .lock()
                        .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
                    reg.get_bucket_mut(&origin).insert(key, value);
                }
                _ => {}
            }
            Ok(JSValue::Undefined)
        },
    )
}

/// Build `storage_removeItem` function.
fn build_storage_remove_item() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.len() < 2 {
                return Err(JSError::TypeError(String::from(
                    "storage_removeItem(kind, key) requires 2 arguments",
                )));
            }
            let kind = match &args[0] {
                JSValue::String(string_value) => string_value.as_str(),
                _ => return Err(JSError::TypeError(String::from("kind must be string"))),
            };
            let key = match &args[1] {
                JSValue::String(string_value) => string_value.clone(),
                _ => return Err(JSError::TypeError(String::from("key must be string"))),
            };
            let origin = context.page_origin.clone();
            match kind {
                "local" => {
                    if let Ok(mut reg) = context.storage_local.lock() {
                        if let Some(bucket) = reg.buckets.get_mut(&origin) {
                            bucket.remove(&key);
                        }
                    }
                }
                "session" => {
                    if let Ok(mut reg) = context.storage_session.lock() {
                        if let Some(bucket) = reg.buckets.get_mut(&origin) {
                            bucket.remove(&key);
                        }
                    }
                }
                _ => {}
            }
            Ok(JSValue::Undefined)
        },
    )
}

/// Build `storage_clear` function.
fn build_storage_clear() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.is_empty() {
                return Err(JSError::TypeError(String::from(
                    "storage_clear(kind) requires 1 argument",
                )));
            }
            let kind = match &args[0] {
                JSValue::String(string_value) => string_value.as_str(),
                _ => return Err(JSError::TypeError(String::from("kind must be string"))),
            };
            let origin = context.page_origin.clone();
            match kind {
                "local" => {
                    if let Ok(mut reg) = context.storage_local.lock() {
                        if let Some(bucket) = reg.buckets.get_mut(&origin) {
                            bucket.clear();
                        }
                    }
                }
                "session" => {
                    if let Ok(mut reg) = context.storage_session.lock() {
                        if let Some(bucket) = reg.buckets.get_mut(&origin) {
                            bucket.clear();
                        }
                    }
                }
                _ => {}
            }
            Ok(JSValue::Undefined)
        },
    )
}

/// Build `storage_keys` function.
fn build_storage_keys() -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.is_empty() {
                return Err(JSError::TypeError(String::from(
                    "storage_keys(kind) requires 1 argument",
                )));
            }
            let kind = match &args[0] {
                JSValue::String(string_value) => string_value.as_str(),
                _ => return Err(JSError::TypeError(String::from("kind must be string"))),
            };
            let origin = context.page_origin.clone();
            let keys: Vec<String> = match kind {
                "local" => context
                    .storage_local
                    .lock()
                    .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?
                    .get_bucket(&origin)
                    .map(|bucket| bucket.keys().cloned().collect())
                    .unwrap_or_default(),
                "session" => context
                    .storage_session
                    .lock()
                    .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?
                    .get_bucket(&origin)
                    .map(|bucket| bucket.keys().cloned().collect())
                    .unwrap_or_default(),
                _ => Vec::new(),
            };
            Ok(JSValue::String(keys.join(" ")))
        },
    )
}
