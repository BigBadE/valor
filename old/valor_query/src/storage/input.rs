//! Storage for input queries (external data sources).
//!
//! Input queries represent data from outside the query system (DOM, etc.).
//! When an input changes, all dependent queries are automatically invalidated.

use crate::query::InputQuery;
use dashmap::DashMap;
use std::any::{Any, TypeId};
use std::hash::Hash;
use std::sync::Arc;

/// Type-erased storage for input values.
trait InputStorageMap: Send + Sync + 'static {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn clear(&self);
    fn len(&self) -> usize;
}

/// Strongly-typed storage for a specific input query type.
#[derive(Debug)]
pub struct TypedInputStorage<I: InputQuery> {
    values: DashMap<u64, Arc<I::Value>>,
}

impl<I: InputQuery> Default for TypedInputStorage<I> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I: InputQuery> TypedInputStorage<I> {
    #[inline]
    pub fn new() -> Self {
        Self {
            values: DashMap::new(),
        }
    }

    #[inline]
    pub fn get(&self, key: &I::Key) -> Arc<I::Value> {
        let hash = Self::hash_key(key);
        self.values
            .get(&hash)
            .map(|entry| entry.value().clone())
            .unwrap_or_else(|| Arc::new(I::default_value()))
    }

    #[inline]
    pub fn set(&self, key: &I::Key, value: I::Value) {
        let hash = Self::hash_key(key);
        self.values.insert(hash, Arc::new(value));
    }

    #[inline]
    pub fn remove(&self, key: &I::Key) -> Option<Arc<I::Value>> {
        let hash = Self::hash_key(key);
        self.values.remove(&hash).map(|(_, val)| val)
    }

    #[inline]
    fn hash_key(key: &I::Key) -> u64 {
        use std::hash::Hasher;
        let mut hasher = rustc_hash::FxHasher::default();
        key.hash(&mut hasher);
        hasher.finish()
    }
}

impl<I: InputQuery> InputStorageMap for TypedInputStorage<I> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn clear(&self) {
        self.values.clear();
    }

    fn len(&self) -> usize {
        self.values.len()
    }
}

/// Central storage for all input queries.
#[derive(Default)]
pub struct InputStorage {
    storages: DashMap<TypeId, Box<dyn InputStorageMap>>,
}

impl InputStorage {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the value for an input query.
    #[inline]
    pub fn get<I: InputQuery>(&self, key: I::Key) -> Arc<I::Value> {
        let type_id = TypeId::of::<I>();

        if let Some(storage) = self.storages.get(&type_id) {
            let typed = storage
                .value()
                .as_any()
                .downcast_ref::<TypedInputStorage<I>>()
                .expect("Type mismatch in input storage");
            typed.get(&key)
        } else {
            Arc::new(I::default_value())
        }
    }

    /// Set the value for an input query.
    ///
    /// Returns the previous revision to help detect changes.
    #[inline]
    pub fn set<I: InputQuery>(&self, key: I::Key, value: I::Value) -> Option<Arc<I::Value>> {
        let type_id = TypeId::of::<I>();

        let storage = self
            .storages
            .entry(type_id)
            .or_insert_with(|| Box::new(TypedInputStorage::<I>::new()));

        let typed = storage
            .value()
            .as_any()
            .downcast_ref::<TypedInputStorage<I>>()
            .expect("Type mismatch in input storage");

        let old = typed
            .values
            .get(&TypedInputStorage::<I>::hash_key(&key))
            .map(|e| e.value().clone());
        typed.set(&key, value);
        old
    }

    /// Remove an input value.
    #[inline]
    pub fn remove<I: InputQuery>(&self, key: I::Key) -> Option<Arc<I::Value>> {
        let type_id = TypeId::of::<I>();

        self.storages.get(&type_id).and_then(|storage| {
            let typed = storage
                .value()
                .as_any()
                .downcast_ref::<TypedInputStorage<I>>()
                .expect("Type mismatch in input storage");
            typed.remove(&key)
        })
    }

    /// Clear all input values.
    #[inline]
    pub fn clear(&self) {
        for storage in self.storages.iter() {
            storage.value().clear();
        }
    }
}
