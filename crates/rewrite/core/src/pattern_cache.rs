use std::sync::Arc;

use dashmap::DashMap;

use crate::dependency::{Dependency, DependencyPattern};

/// Global cache for interning dependency patterns.
/// Multiple properties with identical dependencies share the same Arc.
pub struct PatternCache {
    patterns: DashMap<u64, Arc<DependencyPattern>>,
}

impl PatternCache {
    pub fn new() -> Self {
        Self {
            patterns: DashMap::new(),
        }
    }

    /// Intern a dependency pattern, returning a shared Arc.
    pub fn intern(&self, mut deps: Vec<Dependency>) -> Arc<DependencyPattern> {
        // Sort for deterministic hashing
        deps.sort_by_key(|d| (d.query_type, d.key_hash));

        let hash = Self::hash_deps(&deps);

        self.patterns
            .entry(hash)
            .or_insert_with(|| Arc::new(DependencyPattern::new(deps)))
            .clone()
    }

    fn hash_deps(deps: &[Dependency]) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        for dep in deps {
            dep.hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Get statistics about pattern sharing.
    pub fn stats(&self) -> PatternCacheStats {
        PatternCacheStats {
            unique_patterns: self.patterns.len(),
        }
    }
}

impl Default for PatternCache {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct PatternCacheStats {
    pub unique_patterns: usize,
}
