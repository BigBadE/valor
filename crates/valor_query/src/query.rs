//! Query trait and related types.
//!
//! Queries are the fundamental unit of computation in the query system.
//! Each query is a pure function from a key to a value, with automatic
//! memoization and dependency tracking.

use crate::database::QueryDatabase;
use crate::revision::Revision;
use std::any::TypeId;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Unique identifier for a query invocation.
///
/// Combines the query type with a hash of the key to enable
/// efficient storage and lookup.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct QueryKey {
    /// Type ID of the query
    pub query_type: TypeId,
    /// Hash of the query's input key
    pub key_hash: u64,
}

impl QueryKey {
    /// Create a new query key.
    #[inline]
    pub fn new<Q: Query>(key: &Q::Key) -> Self {
        let mut hasher = rustc_hash::FxHasher::default();
        key.hash(&mut hasher);
        Self {
            query_type: TypeId::of::<Q>(),
            key_hash: hasher.finish(),
        }
    }
}

/// A memoized query result with dependency tracking.
#[derive(Clone, Debug)]
pub struct MemoizedResult<V> {
    /// The computed value
    pub value: Arc<V>,
    /// Revision when this result was computed
    pub computed_at: Revision,
    /// Revision when this result was last verified as still valid
    pub verified_at: Revision,
    /// Query keys this result depends on
    pub dependencies: Vec<QueryKey>,
}

impl<V> MemoizedResult<V> {
    /// Create a new memoized result.
    #[inline]
    pub fn new(value: V, revision: Revision, dependencies: Vec<QueryKey>) -> Self {
        Self {
            value: Arc::new(value),
            computed_at: revision,
            verified_at: revision,
            dependencies,
        }
    }
}

/// Trait for all queries in the system.
///
/// A query is a pure function from `Key` to `Value` that:
/// - Is automatically memoized
/// - Tracks its dependencies on other queries
/// - Is invalidated when dependencies change
///
/// # Example
///
/// ```ignore
/// struct ComputedStyleQuery;
///
/// impl Query for ComputedStyleQuery {
///     type Key = NodeKey;
///     type Value = ComputedStyle;
///
///     fn execute(db: &QueryDatabase, key: Self::Key) -> Self::Value {
///         let inherited = db.query::<InheritedStyleQuery>(key);
///         let matched = db.query::<MatchingRulesQuery>(key);
///         cascade(inherited, matched)
///     }
/// }
/// ```
pub trait Query: 'static + Sized {
    /// The input key type for this query.
    type Key: Clone + Hash + Eq + Send + Sync + 'static;

    /// The output value type for this query.
    type Value: Clone + Send + Sync + 'static;

    /// Execute the query to compute its value.
    ///
    /// This is called when:
    /// - The query has never been computed
    /// - The cached result is stale (dependencies changed)
    ///
    /// The implementation should use `db.query::<OtherQuery>(key)` to
    /// read from other queries, which automatically tracks dependencies.
    fn execute(db: &QueryDatabase, key: Self::Key) -> Self::Value;

    /// Optional: provide a name for debugging/profiling.
    fn name() -> &'static str {
        std::any::type_name::<Self>()
    }
}

/// Trait for input queries (external data sources).
///
/// Input queries are the leaves of the dependency graph - they
/// represent data that comes from outside the query system (DOM, etc.).
/// When an input changes, the revision is incremented and all
/// dependent queries are invalidated.
pub trait InputQuery: 'static + Sized {
    /// The input key type.
    type Key: Clone + Hash + Eq + Send + Sync + 'static;

    /// The input value type.
    type Value: Clone + Send + Sync + 'static;

    /// Default value when no input has been set.
    fn default_value() -> Self::Value;

    /// Optional: provide a name for debugging/profiling.
    fn name() -> &'static str {
        std::any::type_name::<Self>()
    }
}

/// Marker trait for queries that can be executed in parallel.
///
/// A query is parallelizable if it:
/// - Has no side effects
/// - Uses only thread-safe operations
/// - Doesn't require sequential ordering with other queries
///
/// Most queries should implement this trait.
pub trait ParallelQuery: Query {}

/// Extension trait for convenient query execution.
pub trait QueryExt: Query {
    /// Execute this query on the given database.
    #[inline]
    fn get(db: &QueryDatabase, key: Self::Key) -> Arc<Self::Value> {
        db.query::<Self>(key)
    }
}

impl<Q: Query> QueryExt for Q {}
