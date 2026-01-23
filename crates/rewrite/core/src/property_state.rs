use std::thread::ThreadId;

use crate::dependency::DependencyPattern;

/// State of a property in the database.
#[derive(Clone)]
pub enum PropertyState {
    /// Property has not been computed yet.
    Unevaluated,

    /// Property is currently being computed by a thread.
    Computing(ThreadId),

    /// Property has been computed and cached.
    Evaluated {
        value: std::sync::Arc<dyn std::any::Any + Send + Sync>,
        dependencies: std::sync::Arc<DependencyPattern>,
    },
}

impl PropertyState {
    pub fn is_evaluated(&self) -> bool {
        matches!(self, PropertyState::Evaluated { .. })
    }

    pub fn is_computing(&self) -> bool {
        matches!(self, PropertyState::Computing(_))
    }

    pub fn is_unevaluated(&self) -> bool {
        matches!(self, PropertyState::Unevaluated)
    }
}

/// Key identifying a specific property (type-erased for any key type).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct PropertyKey {
    pub query_type: std::any::TypeId,
    pub key_hash: u64,
}

impl PropertyKey {
    pub fn new<Q: crate::Query>(key: &Q::Key) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        std::any::TypeId::of::<Q>().hash(&mut hasher);
        key.hash(&mut hasher);

        Self {
            query_type: std::any::TypeId::of::<Q>(),
            key_hash: hasher.finish(),
        }
    }
}

impl std::fmt::Debug for PropertyKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PropertyKey")
            .field("query_type", &self.query_type)
            .field("key_hash", &self.key_hash)
            .finish()
    }
}
