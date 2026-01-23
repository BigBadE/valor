use std::any::TypeId;

/// A dependency on a specific query with a specific key (type-erased).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Dependency {
    pub query_type: TypeId,
    pub key_hash: u64,
}

impl Dependency {
    pub fn new<Q: crate::Query>(key: Q::Key) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        TypeId::of::<Q>().hash(&mut hasher);
        key.hash(&mut hasher);

        Self {
            query_type: TypeId::of::<Q>(),
            key_hash: hasher.finish(),
        }
    }
}

impl std::fmt::Debug for Dependency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dependency")
            .field("query_type", &self.query_type)
            .field("key_hash", &self.key_hash)
            .finish()
    }
}

/// A set of dependencies, interned for memory efficiency.
/// Multiple properties with the same dependency pattern share the same Arc.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DependencyPattern {
    deps: Vec<Dependency>,
}

impl DependencyPattern {
    pub fn new(deps: Vec<Dependency>) -> Self {
        Self { deps }
    }

    pub fn dependencies(&self) -> &[Dependency] {
        &self.deps
    }

    pub fn is_empty(&self) -> bool {
        self.deps.is_empty()
    }
}

/// Thread-local context for tracking dependencies during query execution.
#[derive(Default)]
pub struct DependencyContext {
    /// Dependencies recorded during current query execution.
    pub(crate) current_deps: Vec<Dependency>,
    /// Stack of query executions (for cycle detection).
    pub(crate) execution_stack: Vec<Dependency>,
}

impl DependencyContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start tracking dependencies for a query execution.
    pub fn begin_query(&mut self, dep: Dependency) -> Result<(), CycleError> {
        // Check for cycles
        if self.execution_stack.contains(&dep) {
            return Err(CycleError {
                cycle: self.execution_stack.clone(),
            });
        }

        self.execution_stack.push(dep);
        Ok(())
    }

    /// Finish tracking dependencies for a query execution.
    pub fn end_query(&mut self) -> Vec<Dependency> {
        self.execution_stack.pop();
        std::mem::take(&mut self.current_deps)
    }

    /// Record a dependency during query execution.
    pub fn record_dependency(&mut self, dep: Dependency) {
        if !self.current_deps.contains(&dep) {
            self.current_deps.push(dep);
        }
    }
}

/// Error indicating a dependency cycle was detected.
#[derive(Debug, Clone)]
pub struct CycleError {
    pub cycle: Vec<Dependency>,
}

impl std::fmt::Display for CycleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Dependency cycle detected: ")?;
        for (i, dep) in self.cycle.iter().enumerate() {
            if i > 0 {
                write!(f, " -> ")?;
            }
            write!(f, "Query({:?}, key_hash={})", dep.query_type, dep.key_hash)?;
        }
        Ok(())
    }
}

impl std::error::Error for CycleError {}
