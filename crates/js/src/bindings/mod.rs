//! Engine-agnostic host bindings facade for registering functions and
//! properties on the JavaScript global object.
//!
//! This module defines a small set of value types and traits that allow
//! Valor to install host-provided namespaces (for example, `console`) into
//! any JavaScript engine adapter without depending on engine-specific APIs.

#![allow(clippy::cast_sign_loss)]

use crate::{DOMUpdate, NodeKey};
use anyhow::Result;
// serde_json used via fully qualified calls (serde_json::json)
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::mpsc::UnboundedSender;
use url::Url;
mod values;
pub use values::{JSError, JSValue, LogLevel};
mod logger;
pub use logger::HostLogger;
mod net;
use net::{fetch_file, fetch_http, FetchDone, FetchEntry, FetchRegistry};

mod dom;
mod storage;
mod util;
use dom::{apply_inner_html, remove_attr_index_sync, serialize_node, set_attr_index_sync};
pub use storage::StorageRegistry;

/// Shorthand for the created nodes registry to keep field types simple.
type CreatedNodeMap = HashMap<crate::NodeKey, CreatedNodeInfo>;

// DOM helpers moved to dom.rs

/// Execution context passed to host callbacks (for example, for logging).
#[derive(Clone)]
pub struct HostContext {
    /// Optional page identifier for context (reserved for future use).
    pub page_id: Option<u64>,
    /// Logger used by host functions such as `console.*`.
    pub logger: Arc<dyn HostLogger>,
    /// Channel for posting DOM updates from host functions (document namespace).
    pub dom_sender: tokio::sync::mpsc::Sender<Vec<crate::DOMUpdate>>,
    /// NodeKey manager for minting stable keys for JS-created nodes (shared via Arc+Mutex).
    pub js_node_keys: Arc<Mutex<crate::NodeKeyManager<u64>>>,
    /// Monotonic local id counter used with js_node_keys.key_of(local_id).
    pub js_local_id_counter: Arc<AtomicU64>,
    /// Map of JS-created nodes to their kind and metadata to support appendChild.
    pub js_created_nodes: Arc<Mutex<CreatedNodeMap>>,
    /// Shared DOM index for element lookup functions (e.g., getElementById).
    pub dom_index: Arc<Mutex<crate::dom_index::DomIndexState>>,
    /// Tokio runtime handle for spawning async network tasks.
    pub tokio_handle: tokio::runtime::Handle,
    /// Origin of the current page (scheme+host+port minimal string) for same-origin checks.
    pub page_origin: String,
    /// Shared network request registry to communicate between host and JS fetch/XHR polyfills.
    pub fetch_registry: Arc<Mutex<FetchRegistry>>,
    /// High-resolution time origin for performance.now (Instant at page start).
    pub performance_start: Instant,
    /// Local storage buckets per origin (string key/value). In-memory only.
    pub storage_local: Arc<Mutex<StorageRegistry>>,
    /// Session storage buckets per origin (per page session, separate from local).
    pub storage_session: Arc<Mutex<StorageRegistry>>,
    /// Optional command channel exposed only to the privileged valor://chrome origin for controlling the host app.
    pub chrome_host_tx: Option<tokio::sync::mpsc::UnboundedSender<ChromeHostCommand>>,
}

/// A synchronous host function signature.
pub type HostFnSync =
    dyn Fn(&HostContext, Vec<JSValue>) -> Result<JSValue, JSError> + Send + Sync + 'static;

/// A single function descriptor the engine adapter can install.
#[derive(Clone)]
pub enum HostFnKind {
    /// Synchronous function.
    Sync(Arc<HostFnSync>),
}

/// A namespaced set of functions and properties (for example, the `console` object).
pub struct HostNamespace {
    /// Functions to install under this namespace.
    pub functions: BTreeMap<String, HostFnKind>,
    /// Constant properties to install under this namespace.
    pub properties: BTreeMap<String, JSValue>,
}

impl HostNamespace {
    /// Create an empty namespace.
    pub fn new() -> Self {
        Self {
            functions: BTreeMap::new(),
            properties: BTreeMap::new(),
        }
    }

    /// Register a synchronous function.
    pub fn with_sync_fn(mut self, name: &str, function: Arc<HostFnSync>) -> Self {
        self.functions
            .insert(name.to_string(), HostFnKind::Sync(function));
        self
    }

    /// Register a constant property.
    pub fn with_property(mut self, name: &str, value: JSValue) -> Self {
        self.properties.insert(name.to_string(), value);
        self
    }
}

impl Default for HostNamespace {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

// Networking types moved to net.rs

// StorageRegistry is defined in bindings/storage.rs

// allocation implemented in net.rs

/// A collection of namespaces to be installed on the global object.
pub struct HostBindings {
    /// Mapping from namespace name to its definitions.
    pub namespaces: BTreeMap<String, HostNamespace>,
}

/// Commands emitted by privileged chrome pages (`<valor://chrome>`) to control the host app.
#[derive(Clone, Debug)]
pub enum ChromeHostCommand {
    /// Navigate the primary content page to the given URL string.
    Navigate(String),
    /// Navigate back in history (stub if not supported yet).
    Back,
    /// Navigate forward in history (stub if not supported yet).
    Forward,
    /// Reload the current content page.
    Reload,
    /// Open a new tab optionally with a URL.
    OpenTab(Option<String>),
    /// Close a tab by ID (opaque to the app); None closes the current.
    CloseTab(Option<u64>),
}

impl HostBindings {
    /// Create empty bindings.
    #[inline]
    pub const fn new() -> Self {
        Self {
            namespaces: BTreeMap::new(),
        }
    }

    /// Add or replace a namespace.
    #[inline]
    #[must_use]
    pub fn with_namespace(mut self, name: &str, namespace: HostNamespace) -> Self {
        self.namespaces.insert(name.to_owned(), namespace);
        self
    }
}

impl Default for HostBindings {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

/// A record for JS-created nodes so we can re-insert them later appropriately.
#[derive(Clone, Debug)]
pub enum CreatedNodeKind {
    /// An element node with a tag name.
    Element { tag: String },
    /// A text node with content.
    Text { text: String },
}

/// Metadata tracked per JS-created node.
#[derive(Clone, Debug)]
pub struct CreatedNodeInfo {
    /// The kind of node created (element or text) and its data.
    pub kind: CreatedNodeKind,
}

/// Internal helper to build a console logging function for a given level.
fn make_log_fn(level: LogLevel) -> Arc<HostFnSync> {
    Arc::new(
        move |context: &HostContext, arguments: Vec<JSValue>| -> Result<JSValue, JSError> {
            let message = stringify_arguments(arguments);
            context.logger.log(level, &message);
            Ok(JSValue::Undefined)
        },
    )
}

/// Build the `console` namespace with standard logging methods.
#[inline]
pub fn build_console_namespace() -> HostNamespace {
    let methods: [(&str, LogLevel); 4] = [
        ("log", LogLevel::Info),
        ("info", LogLevel::Info),
        ("warn", LogLevel::Warn),
        ("error", LogLevel::Error),
    ];

    methods
        .iter()
        .fold(HostNamespace::new(), |namespace, (name, level)| {
            let console_fn = Arc::new(
                move |context: &HostContext, arguments: Vec<JSValue>| -> Result<JSValue, JSError> {
                    let message = stringify_arguments(arguments);
                    context.logger.log(*level, &message);
                    Ok(JSValue::Undefined)
                },
            );
            namespace.with_sync_fn(name, console_fn)
        })
}

/// Build the `document` namespace with minimal DOM manipulation functions.
/// Functions:
/// - createElement(tag: string) -> string (opaque decimal `NodeKey` id)
/// - createTextNode(text: string) -> string (opaque decimal `NodeKey` id)
/// - appendChild(parentKey: string, childKey: string, pos?: number) -> undefined
/// - setAttribute(nodeKey: string, name: string, value: string) -> undefined
/// - removeNode(nodeKey: string) -> undefined
pub fn build_document_namespace() -> HostNamespace {
    use crate::{DOMUpdate, NodeKey};
    // Helpers
    let parse_string = |value: &JSValue, name: &str| -> Result<String, JSError> {
        match value {
            JSValue::String(string_value) => Ok(string_value.clone()),
            _ => Err(JSError::TypeError(format!("{name} must be a string"))),
        }
    };
    let parse_key = |value: &JSValue, name: &str| -> Result<NodeKey, JSError> {
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
    };
    let parse_usize = |value: &JSValue| -> Option<usize> {
        match value {
            JSValue::Number(number) if *number >= 0.0_f64 => Some(*number as usize),
            _ => None,
        }
    };

    // createElement(tag)
    let create_element = Arc::new(
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
    );

    // appendStyleText(cssText) -> create <style data-valor-test-reset="1">cssText</style> under <head>
    let append_style_text = Arc::new(
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
            // Resolve <head> key if present, otherwise attach to ROOT
            let head_key_opt: Option<NodeKey> = {
                let guard = context
                    .dom_index
                    .lock()
                    .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
                // Prefer first child of <html> with tag 'head'; else try any 'head' in index
                let mut found: Option<NodeKey> = None;
                for (key, tag) in &guard.tag_by_key {
                    if tag.eq_ignore_ascii_case("head") {
                        found = Some(*key);
                        break;
                    }
                }
                found
            };
            let parent_key = head_key_opt.unwrap_or(NodeKey::ROOT);

            // Mint element and text node keys
            let style_key = {
                let local_id = context
                    .js_local_id_counter
                    .fetch_add(1, Ordering::Relaxed)
                    .saturating_add(1);
                let mut mgr = context
                    .js_node_keys
                    .lock()
                    .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
                mgr.key_of(local_id)
            };
            if let Ok(mut map) = context.js_created_nodes.lock() {
                map.insert(
                    style_key,
                    CreatedNodeInfo {
                        kind: CreatedNodeKind::Element {
                            tag: String::from("style"),
                        },
                    },
                );
            }
            let text_key = {
                let local_id = context
                    .js_local_id_counter
                    .fetch_add(1, Ordering::Relaxed)
                    .saturating_add(1);
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
                        kind: CreatedNodeKind::Text {
                            text: css_text.clone(),
                        },
                    },
                );
            }

            // Update DomIndex eagerly
            if let Ok(mut idx) = context.dom_index.lock() {
                // Insert style under parent
                dom::reparent_child(&mut idx, style_key, parent_key, usize::MAX);
                idx.tag_by_key.insert(style_key, String::from("style"));
                // Insert text under style at position 0
                dom::reparent_child(&mut idx, text_key, style_key, 0);
                idx.text_by_key.insert(text_key, css_text.clone());
            }

            // Emit updates: InsertElement(style under parent) + SetAttr(data-valor-test-reset) + InsertText(text under style)
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
    );

    // createTextNode(text)
    let create_text = Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.is_empty() {
                return Err(JSError::TypeError(String::from(
                    "createTextNode(text) requires 1 argument",
                )));
            }
            let text = parse_string(&args[0], "text")?;
            let local_id = context
                .js_local_id_counter
                .fetch_add(1, Ordering::Relaxed)
                .saturating_add(1);
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
    );

    // appendChild(parentKey, childKey, pos?)
    let append_child = Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.len() < 2 {
                return Err(JSError::TypeError(String::from(
                    "appendChild(parentKey, childKey, [pos]) requires 2-3 arguments",
                )));
            }
            let parent_key = parse_key(&args[0], "parentKey")?;
            let child_key = parse_key(&args[1], "childKey")?;
            let position = args.get(2).and_then(parse_usize).unwrap_or(usize::MAX);
            // Determine what to insert based on created meta; fallback error if unknown
            let meta = context
                .js_created_nodes
                .lock()
                .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
            if let Some(info) = meta.get(&child_key) {
                use crate::DOMUpdate;
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
                    dom::reparent_child(&mut idx, child_key, parent_key, position);
                }
                Ok(JSValue::Undefined)
            } else {
                Err(JSError::TypeError(String::from(
                    "Unknown childKey; create node via document.createElement/createTextNode first",
                )))
            }
        },
    );

    // setAttribute(nodeKey, name, value)
    let set_attribute = Arc::new(
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
    );

    // removeNode(nodeKey)
    let remove_node = Arc::new(
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
    );

    // getElementById(id)
    let get_element_by_id = Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
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
                    // Test printing: log the resolved NodeKey for this id
                    context.logger.log(
                        LogLevel::Info,
                        &format!("JS: getElementById('{id}') -> NodeKey={}", key.0),
                    );
                    Ok(JSValue::String(key.0.to_string()))
                },
            )
        },
    );

    // setTextContent(nodeKey, text)
    let set_text_content = Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.len() < 2 {
                return Err(JSError::TypeError(String::from(
                    "setTextContent(nodeKey, text) requires 2 arguments",
                )));
            }
            let element_key = parse_key(&args[0], "nodeKey")?;
            let text = parse_string(&args[1], "text")?;
            // Test printing: log the call
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
            // Mint a fresh text node key and remember it
            let local_id = context
                .js_local_id_counter
                .fetch_add(1, Ordering::Relaxed)
                .saturating_add(1);
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
            // Synchronously update the shared DomIndex so immediate getters observe the change
            if let Ok(mut guard) = context.dom_index.lock() {
                // Remove existing children (and subtrees) from indices
                for child in &children {
                    guard.remove_node_and_descendants(*child);
                }
                // Replace parent's children with the new text node at position 0
                guard.children_by_parent.insert(element_key, vec![text_key]);
                guard.parent_by_child.insert(text_key, element_key);
                guard.text_by_key.insert(text_key, text.clone());
            }
            // Build a batch: remove existing children, then insert new text node at position 0
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
            // Test printing: log what we're about to send to the DOM
            context.logger.log(
            LogLevel::Info,
            &format!(
                "JS->DOM: setTextContent will send RemoveNode x{removed_count} then InsertText(nodeKey={}, parent={}, pos=0)",
                text_key.0, element_key.0
            ),
        );
            context.dom_sender.try_send(updates).map_err(|error| {
                JSError::InternalError(format!("failed to send DOM update: {error}"))
            })?;
            Ok(JSValue::Undefined)
        },
    );

    // getTextContent(nodeKey)
    let get_text_content = Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.is_empty() {
                return Err(JSError::TypeError(String::from(
                    "getTextContent(nodeKey) requires 1 argument",
                )));
            }
            let node_key = parse_key(&args[0], "nodeKey")?;
            let guard = context
                .dom_index
                .lock()
                .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
            let text = guard.get_text_content(node_key);
            Ok(JSValue::String(text))
        },
    );

    // getAttribute(nodeKey, name)
    let get_attribute = Arc::new(
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
            } else if name_lc.starts_with("data-") {
                // We do not index arbitrary attributes yet; return empty unless set from JS via setAttribute
                String::new()
            } else {
                String::new()
            };
            Ok(JSValue::String(value))
        },
    );

    // removeAttribute(nodeKey, name) -> implemented as SetAttr with empty string
    let remove_attribute = Arc::new(
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
            // Synchronously update DomIndex for immediate queries
            if let Ok(mut idx) = context.dom_index.lock() {
                let name_lc = name.to_ascii_lowercase();
                remove_attr_index_sync(&mut idx, node_key, &name_lc);
            }
            Ok(JSValue::Undefined)
        },
    );

    // getElementsByClassName(name) -> space-separated NodeKey list
    let get_elements_by_class_name = Arc::new(
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
    );

    // getElementsByTagName(name) -> space-separated NodeKey list
    let get_elements_by_tag_name = Arc::new(
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
    );

    // querySelector(selector) basic: #id, .class, tag
    let query_selector = Arc::new(
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
    );

    // querySelectorAll(selector) -> space-separated NodeKey list (basic)
    let query_selector_all = Arc::new(
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
    );

    // getChildIndex(parentKey, childKey) -> number (or -1)
    let get_child_index = Arc::new(
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
            let index_number = index_opt.map_or(-1.0_f64, |index| index as f64);
            Ok(JSValue::Number(index_number))
        },
    );

    // getChildrenKeys(parentKey) -> space-separated NodeKey list
    let get_children_keys = Arc::new(
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
    );

    // getTagName(nodeKey) -> lowercase tag name or empty string
    let get_tag_name = Arc::new(
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
    );

    // getParentKey(nodeKey) -> parent key as string or empty string
    let get_parent_key = Arc::new(
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
    );

    // getInnerHTML(nodeKey) -> serialize subtree of children (basic attrs: id/class)
    let get_inner_html = Arc::new(
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
    );

    // setInnerHTML(nodeKey, html) -> replace children with parsed fragment (basic subset)
    let set_inner_html = Arc::new(
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
    );

    // =====================
    // Networking host fns
    // =====================
    let net_request_start = Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            // Args: method, url, headersJson (optional), bodyBase64 (optional)
            let method = if args.is_empty() {
                String::from("GET")
            } else {
                match &args[0] {
                    JSValue::String(string_value) => string_value.clone(),
                    _ => String::from("GET"),
                }
            };
            if args.len() < 2 {
                return Err(JSError::TypeError(String::from("net_requestStart(method, url, [headersJson], [bodyBase64]) requires at least 2 arguments")));
            }
            let url_str = match &args[1] {
                JSValue::String(string_value) => string_value.clone(),
                _ => return Err(JSError::TypeError(String::from("url must be a string"))),
            };
            let headers_json = match args.get(2) {
                Some(JSValue::String(string_value)) => Some(string_value.clone()),
                _ => None,
            };
            let body_b64 = match args.get(3) {
                Some(JSValue::String(string_value)) => Some(string_value.clone()),
                _ => None,
            };

            // Same-origin allow-list check: file:// and http://localhost (and 127.0.0.1)
            let relaxed = env::var("VALOR_NET_RELAXED")
                .ok()
                .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
            let parsed = Url::parse(&url_str)
                .map_err(|_| JSError::TypeError(format!("invalid URL: {url_str}")))?;
            let allowed = if relaxed {
                true
            } else {
                match parsed.scheme() {
                    "file" => true,
                    "http" => parsed.host_str().is_some_and(|host| {
                        host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1"
                    }),
                    _ => false,
                }
            };

            // Allocate id and insert Pending
            let id = {
                let mut reg = context
                    .fetch_registry
                    .lock()
                    .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
                let id = reg.allocate_id();
                reg.entries.insert(id, FetchEntry::Pending);
                id
            };

            let reg_arc = Arc::clone(&context.fetch_registry);
            let method_upper = method.to_ascii_uppercase();
            let url_clone = url_str;
            let headers_json_clone = headers_json;
            let body_b64_clone = body_b64;
            // Precompute flags and values to avoid borrowing `context` inside async
            let chrome_restricted = context.page_origin.starts_with("valor://chrome")
                && (parsed.scheme() == "http" || parsed.scheme() == "https");
            let url_for_error = url_clone.clone();

            context.tokio_handle.spawn({
                let finalize_with = move |done: FetchDone| {
                    if let Ok(mut reg) = reg_arc.lock() {
                        reg.entries.insert(id, FetchEntry::Done(done));
                    }
                };

                // Create error response helper
                let error_response = move |error: String| FetchDone {
                    status: 0,
                    status_text: String::new(),
                    ok: false,
                    headers: Vec::new(),
                    body_text: String::new(),
                    body_b64: String::new(),
                    url: url_for_error.clone(),
                    error: Some(error),
                };

                async move {
                    // Check permissions
                    if !allowed {
                        finalize_with(error_response(String::from("Disallowed by policy")));
                        return;
                    }

                    // Check chrome origin restrictions
                    if chrome_restricted {
                        finalize_with(error_response(String::from("Disallowed by policy")));
                        return;
                    }

                    // Success response is constructed inside fetch helpers.

                    // Delegate to helpers based on scheme
                    let scheme = parsed.scheme();
                    match scheme {
                        "file" => match fetch_file(&parsed, url_clone.clone()).await {
                            Ok(done) => finalize_with(done),
                            Err(err) => finalize_with(error_response(err)),
                        },
                        "http" | "https" => match fetch_http(
                            &method_upper,
                            url_clone.clone(),
                            headers_json_clone.clone(),
                            body_b64_clone.clone(),
                        )
                        .await
                        {
                            Ok(done) => finalize_with(done),
                            Err(err) => finalize_with(error_response(err)),
                        },
                        _ => finalize_with(error_response(format!("Unsupported scheme: {scheme}"))),
                    }
                }
            });

            Ok(JSValue::String(id.to_string()))
        },
    );

    let net_request_poll = Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
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
                    "ok": done.ok,
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
    );

    // =====================
    // Storage host functions
    // =====================
    let storage_get_item = Arc::new(
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
    );
    let storage_has_item = Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            if args.len() < 2 {
                return Err(JSError::TypeError(String::from(
                    "storage_hasItem(kind, key) requires 2 arguments",
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
            let exists = match kind {
                "local" => context
                    .storage_local
                    .lock()
                    .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?
                    .get_bucket(&origin)
                    .is_some_and(|bucket| bucket.contains_key(&key)),
                "session" => context
                    .storage_session
                    .lock()
                    .map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?
                    .get_bucket(&origin)
                    .is_some_and(|bucket| bucket.contains_key(&key)),
                _ => false,
            };
            Ok(JSValue::Boolean(exists))
        },
    );
    let storage_set_item = Arc::new(
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
    );
    let storage_remove_item = Arc::new(
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
    );
    let storage_clear = Arc::new(
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
    );
    let storage_keys = Arc::new(
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
    );

    HostNamespace::new()
        .with_sync_fn("createElement", create_element)
        .with_sync_fn("createTextNode", create_text)
        .with_sync_fn("appendChild", append_child)
        .with_sync_fn("setAttribute", set_attribute)
        .with_sync_fn("removeNode", remove_node)
        .with_sync_fn("getElementById", get_element_by_id)
        .with_sync_fn("setTextContent", set_text_content)
        .with_sync_fn("getTextContent", get_text_content)
        .with_sync_fn("getAttribute", get_attribute)
        .with_sync_fn("removeAttribute", remove_attribute)
        .with_sync_fn("getElementsByClassName", get_elements_by_class_name)
        .with_sync_fn("getElementsByTagName", get_elements_by_tag_name)
        .with_sync_fn("querySelector", query_selector)
        .with_sync_fn("querySelectorAll", query_selector_all)
        .with_sync_fn("getChildIndex", get_child_index)
        .with_sync_fn("getChildrenKeys", get_children_keys)
        .with_sync_fn("appendStyleText", append_style_text)
        .with_sync_fn("getTagName", get_tag_name)
        .with_sync_fn("getParentKey", get_parent_key)
        .with_sync_fn("getInnerHTML", get_inner_html)
        .with_sync_fn("setInnerHTML", set_inner_html)
        .with_sync_fn("net_requestStart", net_request_start)
        .with_sync_fn("net_requestPoll", net_request_poll)
        .with_sync_fn("storage_getItem", storage_get_item)
        .with_sync_fn("storage_hasItem", storage_has_item)
        .with_sync_fn("storage_setItem", storage_set_item)
        .with_sync_fn("storage_removeItem", storage_remove_item)
        .with_sync_fn("storage_clear", storage_clear)
        .with_sync_fn("storage_keys", storage_keys)
}

/// Build the default set of host bindings to install into a JS engine.
/// Currently includes:
/// - `console` namespace with logging methods.
#[inline]
pub fn build_default_bindings() -> HostBindings {
    HostBindings::new()
        .with_namespace("console", build_console_namespace())
        .with_namespace("document", build_document_namespace())
        .with_namespace("performance", build_performance_namespace())
}

/// Build the `chromeHost` namespace. Functions are gated to `<valor://chrome>` origin
/// and require an attached host command channel in `HostContext`.
#[inline]
#[allow(clippy::too_many_lines)]
pub fn build_chrome_host_namespace() -> HostNamespace {
    // Helper to check privilege and get sender
    let get_sender =
        |context: &HostContext| -> Result<UnboundedSender<ChromeHostCommand>, JSError> {
            if !context.page_origin.starts_with("valor://chrome") {
                return Err(JSError::TypeError(String::from(
                    "chromeHost is not available",
                )));
            }
            context
                .chrome_host_tx
                .clone()
                .ok_or_else(|| JSError::TypeError(String::from("chromeHost is not available")))
        };

    let navigate_fn = Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            let sender = get_sender(context)?;
            if args.is_empty() {
                return Err(JSError::TypeError(String::from(
                    "navigate(url) requires 1 argument",
                )));
            }
            let url = match &args[0] {
                JSValue::String(string_value) => string_value.clone(),
                _ => return Err(JSError::TypeError(String::from("url must be a string"))),
            };
            sender
                .send(ChromeHostCommand::Navigate(url))
                .map_err(|error| {
                    JSError::InternalError(format!("failed to send navigate: {error}"))
                })?;
            Ok(JSValue::Undefined)
        },
    );

    let back_fn = Arc::new(
        move |context: &HostContext, _args: Vec<JSValue>| -> Result<JSValue, JSError> {
            let sender = get_sender(context)?;
            sender
                .send(ChromeHostCommand::Back)
                .map_err(|error| JSError::InternalError(format!("failed to send back: {error}")))?;
            Ok(JSValue::Undefined)
        },
    );

    let forward_fn = Arc::new(
        move |context: &HostContext, _args: Vec<JSValue>| -> Result<JSValue, JSError> {
            let sender = get_sender(context)?;
            sender.send(ChromeHostCommand::Forward).map_err(|error| {
                JSError::InternalError(format!("failed to send forward: {error}"))
            })?;
            Ok(JSValue::Undefined)
        },
    );

    let reload_fn = Arc::new(
        move |context: &HostContext, _args: Vec<JSValue>| -> Result<JSValue, JSError> {
            let sender = get_sender(context)?;
            sender.send(ChromeHostCommand::Reload).map_err(|error| {
                JSError::InternalError(format!("failed to send reload: {error}"))
            })?;
            Ok(JSValue::Undefined)
        },
    );

    let open_tab_fn = Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            let sender = get_sender(context)?;
            let url_opt = match args.first() {
                Some(JSValue::String(string_value)) => Some(string_value.clone()),
                _ => None,
            };
            sender
                .send(ChromeHostCommand::OpenTab(url_opt))
                .map_err(|error| {
                    JSError::InternalError(format!("failed to send openTab: {error}"))
                })?;
            Ok(JSValue::Undefined)
        },
    );

    let close_tab_fn = Arc::new(
        move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
            let sender = get_sender(context)?;
            let id_opt = match args.first() {
                Some(JSValue::Number(number_value)) => Some(*number_value as u64),
                _ => None,
            };
            sender
                .send(ChromeHostCommand::CloseTab(id_opt))
                .map_err(|error| {
                    JSError::InternalError(format!("failed to send closeTab: {error}"))
                })?;
            Ok(JSValue::Undefined)
        },
    );

    HostNamespace::new()
        .with_sync_fn("navigate", navigate_fn)
        .with_sync_fn("back", back_fn)
        .with_sync_fn("forward", forward_fn)
        .with_sync_fn("reload", reload_fn)
        .with_sync_fn("openTab", open_tab_fn)
        .with_sync_fn("closeTab", close_tab_fn)
}

/// `HostBindings` bundle containing only the `chromeHost` namespace.
#[inline]
pub fn build_chrome_host_bindings() -> HostBindings {
    HostBindings::new().with_namespace("chromeHost", build_chrome_host_namespace())
}

/// Convert a vector of `JSValue` to a space-separated string.
#[inline]
pub fn stringify_arguments(arguments: Vec<JSValue>) -> String {
    arguments
        .into_iter()
        .map(|value| match value {
            JSValue::Undefined => String::from("undefined"),
            JSValue::Null => String::from("null"),
            JSValue::Boolean(value) => value.to_string(),
            JSValue::Number(value) => value.to_string(),
            JSValue::String(value) => value,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Build the `performance` namespace with a high-resolution `now()` function.
#[inline]
pub fn build_performance_namespace() -> HostNamespace {
    let now_fn = Arc::new(
        move |context: &HostContext, _args: Vec<JSValue>| -> Result<JSValue, JSError> {
            let elapsed = Instant::now().duration_since(context.performance_start);
            let milliseconds = elapsed.as_secs_f64() * 1_000.0_f64;
            Ok(JSValue::Number(milliseconds))
        },
    );
    HostNamespace::new().with_sync_fn("now", now_fn)
}
