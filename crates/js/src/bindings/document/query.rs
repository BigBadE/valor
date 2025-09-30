//! Query functions for DOM element selection.

use crate::bindings::document::{parse_key, parse_string};
use crate::bindings::dom::{remove_attr_index_sync, set_attr_index_sync};
use crate::bindings::values::{JSError, JSValue};
use crate::bindings::{CreatedNodeInfo, CreatedNodeKind, HostContext, HostFnSync};
use crate::{DOMUpdate, NodeKey};
use core::sync::atomic::Ordering;
use std::sync::Arc;
/// Build getElementById(id) function.
#[inline]
pub fn build_get_element_by_id() -> Arc<HostFnSync> {
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
#[inline]
pub fn build_set_text_content() -> Arc<HostFnSync> {
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
#[inline]
pub fn build_set_attribute() -> Arc<HostFnSync> {
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
#[inline]
pub fn build_get_attribute() -> Arc<HostFnSync> {
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
#[inline]
pub fn build_remove_attribute() -> Arc<HostFnSync> {
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
#[inline]
pub fn build_get_elements_by_class_name() -> Arc<HostFnSync> {
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
#[inline]
pub fn build_get_elements_by_tag_name() -> Arc<HostFnSync> {
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
#[inline]
pub fn build_query_selector() -> Arc<HostFnSync> {
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
#[inline]
pub fn build_query_selector_all() -> Arc<HostFnSync> {
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
#[inline]
pub fn build_get_child_index() -> Arc<HostFnSync> {
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
#[inline]
pub fn build_get_children_keys() -> Arc<HostFnSync> {
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
#[inline]
pub fn build_get_tag_name() -> Arc<HostFnSync> {
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
#[inline]
pub fn build_get_parent_key() -> Arc<HostFnSync> {
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
