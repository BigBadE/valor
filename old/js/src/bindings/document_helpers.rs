//! Helper functions for document namespace operations.

use crate::bindings::values::{JSError, JSValue};
use crate::bindings::{CreatedNodeInfo, CreatedNodeKind, HostContext};
use crate::NodeKey;
use core::sync::atomic::Ordering;

/// Type alias for network request arguments.
pub type NetRequestArgs = (String, String, Option<String>, Option<String>);

/// Find the <head> element in the DOM index, if present.
///
/// # Errors
/// Returns an error if the DOM index mutex is poisoned.
pub fn find_head_element(context: &HostContext) -> Result<Option<NodeKey>, JSError> {
    let guard = context
        .dom_index
        .lock()
        .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
    let mut sorted_tags: Vec<(NodeKey, String)> = guard
        .tag_by_key
        .iter()
        .map(|(key, value)| (*key, value.clone()))
        .collect();
    sorted_tags.sort_by_key(|item| item.0 .0);
    for (key, tag) in sorted_tags {
        if tag.eq_ignore_ascii_case("head") {
            return Ok(Some(key));
        }
    }
    Ok(None)
}

/// Create a new node key for a JS-created node.
///
/// # Errors
/// Returns an error if the node keys mutex is poisoned.
pub fn create_node_key(context: &HostContext) -> Result<NodeKey, JSError> {
    let local_id = context.js_local_id_counter.fetch_add(1, Ordering::Relaxed) + 1;
    let mut mgr = context
        .js_node_keys
        .lock()
        .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
    Ok(mgr.key_of(local_id))
}

/// Register a created element node.
pub fn register_element_node(context: &HostContext, node_key: NodeKey, tag: String) {
    if let Ok(mut map) = context.js_created_nodes.lock() {
        map.insert(
            node_key,
            CreatedNodeInfo {
                kind: CreatedNodeKind::Element { tag },
            },
        );
    }
}

/// Register a created text node.
pub fn register_text_node(context: &HostContext, node_key: NodeKey, text: String) {
    if let Ok(mut map) = context.js_created_nodes.lock() {
        map.insert(
            node_key,
            CreatedNodeInfo {
                kind: CreatedNodeKind::Text { text },
            },
        );
    }
}

/// Parse network request arguments.
///
/// # Errors
/// Returns an error if arguments are invalid.
pub fn parse_net_request_args(args: &[JSValue]) -> Result<NetRequestArgs, JSError> {
    let method = args
        .first()
        .and_then(|value| match value {
            JSValue::String(string_val) => Some(string_val.clone()),
            _ => None,
        })
        .unwrap_or_else(|| String::from("GET"));
    if args.len() < 2 {
        return Err(JSError::TypeError(String::from(
            "net_requestStart(method, url, [headersJson], [bodyBase64]) requires at least 2 arguments",
        )));
    }
    let url_str = match &args[1] {
        JSValue::String(string_value) => string_value.clone(),
        _ => return Err(JSError::TypeError(String::from("url must be a string"))),
    };
    let headers_json = args.get(2).and_then(|value| match value {
        JSValue::String(string_val) => Some(string_val.clone()),
        _ => None,
    });
    let body_b64 = args.get(3).and_then(|value| match value {
        JSValue::String(string_val) => Some(string_val.clone()),
        _ => None,
    });
    Ok((method, url_str, headers_json, body_b64))
}
