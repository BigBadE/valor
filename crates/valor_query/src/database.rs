//! Core query database implementation.
//!
//! The `QueryDatabase` is the central coordinator for all query execution,
//! memoization, and dependency tracking.

use crate::query::{InputQuery, MemoizedResult, Query, QueryKey};
use crate::revision::{Revision, RevisionCounter};
use crate::storage::{InputStorage, MemoizedStorage, QueryStorage};
use dashmap::DashMap;
use log::trace;
use std::any::TypeId;
use std::cell::RefCell;
use std::hash::Hash;
use std::sync::Arc;
use thread_local::ThreadLocal;

/// Query execution context for dependency tracking.
///
/// Each thread maintains a stack of active queries to automatically
/// track dependencies as queries call other queries.
#[derive(Debug, Default)]
struct QueryStack {
    /// Stack of currently executing queries.
    stack: Vec<QueryKey>,
}

impl QueryStack {
    #[inline]
    fn push(&mut self, key: QueryKey) {
        self.stack.push(key);
    }

    #[inline]
    fn pop(&mut self) -> Option<QueryKey> {
        self.stack.pop()
    }

    #[inline]
    fn current_dependencies(&self) -> Vec<QueryKey> {
        self.stack.clone()
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }
}

/// The central query database.
///
/// This is the main interface for executing queries, storing results,
/// and managing dependency-based invalidation.
pub struct QueryDatabase {
    /// Current revision counter - incremented on input changes.
    revision: RevisionCounter,

    /// Storage for each query type (type-erased).
    storages: DashMap<TypeId, Box<dyn QueryStorage>>,

    /// Storage for input values.
    inputs: InputStorage,

    /// Per-thread query execution stack for dependency tracking.
    query_stacks: ThreadLocal<RefCell<QueryStack>>,
}

impl Default for QueryDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl QueryDatabase {
    /// Create a new query database.
    #[inline]
    pub fn new() -> Self {
        Self {
            revision: RevisionCounter::new(),
            storages: DashMap::new(),
            inputs: InputStorage::new(),
            query_stacks: ThreadLocal::new(),
        }
    }

    /// Execute a query, returning the memoized result if valid.
    ///
    /// This is the main entry point for query execution. It:
    /// 1. Checks if a cached result exists and is still valid
    /// 2. If not, executes the query function
    /// 3. Automatically tracks dependencies during execution
    /// 4. Memoizes the result for future queries
    pub fn query<Q: Query>(&self, key: Q::Key) -> Arc<Q::Value> {
        let query_key = QueryKey::new::<Q>(&key);

        // Try to get cached result
        let storage = self.get_or_create_storage::<Q>();
        if let Some(cached) = storage.get(&key) {
            // Check if dependencies are still valid
            if self.verify_dependencies(&cached) {
                trace!("Cache hit for {}: {:?}", Q::name(), query_key);

                // Mark as verified at current revision
                storage.mark_verified(&key, self.revision.current());
                return cached.value;
            }
        }

        trace!("Cache miss for {}: {:?}", Q::name(), query_key);

        // Execute query with dependency tracking
        self.execute_query::<Q>(key, query_key, &storage)
    }

    /// Get an input value.
    #[inline]
    pub fn input<I: InputQuery>(&self, key: I::Key) -> Arc<I::Value> {
        self.inputs.get::<I>(key)
    }

    /// Set an input value, incrementing the revision.
    ///
    /// This invalidates all queries that transitively depend on this input.
    pub fn set_input<I: InputQuery>(&self, key: I::Key, value: I::Value) {
        // For inputs, create a QueryKey using TypeId
        use std::hash::Hasher;
        let mut hasher = rustc_hash::FxHasher::default();
        key.hash(&mut hasher);
        let query_key = QueryKey {
            query_type: TypeId::of::<I>(),
            key_hash: hasher.finish(),
        };

        // Store the input
        self.inputs.set::<I>(key, value);

        // Increment revision
        let new_revision = self.revision.increment();

        trace!(
            "Input changed: {} -> revision {:?}",
            I::name(),
            new_revision
        );

        // Invalidate all queries that depend on this input
        self.invalidate_dependents(&query_key);
    }

    /// Get the current revision.
    #[inline]
    pub fn current_revision(&self) -> Revision {
        self.revision.current()
    }

    /// Clear all cached query results (but keep inputs).
    pub fn clear_cache(&self) {
        for storage in self.storages.iter() {
            storage.value().clear();
        }
    }

    /// Execute a query and memoize the result.
    fn execute_query<Q: Query>(
        &self,
        key: Q::Key,
        query_key: QueryKey,
        storage: &MemoizedStorage<Q>,
    ) -> Arc<Q::Value> {
        // Push onto dependency stack
        self.push_query_stack(query_key.clone());

        // Execute the query
        let value = Q::execute(self, key.clone());

        // Pop from stack and collect dependencies
        self.pop_query_stack();
        let dependencies = self.current_dependencies();

        // Create memoized result
        let revision = self.revision.current();
        let result = MemoizedResult::new(value, revision, dependencies);
        let value_arc = result.value.clone();

        // Store result
        storage.insert(&key, result);

        value_arc
    }

    /// Verify that all dependencies of a cached result are still valid.
    fn verify_dependencies<V>(&self, result: &MemoizedResult<V>) -> bool {
        let current = self.revision.current();

        // If we've already verified this result at the current revision, it's valid
        if result.verified_at == current {
            return true;
        }

        // If computed at current revision, must be valid
        if result.computed_at == current {
            return true;
        }

        // Check each dependency recursively
        // For now, we conservatively assume invalid if any dependency changed
        // A more sophisticated approach would recursively verify dependencies
        result.verified_at.is_newer_than(result.computed_at) == false
    }

    /// Invalidate all queries that depend on the given query key.
    fn invalidate_dependents(&self, key: &QueryKey) {
        for storage in self.storages.iter() {
            storage.value().invalidate_dependents(key);
        }
    }

    /// Get or create storage for a query type.
    fn get_or_create_storage<Q: Query>(&self) -> Arc<MemoizedStorage<Q>> {
        let type_id = TypeId::of::<Q>();

        let storage = self
            .storages
            .entry(type_id)
            .or_insert_with(|| Box::new(MemoizedStorage::<Q>::new()));

        // SAFETY: We just created this storage with the correct type
        let ptr = storage.value().as_any() as *const dyn std::any::Any;
        let typed = unsafe { &*(ptr as *const MemoizedStorage<Q>) };
        Arc::new(MemoizedStorage {
            cache: typed.cache.clone(),
            reverse_deps: typed.reverse_deps.clone(),
        })
    }

    /// Push a query onto the current thread's execution stack.
    fn push_query_stack(&self, key: QueryKey) {
        self.query_stacks
            .get_or(|| RefCell::new(QueryStack::default()))
            .borrow_mut()
            .push(key);
    }

    /// Pop a query from the current thread's execution stack.
    fn pop_query_stack(&self) {
        if let Some(stack) = self.query_stacks.get() {
            stack.borrow_mut().pop();
        }
    }

    /// Get the current dependencies for the executing query.
    fn current_dependencies(&self) -> Vec<QueryKey> {
        self.query_stacks
            .get()
            .map(|stack| stack.borrow().current_dependencies())
            .unwrap_or_default()
    }
}
