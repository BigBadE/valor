//! Document namespace builder with DOM manipulation functions.
//!
//! This module provides the `document` namespace for JavaScript, organized into:
//! - Core DOM operations (createElement, appendChild, etc.)
//! - Query functions (getElementById, querySelector, etc.)
//! - Storage functions (localStorage, sessionStorage)
//! - Network functions (fetch requests)

mod core;
mod network;
mod query;
mod storage;

use crate::bindings::values::{JSError, JSValue};
use crate::bindings::HostNamespace;
use crate::NodeKey;

pub use core::{
    build_append_child, build_append_style_text, build_create_element, build_create_text_node,
    build_create_text_node_root, build_get_inner_html, build_remove_node, build_set_inner_html,
};
pub use network::{build_net_request, build_net_request_poll};
pub use query::{
    build_get_attribute, build_remove_attribute, build_set_attribute, build_set_text_content,
};
pub use query::{
    build_get_child_index, build_get_children_keys, build_get_element_by_id,
    build_get_elements_by_class_name, build_get_elements_by_tag_name, build_get_parent_key,
    build_get_tag_name, build_query_selector, build_query_selector_all,
};
pub use storage::{
    build_storage_clear, build_storage_get_item, build_storage_keys, build_storage_remove_item,
    build_storage_set_item,
};

/// Helper to parse a string argument from `JSValue`.
///
/// # Errors
/// Returns an error if the value is not a string.
pub fn parse_string(value: &JSValue, name: &str) -> Result<String, JSError> {
    match value {
        JSValue::String(string_value) => Ok(string_value.clone()),
        _ => Err(JSError::TypeError(format!("{name} must be a string"))),
    }
}

/// Helper to parse a `NodeKey` from a decimal string `JSValue`.
///
/// # Errors
/// Returns an error if the value is not a string or cannot be parsed as u64.
pub fn parse_key(value: &JSValue, name: &str) -> Result<NodeKey, JSError> {
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
pub fn parse_usize(value: &JSValue) -> Option<usize> {
    match value {
        JSValue::Number(number) if *number >= 0.0f64 && number.is_finite() => {
            Some(number.trunc() as usize)
        }
        _ => None,
    }
}

/// Build the `document` namespace with all DOM manipulation functions.
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
