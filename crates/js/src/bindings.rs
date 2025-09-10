//! Engine-agnostic host bindings facade for registering functions and
//! properties on the JavaScript global object.
//!
//! This module defines a small set of value types and traits that allow
//! Valor to install host-provided namespaces (for example, `console`) into
//! any JavaScript engine adapter without depending on engine-specific APIs.

use anyhow::Result;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

/// An engine-agnostic representation of JavaScript values.
/// This is intentionally small for now; more variants can be added as needed.
#[derive(Clone, Debug)]
pub enum JSValue {
    /// The `undefined` value.
    Undefined,
    /// The `null` value.
    Null,
    /// A boolean primitive.
    Boolean(bool),
    /// A number (IEEE 754 double precision).
    Number(f64),
    /// A string value (UTF-8).
    String(String),
}

/// Error type used by host callbacks.
#[derive(Debug)]
pub enum JSError {
    /// A type error (for example, wrong argument types).
    TypeError(String),
    /// An internal error not exposed to user code in detail.
    InternalError(String),
}

impl Display for JSError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            JSError::TypeError(message) => write!(f, "TypeError: {}", message),
            JSError::InternalError(message) => write!(f, "InternalError: {}", message),
        }
    }
}

impl std::error::Error for JSError {}

/// Log severity levels understood by the host logger.
#[derive(Copy, Clone, Debug)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// Cross-runtime logger used by bindings like `console.*`.
pub trait HostLogger: Send + Sync {
    /// Log a message with a given level.
    fn log(&self, level: LogLevel, message: &str);
}

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
    pub js_node_keys: std::sync::Arc<std::sync::Mutex<crate::NodeKeyManager<u64>>>,
    /// Monotonic local id counter used with js_node_keys.key_of(local_id).
    pub js_local_id_counter: std::sync::Arc<std::sync::atomic::AtomicU64>,
    /// Map of JS-created nodes to their kind and metadata to support appendChild.
    pub js_created_nodes: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<crate::NodeKey, CreatedNodeInfo>>>,
    /// Shared DOM index for element lookup functions (e.g., getElementById).
    pub dom_index: std::sync::Arc<std::sync::Mutex<crate::dom_index::DomIndexState>>,
}

/// A synchronous host function signature.
pub type HostFnSync = dyn Fn(&HostContext, Vec<JSValue>) -> Result<JSValue, JSError>
    + Send
    + Sync
    + 'static;

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
        Self { functions: BTreeMap::new(), properties: BTreeMap::new() }
    }

    /// Register a synchronous function.
    pub fn with_sync_fn(mut self, name: &str, function: Arc<HostFnSync>) -> Self {
        self.functions.insert(name.to_string(), HostFnKind::Sync(function));
        self
    }

    /// Register a constant property.
    pub fn with_property(mut self, name: &str, value: JSValue) -> Self {
        self.properties.insert(name.to_string(), value);
        self
    }
}

/// A collection of namespaces to be installed on the global object.
pub struct HostBindings {
    /// Mapping from namespace name to its definitions.
    pub namespaces: BTreeMap<String, HostNamespace>,
}

impl HostBindings {
    /// Create empty bindings.
    pub fn new() -> Self { Self { namespaces: BTreeMap::new() } }

    /// Add or replace a namespace.
    pub fn with_namespace(mut self, name: &str, namespace: HostNamespace) -> Self {
        self.namespaces.insert(name.to_string(), namespace);
        self
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
    Arc::new(move |context: &HostContext, arguments: Vec<JSValue>| -> Result<JSValue, JSError> {
        let message = stringify_arguments(arguments);
        context.logger.log(level, &message);
        Ok(JSValue::Undefined)
    })
}

/// Build the `console` namespace with standard logging methods.
pub fn build_console_namespace() -> HostNamespace {
    let methods: [(&str, LogLevel); 4] = [
        ("log", LogLevel::Info),
        ("info", LogLevel::Info),
        ("warn", LogLevel::Warn),
        ("error", LogLevel::Error),
    ];

    methods.iter().fold(HostNamespace::new(), |ns, (name, level)| {
        ns.with_sync_fn(name, make_log_fn(*level))
    })
}

/// Build the `document` namespace with minimal DOM manipulation functions.
/// Functions:
/// - createElement(tag: string) -> string (opaque decimal NodeKey id)
/// - createTextNode(text: string) -> string (opaque decimal NodeKey id)
/// - appendChild(parentKey: string, childKey: string, pos?: number) -> undefined
/// - setAttribute(nodeKey: string, name: string, value: string) -> undefined
/// - removeNode(nodeKey: string) -> undefined
pub fn build_document_namespace() -> HostNamespace {
    use crate::{DOMUpdate, NodeKey};
    // Helpers
    let parse_string = |value: &JSValue, name: &str| -> Result<String, JSError> {
        match value {
            JSValue::String(s) => Ok(s.clone()),
            _ => Err(JSError::TypeError(format!("{} must be a string", name))),
        }
    };
    let parse_key = |value: &JSValue, name: &str| -> Result<NodeKey, JSError> {
        match value {
            JSValue::String(s) => {
                let parsed = s.parse::<u64>().map_err(|_| JSError::TypeError(format!("{} must be a decimal string (NodeKey)", name)))?;
                Ok(NodeKey(parsed))
            }
            _ => Err(JSError::TypeError(format!("{} must be a decimal string (NodeKey)", name))),
        }
    };
    let parse_usize = |value: &JSValue| -> Option<usize> {
        match value {
            JSValue::Number(n) if *n >= 0.0 => Some(*n as usize),
            _ => None,
        }
    };

    // createElement(tag)
    let create_element = Arc::new(move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
        if args.len() < 1 { return Err(JSError::TypeError(String::from("createElement(tag) requires 1 argument"))); }
        let tag = parse_string(&args[0], "tag")?;
        let local_id = context.js_local_id_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        let node_key = {
            let mut mgr = context.js_node_keys.lock().map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
            mgr.key_of(local_id)
        };
        // Track kind meta
        if let Ok(mut map) = context.js_created_nodes.lock() {
            map.insert(node_key, CreatedNodeInfo { kind: CreatedNodeKind::Element { tag: tag.clone() } });
        }
        // Immediately insert under root at end so it exists in the DOM; user can reparent later.
        let update = DOMUpdate::InsertElement { parent: NodeKey::ROOT, node: node_key, tag: tag.clone(), pos: usize::MAX };
        context.dom_sender.try_send(vec![update]).map_err(|e| JSError::InternalError(format!("failed to send DOM update: {}", e)))?;
        Ok(JSValue::String(node_key.0.to_string()))
    });

    // createTextNode(text)
    let create_text = Arc::new(move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
        if args.len() < 1 { return Err(JSError::TypeError(String::from("createTextNode(text) requires 1 argument"))); }
        let text = parse_string(&args[0], "text")?;
        let local_id = context.js_local_id_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        let node_key = {
            let mut mgr = context.js_node_keys.lock().map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
            mgr.key_of(local_id)
        };
        if let Ok(mut map) = context.js_created_nodes.lock() {
            map.insert(node_key, CreatedNodeInfo { kind: CreatedNodeKind::Text { text: text.clone() } });
        }
        let update = DOMUpdate::InsertText { parent: NodeKey::ROOT, node: node_key, text: text.clone(), pos: usize::MAX };
        context.dom_sender.try_send(vec![update]).map_err(|e| JSError::InternalError(format!("failed to send DOM update: {}", e)))?;
        Ok(JSValue::String(node_key.0.to_string()))
    });

    // appendChild(parentKey, childKey, pos?)
    let append_child = Arc::new(move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
        if args.len() < 2 { return Err(JSError::TypeError(String::from("appendChild(parentKey, childKey, [pos]) requires 2-3 arguments"))); }
        let parent_key = parse_key(&args[0], "parentKey")?;
        let child_key = parse_key(&args[1], "childKey")?;
        let position = args.get(2).and_then(parse_usize).unwrap_or(usize::MAX);
        // Determine what to insert based on created meta; fallback error if unknown
        let meta = context.js_created_nodes.lock().map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
        if let Some(info) = meta.get(&child_key) {
            use crate::DOMUpdate as DU;
            match &info.kind {
                CreatedNodeKind::Element { tag } => {
                    let update = DU::InsertElement { parent: parent_key, node: child_key, tag: tag.clone(), pos: position };
                    drop(meta);
                    context.dom_sender.try_send(vec![update]).map_err(|e| JSError::InternalError(format!("failed to send DOM update: {}", e)))?;
                }
                CreatedNodeKind::Text { text } => {
                    let update = DU::InsertText { parent: parent_key, node: child_key, text: text.clone(), pos: position };
                    drop(meta);
                    context.dom_sender.try_send(vec![update]).map_err(|e| JSError::InternalError(format!("failed to send DOM update: {}", e)))?;
                }
            }
            Ok(JSValue::Undefined)
        } else {
            Err(JSError::TypeError(String::from("Unknown childKey; create node via document.createElement/createTextNode first")))
        }
    });

    // setAttribute(nodeKey, name, value)
    let set_attribute = Arc::new(move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
        if args.len() < 3 { return Err(JSError::TypeError(String::from("setAttribute(nodeKey, name, value) requires 3 arguments"))); }
        let node_key = parse_key(&args[0], "nodeKey")?;
        let name = parse_string(&args[1], "name")?;
        let value = parse_string(&args[2], "value")?;
        let update = DOMUpdate::SetAttr { node: node_key, name, value };
        context.dom_sender.try_send(vec![update]).map_err(|e| JSError::InternalError(format!("failed to send DOM update: {}", e)))?;
        Ok(JSValue::Undefined)
    });

    // removeNode(nodeKey)
    let remove_node = Arc::new(move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
        if args.len() < 1 { return Err(JSError::TypeError(String::from("removeNode(nodeKey) requires 1 argument"))); }
        let node_key = parse_key(&args[0], "nodeKey")?;
        let update = DOMUpdate::RemoveNode { node: node_key };
        context.dom_sender.try_send(vec![update]).map_err(|e| JSError::InternalError(format!("failed to send DOM update: {}", e)))?;
        Ok(JSValue::Undefined)
    });

    // getElementById(id)
    let get_element_by_id = Arc::new(move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
        if args.len() < 1 { return Err(JSError::TypeError(String::from("getElementById(id) requires 1 argument"))); }
        let id = parse_string(&args[0], "id")?;
        let guard = context.dom_index.lock().map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
        if let Some(key) = guard.get_element_by_id(&id) {
            // Test printing: log the resolved NodeKey for this id
            context.logger.log(LogLevel::Info, &format!("JS: getElementById('{}') -> NodeKey={}", id, key.0));
            Ok(JSValue::String(key.0.to_string()))
        } else {
            context.logger.log(LogLevel::Info, &format!("JS: getElementById('{}') -> null", id));
            Ok(JSValue::Null)
        }
    });

    // setTextContent(nodeKey, text)
    let set_text_content = Arc::new(move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
        if args.len() < 2 { return Err(JSError::TypeError(String::from("setTextContent(nodeKey, text) requires 2 arguments"))); }
        let element_key = parse_key(&args[0], "nodeKey")?;
        let text = parse_string(&args[1], "text")?;
        // Test printing: log the call
        context.logger.log(LogLevel::Info, &format!("JS: setTextContent(nodeKey={}, text='{}')", element_key.0, text));
        // Snapshot current children to remove them
        let children: Vec<NodeKey> = {
            let guard = context.dom_index.lock().map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
            guard.children_by_parent.get(&element_key).cloned().unwrap_or_default()
        };
        // Mint a fresh text node key and remember it
        let local_id = context.js_local_id_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        let text_key = {
            let mut mgr = context.js_node_keys.lock().map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
            mgr.key_of(local_id)
        };
        if let Ok(mut map) = context.js_created_nodes.lock() {
            map.insert(text_key, CreatedNodeInfo { kind: CreatedNodeKind::Text { text: text.clone() } });
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
        for child in children { updates.push(DOMUpdate::RemoveNode { node: child }); }
        updates.push(DOMUpdate::InsertText { parent: element_key, node: text_key, text: text.clone(), pos: 0 });
        // Test printing: log what we're about to send to the DOM
        context.logger.log(
            LogLevel::Info,
            &format!(
                "JS->DOM: setTextContent will send RemoveNode x{} then InsertText(nodeKey={}, parent={}, pos=0)",
                removed_count, text_key.0, element_key.0
            ),
        );
        context.dom_sender.try_send(updates).map_err(|e| JSError::InternalError(format!("failed to send DOM update: {}", e)))?;
        Ok(JSValue::Undefined)
    });

    // getTextContent(nodeKey)
    let get_text_content = Arc::new(move |context: &HostContext, args: Vec<JSValue>| -> Result<JSValue, JSError> {
        if args.len() < 1 { return Err(JSError::TypeError(String::from("getTextContent(nodeKey) requires 1 argument"))); }
        let node_key = parse_key(&args[0], "nodeKey")?;
        let guard = context.dom_index.lock().map_err(|_| JSError::InternalError(String::from("mutex poisoned")))?;
        let text = guard.get_text_content(node_key);
        Ok(JSValue::String(text))
    });

    HostNamespace::new()
        .with_sync_fn("createElement", create_element)
        .with_sync_fn("createTextNode", create_text)
        .with_sync_fn("appendChild", append_child)
        .with_sync_fn("setAttribute", set_attribute)
        .with_sync_fn("removeNode", remove_node)
        .with_sync_fn("getElementById", get_element_by_id)
        .with_sync_fn("setTextContent", set_text_content)
        .with_sync_fn("getTextContent", get_text_content)
}

/// Build the default set of host bindings to install into a JS engine.
/// Currently includes:
/// - `console` namespace with logging methods.
pub fn build_default_bindings() -> HostBindings {
    HostBindings::new()
        .with_namespace("console", build_console_namespace())
        .with_namespace("document", build_document_namespace())
}

/// Convert a vector of JSValue to a space-separated string.
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
