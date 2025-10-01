//! Engine-agnostic host bindings facade for registering functions and
//! properties on the JavaScript global object.
//!
//! This module defines a small set of value types and traits that allow
//! Valor to install host-provided namespaces (for example, `console`) into
//! any JavaScript engine adapter without depending on engine-specific APIs.

#![allow(
    clippy::cast_sign_loss,
    reason = "NodeKey conversions require casting from signed to unsigned"
)]

use crate::dom_index::DomIndexState;
use crate::{DOMUpdate, NodeKey, NodeKeyManager};
use anyhow::Result;
use core::sync::atomic::AtomicU64;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::runtime::Handle as TokioHandle;
use tokio::sync::mpsc::{Sender as MpscSender, UnboundedSender};
/// JavaScript value and error types for host bindings.
mod values;
pub use values::{JSError, JSValue, LogLevel};
/// Logger trait and implementations for host functions.
mod logger;
pub use logger::HostLogger;
/// Network fetch functionality for HTTP and file:// URLs.
mod net;
use net::FetchRegistry;

/// Document namespace builder with DOM manipulation functions.
pub mod document;
/// Helper functions for document namespace operations.
mod document_helpers;
/// DOM helper functions for HTML serialization and attribute indexing.
mod dom;
/// Storage registry for localStorage and sessionStorage.
mod storage;
/// Utility functions for HTTP headers and body encoding.
mod util;
pub use document::build_document_namespace;
pub use storage::StorageRegistry;

/// Shorthand for the created nodes registry to keep field types simple.
type CreatedNodeMap = HashMap<NodeKey, CreatedNodeInfo>;

// DOM helpers moved to dom.rs

/// Execution context passed to host callbacks (for example, for logging).
#[derive(Clone)]
pub struct HostContext {
    /// Optional page identifier for context (reserved for future use).
    pub page_id: Option<u64>,
    /// Logger used by host functions such as `console.*`.
    pub logger: Arc<dyn HostLogger>,
    /// Channel for posting DOM updates from host functions (document namespace).
    pub dom_sender: MpscSender<Vec<DOMUpdate>>,
    /// `NodeKey` manager for minting stable keys for JS-created nodes (shared via `Arc`+`Mutex`).
    pub js_node_keys: Arc<Mutex<NodeKeyManager<u64>>>,
    /// Monotonic local id counter used with `js_node_keys.key_of(local_id)`.
    pub js_local_id_counter: Arc<AtomicU64>,
    /// Map of JS-created nodes to their kind and metadata to support appendChild.
    pub js_created_nodes: Arc<Mutex<CreatedNodeMap>>,
    /// Shared DOM index for element lookup functions (e.g., getElementById).
    pub dom_index: Arc<Mutex<DomIndexState>>,
    /// Tokio runtime handle for spawning async network tasks.
    pub tokio_handle: TokioHandle,
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
    /// Optional command channel exposed only to the privileged `<valor://chrome>` origin for controlling the host app.
    pub chrome_host_tx: Option<UnboundedSender<ChromeHostCommand>>,
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
    #[inline]
    pub const fn new() -> Self {
        Self {
            functions: BTreeMap::new(),
            properties: BTreeMap::new(),
        }
    }

    /// Register a synchronous function.
    #[inline]
    #[must_use]
    pub fn with_sync_fn(mut self, name: &str, function: Arc<HostFnSync>) -> Self {
        self.functions
            .insert(name.to_owned(), HostFnKind::Sync(function));
        self
    }

    /// Register a constant property.
    #[inline]
    #[must_use]
    pub fn with_property(mut self, name: &str, value: JSValue) -> Self {
        self.properties.insert(name.to_owned(), value);
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
#[allow(
    dead_code,
    reason = "Reserved for future use when console namespace needs dynamic log level functions"
)]
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
            let level_copy = *level;
            let console_fn = Arc::new(
                move |context: &HostContext, arguments: Vec<JSValue>| -> Result<JSValue, JSError> {
                    let message = stringify_arguments(arguments);
                    context.logger.log(level_copy, &message);
                    Ok(JSValue::Undefined)
                },
            );
            namespace.with_sync_fn(name, console_fn)
        })
}

/// Build the default set of host bindings to install into a JS engine.
/// Currently includes:
/// - `console` namespace with logging methods.
/// - `document` namespace with DOM manipulation.
/// - `performance` namespace with timing.
pub fn build_default_bindings() -> HostBindings {
    HostBindings::new()
        .with_namespace("console", build_console_namespace())
        .with_namespace("document", build_document_namespace())
        .with_namespace("performance", build_performance_namespace())
}

/// Build the `chromeHost` namespace. Functions are gated to `<valor://chrome>` origin
/// and require an attached host command channel in `HostContext`.
#[allow(
    clippy::too_many_lines,
    reason = "Chrome host namespace requires many navigation and tab management functions"
)]
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
pub fn build_chrome_host_bindings() -> HostBindings {
    HostBindings::new().with_namespace("chromeHost", build_chrome_host_namespace())
}

/// Convert a vector of `JSValue` to a space-separated string.
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
pub fn build_performance_namespace() -> HostNamespace {
    let now_fn = Arc::new(
        move |context: &HostContext, _args: Vec<JSValue>| -> Result<JSValue, JSError> {
            let elapsed = Instant::now().duration_since(context.performance_start);
            let milliseconds = elapsed.as_secs_f64() * 1_000.0f64;
            Ok(JSValue::Number(milliseconds))
        },
    );
    HostNamespace::new()
        .with_sync_fn("now", now_fn)
        .with_property("timeOrigin", JSValue::Number(0.0f64))
}
