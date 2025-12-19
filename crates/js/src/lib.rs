//! Engine-agnostic JavaScript facade and DOM mirroring primitives.
//! This crate centralizes interfaces and types that are shared across JS engines
//! and Valor subsystems (HTML parser, layouter, renderer, etc.).

use anyhow::{Error as AnyhowError, Result};
use core::hash::Hash;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, mpsc};

pub mod console;
pub use console::{Console, ConsoleLogger};

/// Engine-agnostic host bindings facade: values, logger, and namespace builders.
pub mod bindings;
pub use bindings::{
    build_chrome_host_bindings, build_chrome_host_namespace, build_console_namespace,
    build_default_bindings, build_document_namespace, stringify_arguments, ChromeHostCommand,
    CreatedNodeInfo, CreatedNodeKind, HostBindings, HostContext, HostFnKind, HostFnSync,
    HostLogger, HostNamespace, JSError, JSValue, LogLevel,
};

/// DOM index mirror for element lookups from host-side APIs.
pub mod dom_index;
pub use dom_index::{DomIndex, DomIndexState, SharedDomIndex};

pub mod modules;
/// JavaScript prelude script for bootstrapping runtime behavior in the engine.
pub mod runtime;
pub use modules::{ModuleResolver, SimpleFileModuleResolver};

// ============================
// Engine-agnostic JS context trait
// ============================

/// A minimal interface for evaluating JavaScript in a per-page engine.
/// Keep this trait small so engines can be swapped (e.g., QuickJS/V8).
pub trait JsEngine {
    /// Evaluate a classic script.
    ///
    /// # Errors
    /// Returns an error if script evaluation fails.
    fn eval_script(&mut self, source: &str, url: &str) -> Result<()>;
    /// Evaluate an ES module's executable form. For now, engines may accept
    /// pre-bundled side-effect-only module code produced by the host.
    ///
    /// # Errors
    /// Returns an error if module evaluation fails.
    fn eval_module(&mut self, source: &str, url: &str) -> Result<()>;
    /// Run pending microtasks/jobs until idle.
    ///
    /// # Errors
    /// Returns an error if job execution fails.
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
    pub const ROOT: Self = Self(0);
    /// Pack epoch+shard+counter into a single 64-bit key.
    #[inline]
    pub fn pack(epoch: u16, shard: u8, counter: u64) -> Self {
        let counter_masked = counter & ((1u64 << 40i32) - 1);
        Self((u64::from(epoch) << 48) | (u64::from(shard) << 40) | counter_masked)
    }
    /// Extract epoch from the key.
    #[inline]
    pub const fn epoch(self) -> u16 {
        (self.0 >> 48) as u16
    }
    /// Extract shard from the key.
    #[inline]
    pub const fn shard(self) -> u8 {
        ((self.0 >> 40) & 0xFF) as u8
    }
    /// Extract counter from the key.
    #[inline]
    pub const fn counter(self) -> u64 {
        self.0 & ((1u64 << 40i32) - 1)
    }
}

/// Global key space for minting `NodeKey`s with unique epochs and shard IDs.
#[derive(Debug)]
pub struct KeySpace {
    /// Current epoch for this key space.
    epoch: u16,
    /// Next shard ID to allocate.
    next_shard_id: u8,
}

impl KeySpace {
    /// Create a new key space with a time-derived epoch.
    #[inline]
    pub fn new() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let epoch = (((now.as_secs() as u32) ^ now.subsec_nanos()) & 0xFFFF) as u16;
        Self {
            epoch,
            next_shard_id: 1,
        }
    }
    /// Register a new manager for a given producer shard.
    #[inline]
    pub fn register_manager<L: Eq + Hash + Copy>(&mut self) -> NodeKeyManager<L> {
        let shard = self.next_shard_id;
        self.next_shard_id = self.next_shard_id.wrapping_add(1);
        NodeKeyManager::new(self.epoch, shard)
    }
    /// Return the current epoch.
    #[inline]
    pub const fn epoch(&self) -> u16 {
        self.epoch
    }
}

impl Default for KeySpace {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

/// Per-shard manager mapping local IDs to `NodeKey`s and minting new keys.
#[derive(Clone, Debug)]
pub struct NodeKeyManager<L: Eq + Hash + Copy> {
    /// Epoch for keys minted by this manager.
    epoch: u16,
    /// Shard ID for keys minted by this manager.
    shard: u8,
    /// Counter for minting unique keys.
    counter: u64,
    /// Map from local IDs to `NodeKey`s.
    map: HashMap<L, NodeKey>,
}

impl<L: Eq + Hash + Copy> NodeKeyManager<L> {
    /// Create a new manager for the given epoch and shard.
    fn new(epoch: u16, shard: u8) -> Self {
        Self {
            epoch,
            shard,
            counter: 1,
            map: HashMap::new(),
        }
    }
    /// Get the `NodeKey` for a local ID, minting if not present.
    #[inline]
    pub fn key_of(&mut self, local_id: L) -> NodeKey {
        if let Some(&key) = self.map.get(&local_id) {
            return key;
        }
        let key = NodeKey::pack(self.epoch, self.shard, self.counter);
        self.counter = self.counter.wrapping_add(1);
        self.map.insert(local_id, key);
        key
    }
    /// Seed a mapping from a local ID to an existing `NodeKey`.
    #[inline]
    pub fn seed(&mut self, local_id: L, key: NodeKey) {
        self.map.insert(local_id, key);
    }
}

// ============================
// DOM Update model + mirror pattern
// ============================

/// A batchable update applied to the runtime DOM and mirrored to subscribers.
#[derive(Debug, Clone)]
pub enum DOMUpdate {
    InsertElement {
        parent: NodeKey,
        node: NodeKey,
        tag: String,
        pos: usize,
    },
    InsertText {
        parent: NodeKey,
        node: NodeKey,
        text: String,
        pos: usize,
    },
    SetAttr {
        node: NodeKey,
        name: String,
        value: String,
    },
    RemoveNode {
        node: NodeKey,
    },
    UpdateText {
        node: NodeKey,
        text: String,
    },
    EndOfDocument,
}

/// A subscriber that receives `DOMUpdate` values and mirrors them into its own state.
pub trait DOMSubscriber {
    /// Apply a single `DOMUpdate` to the subscriber state.
    ///
    /// # Errors
    /// Returns an error if the update cannot be applied.
    fn apply_update(&mut self, update: DOMUpdate) -> Result<()>;
}

/// Generic mirror that can apply incoming DOM updates and send changes back to the DOM runtime.
pub struct DOMMirror<T: DOMSubscriber> {
    /// Receiver for incoming DOM updates.
    in_updater: broadcast::Receiver<Vec<DOMUpdate>>,
    /// Sender for outgoing DOM updates.
    out_updater: mpsc::Sender<Vec<DOMUpdate>>,
    /// The subscriber implementation.
    mirror: T,
}

impl<T: DOMSubscriber> DOMMirror<T> {
    /// Create a new `DOMMirror` wrapping a subscriber implementation.
    #[inline]
    pub const fn new(
        out_updater: mpsc::Sender<Vec<DOMUpdate>>,
        in_updater: broadcast::Receiver<Vec<DOMUpdate>>,
        mirror: T,
    ) -> Self {
        Self {
            in_updater,
            out_updater,
            mirror,
        }
    }
    /// Drain and apply all pending `DOMUpdate` batches.
    ///
    /// # Errors
    /// Returns an error if the channel is closed or updates cannot be applied.
    #[inline]
    pub fn update(&mut self) -> Result<()> {
        use tokio::sync::broadcast::error::TryRecvError;
        while let Some(updates) = match self.in_updater.try_recv() {
            Ok(updates) => Ok::<_, AnyhowError>(Some(updates)),
            Err(TryRecvError::Closed) => {
                return Err(AnyhowError::msg(
                    "Recv channel was closed before document ended!",
                ));
            }
            _ => Ok(None),
        }? {
            for update in updates {
                self.mirror.apply_update(update)?;
            }
        }
        Ok(())
    }
    /// Synchronous, non-async variant for draining pending updates (for blocking threads)
    ///
    /// # Errors
    /// Returns an error if the channel is closed or updates cannot be applied.
    #[inline]
    pub fn try_update_sync(&mut self) -> Result<()> {
        use tokio::sync::broadcast::error::TryRecvError;
        loop {
            match self.in_updater.try_recv() {
                Ok(batch) => {
                    for update in batch {
                        self.mirror.apply_update(update)?;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Lagged(_)) => {}
                Err(TryRecvError::Closed) => {
                    // Channel closed - this is normal during shutdown, not an error
                    break;
                }
            }
        }
        Ok(())
    }
    /// Access the inner mirror mutably (engine-level integration)
    #[inline]
    pub const fn mirror_mut(&mut self) -> &mut T {
        &mut self.mirror
    }
    /// Access the inner mirror immutably (read-only access)
    #[inline]
    pub const fn mirror(&self) -> &T {
        &self.mirror
    }
    /// Consume the `DOMMirror` and return the inner subscriber.
    #[inline]
    #[must_use]
    pub fn into_inner(self) -> T {
        self.mirror
    }
    /// Send a batch of DOM changes back to the DOM runtime.
    ///
    /// # Errors
    /// Returns an error if the channel is closed.
    #[inline]
    pub async fn send_dom_change(&mut self, changes: Vec<DOMUpdate>) -> Result<()> {
        // Ignore send errors - receiver might be dropped during shutdown
        drop(self.out_updater.send(changes).await);
        Ok(())
    }
}
