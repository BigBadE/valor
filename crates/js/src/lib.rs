//! Engine-agnostic JavaScript facade and DOM mirroring primitives.
//! This crate centralizes interfaces and types that are shared across JS engines
//! and Valor subsystems (HTML parser, layouter, renderer, etc.).

use anyhow::Result;
use tokio::sync::{broadcast, mpsc};
use std::collections::HashMap;
use std::hash::Hash;
use std::time::{SystemTime, UNIX_EPOCH};

pub mod console;
pub use console::{Console, ConsoleLogger};


/// Engine-agnostic host bindings facade: values, logger, and namespace builders.
pub mod bindings;
pub use bindings::{JSValue, JSError, HostBindings, HostNamespace, HostFnKind, HostFnSync, HostContext, HostLogger, LogLevel, CreatedNodeKind, CreatedNodeInfo, ChromeHostCommand, build_console_namespace, build_document_namespace, build_default_bindings, build_chrome_host_namespace, build_chrome_host_bindings, stringify_arguments};

/// DOM index mirror for element lookups from host-side APIs.
pub mod dom_index;
pub use dom_index::{DomIndex, DomIndexState, SharedDomIndex};

/// JavaScript prelude script for bootstrapping runtime behavior in the engine.
pub mod runtime;
pub mod modules;
pub use modules::{ModuleResolver, SimpleFileModuleResolver};

// ============================
// Engine-agnostic JS context trait
// ============================

/// A minimal interface for evaluating JavaScript in a per-page engine.
/// Keep this trait small so engines can be swapped (e.g., QuickJS/V8).
pub trait JsEngine {
    /// Evaluate a classic script.
    fn eval_script(&mut self, source: &str, url: &str) -> Result<()>;
    /// Evaluate an ES module's executable form. For now, engines may accept
    /// pre-bundled side-effect-only module code produced by the host.
    fn eval_module(&mut self, source: &str, url: &str) -> Result<()>;
    /// Run pending microtasks/jobs until idle.
    fn run_jobs(&mut self) -> Result<()>;
}

// ============================
// Stable Node keys (shared across subsystems)
// ============================

/// A 64-bit stable key for DOM nodes used to correlate asynchronous updates.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct NodeKey(pub u64);

impl NodeKey {
    /// The root node key (always present).
    pub const ROOT: NodeKey = NodeKey(0);
    /// Pack epoch+shard+counter into a single 64-bit key.
    #[inline]
    pub fn pack(epoch: u16, shard: u8, counter: u64) -> Self {
        let c = counter & ((1u64 << 40) - 1);
        NodeKey(((epoch as u64) << 48) | ((shard as u64) << 40) | c)
    }
    /// Extract epoch from the key.
    #[inline]
    pub fn epoch(self) -> u16 { (self.0 >> 48) as u16 }
    /// Extract shard from the key.
    #[inline]
    pub fn shard(self) -> u8 { ((self.0 >> 40) & 0xFF) as u8 }
    /// Extract counter from the key.
    #[inline]
    pub fn counter(self) -> u64 { self.0 & ((1u64 << 40) - 1) }
}

/// Global key space for minting NodeKeys with unique epochs and shard IDs.
#[derive(Debug)]
pub struct KeySpace {
    epoch: u16,
    next_shard_id: u8,
}

impl KeySpace {
    /// Create a new key space with a time-derived epoch.
    pub fn new() -> Self {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        let epoch = (((now.as_secs() as u32) ^ now.subsec_nanos()) & 0xFFFF) as u16;
        Self { epoch, next_shard_id: 1 }
    }
    /// Register a new manager for a given producer shard.
    pub fn register_manager<L: Eq + Hash + Copy>(&mut self) -> NodeKeyManager<L> {
        let shard = self.next_shard_id;
        self.next_shard_id = self.next_shard_id.wrapping_add(1);
        NodeKeyManager::new(self.epoch, shard)
    }
    /// Return the current epoch.
    pub fn epoch(&self) -> u16 { self.epoch }
}

impl Default for KeySpace {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-shard manager mapping local IDs to NodeKeys and minting new keys.
#[derive(Clone, Debug)]
pub struct NodeKeyManager<L: Eq + Hash + Copy> {
    epoch: u16,
    shard: u8,
    counter: u64,
    map: HashMap<L, NodeKey>,
}

impl<L: Eq + Hash + Copy> NodeKeyManager<L> {
    fn new(epoch: u16, shard: u8) -> Self { Self { epoch, shard, counter: 1, map: HashMap::new() } }
    /// Get the NodeKey for a local ID, minting if not present.
    #[inline]
    pub fn key_of(&mut self, id: L) -> NodeKey {
        if let Some(&k) = self.map.get(&id) { return k; }
        let key = NodeKey::pack(self.epoch, self.shard, self.counter);
        self.counter = self.counter.wrapping_add(1);
        self.map.insert(id, key);
        key
    }
    /// Seed a mapping from a local ID to an existing NodeKey.
    #[inline]
    pub fn seed(&mut self, id: L, key: NodeKey) { self.map.insert(id, key); }
}

// ============================
// DOM Update model + mirror pattern
// ============================

/// A batchable update applied to the runtime DOM and mirrored to subscribers.
#[derive(Debug, Clone)]
pub enum DOMUpdate {
    InsertElement { parent: NodeKey, node: NodeKey, tag: String, pos: usize },
    InsertText { parent: NodeKey, node: NodeKey, text: String, pos: usize },
    SetAttr { node: NodeKey, name: String, value: String },
    RemoveNode { node: NodeKey },
    EndOfDocument,
}

/// A subscriber that receives DOMUpdate values and mirrors them into its own state.
pub trait DOMSubscriber {
    /// Apply a single DOMUpdate to the subscriber state.
    fn apply_update(&mut self, update: DOMUpdate) -> anyhow::Result<()>;
}

/// Generic mirror that can apply incoming DOM updates and send changes back to the DOM runtime.
pub struct DOMMirror<T: DOMSubscriber> {
    in_updater: broadcast::Receiver<Vec<DOMUpdate>>,
    out_updater: mpsc::Sender<Vec<DOMUpdate>>,
    mirror: T,
}

impl<T: DOMSubscriber> DOMMirror<T> {
    /// Create a new DOMMirror wrapping a subscriber implementation.
    pub fn new(out_updater: mpsc::Sender<Vec<DOMUpdate>>, in_updater: broadcast::Receiver<Vec<DOMUpdate>>, mirror: T) -> Self { Self { in_updater, out_updater, mirror } }
    /// Drain and apply all pending DOMUpdate batches asynchronously.
    pub async fn update(&mut self) -> anyhow::Result<()> {
        use tokio::sync::broadcast::error::TryRecvError;
        while let Some(updates) = match self.in_updater.try_recv() {
            Ok(updates) => Ok::<_, anyhow::Error>(Some(updates)),
            Err(TryRecvError::Closed) => { return Err(anyhow::anyhow!("Recv channel was closed before document ended!")); }
            _ => Ok(None),
        }? {
            for update in updates { self.mirror.apply_update(update)?; }
        }
        Ok(())
    }
    /// Synchronous, non-async variant for draining pending updates (for blocking threads)
    pub fn try_update_sync(&mut self) -> anyhow::Result<()> {
        use tokio::sync::broadcast::error::TryRecvError;
        loop {
            match self.in_updater.try_recv() {
                Ok(batch) => { for update in batch { self.mirror.apply_update(update)?; } }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Lagged(_)) => continue,
                Err(TryRecvError::Closed) => { return Err(anyhow::anyhow!("Recv channel was closed before document ended!")); }
            }
        }
        Ok(())
    }
    /// Access the inner mirror mutably (engine-level integration)
    pub fn mirror_mut(&mut self) -> &mut T { &mut self.mirror }
    /// Access the inner mirror immutably (read-only access)
    pub fn mirror(&self) -> &T { &self.mirror }
    /// Send a batch of DOM changes back to the DOM runtime.
    pub async fn send_dom_change(&mut self, changes: Vec<DOMUpdate>) -> anyhow::Result<()> { self.out_updater.send(changes).await?; Ok(()) }
}
