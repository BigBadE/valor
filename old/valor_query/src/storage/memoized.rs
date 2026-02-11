//! Memoized storage for query results.
//!
//! This module provides thread-safe storage for computed query results
//! with support for dependency tracking and cache invalidation.

use crate::query::{MemoizedResult, Query, QueryKey};
use crate::revision::Revision;
use dashmap::DashMap;
use std::any::Any;
use std::hash::Hash;

/// Type-erased trait for query storage.
///
/// This allows the `QueryDatabase` to store different query types
/// in a single collection.
pub trait QueryStorage: Send + Sync + 'static {
    /// Get the storage as a dynamic reference.
    fn as_any(&self) -> &dyn Any;

    /// Get the storage as a mutable dynamic reference.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Invalidate all entries that depend on the given query key.
    fn invalidate_dependents(&self, key: &QueryKey);

    /// Clear all cached results.
    fn clear(&self);

    /// Get the number of cached entries.
    fn len(&self) -> usize;

    /// Check if storage is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Strongly-typed storage for a specific query type.
#[derive(Debug)]
pub struct MemoizedStorage<Q: Query> {
    /// Cached query results, keyed by the query's key hash.
    pub(crate) cache: DashMap<u64, MemoizedResult<Q::Value>>,

    /// Reverse dependency index: query key -> dependents that use it.
    /// When a query is invalidated, we use this to find what else to invalidate.
    pub(crate) reverse_deps: DashMap<QueryKey, Vec<u64>>,
}

impl<Q: Query> Default for MemoizedStorage<Q> {
    fn default() -> Self {
        Self::new()
    }
}

impl<Q: Query> MemoizedStorage<Q> {
    /// Create a new empty storage.
    #[inline]
    pub fn new() -> Self {
        Self {
            cache: DashMap::new(),
            reverse_deps: DashMap::new(),
        }
    }

    /// Get a cached result if it exists.
    #[inline]
    pub fn get(&self, key: &Q::Key) -> Option<MemoizedResult<Q::Value>> {
        let hash = Self::hash_key(key);
        self.cache.get(&hash).map(|entry| entry.value().clone())
    }

    /// Store a computed result.
    #[inline]
    pub fn insert(&self, key: &Q::Key, result: MemoizedResult<Q::Value>) {
        let hash = Self::hash_key(key);

        // Register reverse dependencies
        for dep in &result.dependencies {
            self.reverse_deps.entry(dep.clone()).or_default().push(hash);
        }

        self.cache.insert(hash, result);
    }

    /// Remove a cached result.
    #[inline]
    pub fn remove(&self, key: &Q::Key) -> Option<MemoizedResult<Q::Value>> {
        let hash = Self::hash_key(key);
        self.cache.remove(&hash).map(|(_, val)| val)
    }

    /// Mark a result as verified at the current revision.
    ///
    /// This is used when we've checked that a result's dependencies
    /// haven't changed, so the cached value is still valid.
    #[inline]
    pub fn mark_verified(&self, key: &Q::Key, revision: Revision) {
        let hash = Self::hash_key(key);
        if let Some(mut entry) = self.cache.get_mut(&hash) {
            entry.value_mut().verified_at = revision;
        }
    }

    /// Invalidate a specific entry.
    #[inline]
    pub fn invalidate(&self, key: &Q::Key) {
        let hash = Self::hash_key(key);
        self.cache.remove(&hash);
    }

    /// Hash a key for storage lookup.
    #[inline]
    fn hash_key(key: &Q::Key) -> u64 {
        use std::hash::Hasher;
        let mut hasher = rustc_hash::FxHasher::default();
        key.hash(&mut hasher);
        hasher.finish()
    }
}

impl<Q: Query> QueryStorage for MemoizedStorage<Q> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn invalidate_dependents(&self, key: &QueryKey) {
        if let Some((_, dependents)) = self.reverse_deps.remove(key) {
            for hash in dependents {
                self.cache.remove(&hash);
            }
        }
    }

    fn clear(&self) {
        self.cache.clear();
        self.reverse_deps.clear();
    }

    fn len(&self) -> usize {
        self.cache.len()
    }
}
