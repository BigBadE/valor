use std::any::Any;
use std::sync::Arc;
use std::thread;

use dashmap::DashMap;

use crate::dependency::{Dependency, DependencyContext};
use crate::input::{Input, InputQuery};
use crate::pattern_cache::PatternCache;
use crate::property_state::{PropertyKey, PropertyState};
use crate::{NodeId, Query, Relationship};

/// Node relationships in the tree.
#[derive(Debug, Clone, Default)]
struct NodeRelationships {
    parent: Option<NodeId>,
    children: Vec<NodeId>,
    next_sibling: Option<NodeId>,
    prev_sibling: Option<NodeId>,
}

/// Central query database with memoization and dependency tracking.
pub struct Database {
    /// Property states (unevaluated, computing, or evaluated with dependencies).
    properties: DashMap<PropertyKey, PropertyState>,

    /// Reverse dependency index: property -> what depends on it.
    dependents: DashMap<PropertyKey, Vec<PropertyKey>>,

    /// Global pattern cache for sharing dependency patterns.
    pattern_cache: PatternCache,

    /// Node relationship graph.
    relationships: DashMap<NodeId, NodeRelationships>,

    /// Input data storage (external data that queries depend on).
    inputs: DashMap<PropertyKey, Arc<dyn Any + Send + Sync>>,

    /// Next node ID to allocate.
    next_node_id: DashMap<(), u64>,
}

impl Database {
    pub fn new() -> Self {
        let next_node_id = DashMap::new();
        next_node_id.insert((), 0);
        Self {
            properties: DashMap::new(),
            dependents: DashMap::new(),
            pattern_cache: PatternCache::new(),
            relationships: DashMap::new(),
            inputs: DashMap::new(),
            next_node_id,
        }
    }

    /// Create a new node and return its ID.
    pub fn create_node(&self) -> NodeId {
        let mut entry = self.next_node_id.entry(()).or_insert(0);
        let id = *entry;
        *entry += 1;
        drop(entry);
        NodeId::new(id)
    }

    /// Set an input value.
    pub fn set_input<I: Input>(&self, key: I::Key, value: I::Value) {
        let prop_key = PropertyKey::new::<InputQuery<I>>(&key);
        self.inputs.insert(prop_key.clone(), Arc::new(value));

        // Invalidate any queries that depend on this input
        self.invalidate_key(&prop_key);
    }

    /// Get an input value.
    pub fn get_input<I: Input>(&self, key: &I::Key) -> Option<I::Value> {
        let prop_key = PropertyKey::new::<InputQuery<I>>(key);
        self.inputs
            .get(&prop_key)
            .and_then(|v| v.downcast_ref::<I::Value>().cloned())
    }

    /// Execute a query and return the memoized result.
    /// Uses ownership-based claiming for lock-free computation.
    pub fn query<Q: Query>(&self, key: Q::Key, ctx: &mut DependencyContext) -> Q::Value
    where
        Q::Value: Clone + Send + Sync,
    {
        let prop_key = PropertyKey::new::<Q>(&key);

        // Record this as a dependency if we're in a query execution
        ctx.record_dependency(Dependency::new::<Q>(key.clone()));

        // Check if already evaluated
        if let Some(state) = self.properties.get(&prop_key) {
            if let PropertyState::Evaluated { value, .. } = &*state {
                if let Some(val) = value.downcast_ref::<Q::Value>() {
                    return val.clone();
                }
            }
        }

        // Try to claim ownership
        let current_thread = thread::current().id();

        // Use DashMap's entry API properly
        let mut entry = self
            .properties
            .entry(prop_key.clone())
            .or_insert(PropertyState::Unevaluated);
        let claimed = match &*entry {
            PropertyState::Unevaluated => {
                *entry = PropertyState::Computing(current_thread);
                true
            }
            PropertyState::Computing(tid) if *tid == current_thread => true,
            _ => false,
        };
        drop(entry);

        if !claimed {
            // Someone else is computing or already computed it
            // For now, just spin-wait (TODO: proper work-stealing/helping)
            loop {
                if let Some(state) = self.properties.get(&prop_key) {
                    if let PropertyState::Evaluated { value, .. } = &*state {
                        if let Some(val) = value.downcast_ref::<Q::Value>() {
                            return val.clone();
                        }
                    }
                }
                thread::yield_now();
            }
        }

        // We own it - compute synchronously
        ctx.begin_query(Dependency::new::<Q>(key.clone()))
            .expect("Dependency cycle detected");

        // Execute query (this may recursively call query() on other properties)
        let value = Q::execute(self, key, ctx);

        // Get dependencies that were recorded
        let deps = ctx.end_query();

        // Intern the dependency pattern
        let pattern = self.pattern_cache.intern(deps);

        // Update reverse dependency index
        for dep in pattern.dependencies() {
            let dep_key = PropertyKey {
                query_type: dep.query_type,
                key_hash: dep.key_hash,
            };
            self.dependents
                .entry(dep_key)
                .or_insert_with(Vec::new)
                .push(prop_key.clone());
        }

        // Store the computed value
        self.properties.insert(
            prop_key,
            PropertyState::Evaluated {
                value: Arc::new(value.clone()) as Arc<dyn Any + Send + Sync>,
                dependencies: pattern,
            },
        );

        value
    }

    /// Resolve a relationship to get related node IDs.
    pub fn resolve_relationship(&self, node: NodeId, rel: Relationship) -> Vec<NodeId> {
        let rels = self.relationships.get(&node);

        match rel {
            Relationship::Parent => rels.as_ref().and_then(|r| r.parent).into_iter().collect(),
            Relationship::Children => rels
                .as_ref()
                .map(|r| r.children.clone())
                .unwrap_or_default(),
            Relationship::NextSibling => rels
                .as_ref()
                .and_then(|r| r.next_sibling)
                .into_iter()
                .collect(),
            Relationship::PreviousSibling => rels
                .as_ref()
                .and_then(|r| r.prev_sibling)
                .into_iter()
                .collect(),
            Relationship::PreviousSiblings => {
                let mut result = Vec::new();
                let mut current = rels.as_ref().and_then(|r| r.prev_sibling);
                while let Some(sibling) = current {
                    result.push(sibling);
                    current = self
                        .relationships
                        .get(&sibling)
                        .as_ref()
                        .and_then(|r| r.prev_sibling);
                }
                result
            }
            Relationship::NextSiblings => {
                let mut result = Vec::new();
                let mut current = rels.as_ref().and_then(|r| r.next_sibling);
                while let Some(sibling) = current {
                    result.push(sibling);
                    current = self
                        .relationships
                        .get(&sibling)
                        .as_ref()
                        .and_then(|r| r.next_sibling);
                }
                result
            }
            Relationship::Siblings => {
                let mut result = self.resolve_relationship(node, Relationship::PreviousSiblings);
                result.extend(self.resolve_relationship(node, Relationship::NextSiblings));
                result
            }
            Relationship::Ancestors => {
                let mut result = Vec::new();
                let mut current = rels.as_ref().and_then(|r| r.parent);
                while let Some(ancestor) = current {
                    result.push(ancestor);
                    current = self
                        .relationships
                        .get(&ancestor)
                        .as_ref()
                        .and_then(|r| r.parent);
                }
                result
            }
            Relationship::Descendants => {
                let children = rels
                    .as_ref()
                    .map(|r| r.children.clone())
                    .unwrap_or_default();
                let mut result = children.clone();
                for child in children {
                    result.extend(self.resolve_relationship(child, Relationship::Descendants));
                }
                result
            }
        }
    }

    /// Establish a relationship between nodes.
    pub fn establish_relationship(&self, node: NodeId, rel: Relationship, target: NodeId) {
        match rel {
            Relationship::Parent => {
                self.relationships.entry(node).or_default().parent = Some(target);
            }
            Relationship::Children => {
                self.relationships
                    .entry(node)
                    .or_default()
                    .children
                    .push(target);
            }
            Relationship::NextSibling => {
                self.relationships.entry(node).or_default().next_sibling = Some(target);
            }
            Relationship::PreviousSibling => {
                self.relationships.entry(node).or_default().prev_sibling = Some(target);
            }
            _ => {
                // Other relationships are derived
            }
        }
    }

    /// Invalidate a specific property and all properties that depend on it.
    pub fn invalidate<Q: Query>(&self, key: &Q::Key) {
        let prop_key = PropertyKey::new::<Q>(key);
        self.invalidate_key(&prop_key);
    }

    fn invalidate_key(&self, key: &PropertyKey) {
        // Mark as unevaluated
        self.properties
            .insert(key.clone(), PropertyState::Unevaluated);

        // Recursively invalidate all dependents
        if let Some(deps) = self.dependents.get(key) {
            for dependent in deps.value() {
                self.invalidate_key(dependent);
            }
        }
    }

    /// Clear all cached results.
    pub fn clear(&self) {
        self.properties.clear();
        self.dependents.clear();
        self.inputs.clear();
    }

    /// Get statistics about the database.
    pub fn stats(&self) -> DatabaseStats {
        let mut evaluated = 0;
        let mut computing = 0;
        let mut unevaluated = 0;

        for entry in self.properties.iter() {
            match entry.value() {
                PropertyState::Evaluated { .. } => evaluated += 1,
                PropertyState::Computing(_) => computing += 1,
                PropertyState::Unevaluated => unevaluated += 1,
            }
        }

        DatabaseStats {
            total_properties: self.properties.len(),
            evaluated,
            computing,
            unevaluated,
            pattern_cache: self.pattern_cache.stats(),
        }
    }
}

impl Default for Database {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct DatabaseStats {
    pub total_properties: usize,
    pub evaluated: usize,
    pub computing: usize,
    pub unevaluated: usize,
    pub pattern_cache: crate::pattern_cache::PatternCacheStats,
}
