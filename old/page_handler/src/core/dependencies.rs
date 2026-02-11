//! Fine-grained dependency tracking for incremental layout computation.
//!
//! This module implements automatic dependency tracking that records what each
//! layout computation reads (style properties, parent size, child sizes, etc.)
//! and automatically invalidates only the affected computations when inputs change.

use js::NodeKey;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher as _};
use std::mem;
use std::sync::Arc;

/// Property ID for efficient dependency tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PropertyId(pub u32);

impl PropertyId {
    // Style properties
    pub const WIDTH: Self = Self(0);
    pub const HEIGHT: Self = Self(1);
    pub const PADDING_TOP: Self = Self(2);
    pub const PADDING_RIGHT: Self = Self(3);
    pub const PADDING_BOTTOM: Self = Self(4);
    pub const PADDING_LEFT: Self = Self(5);
    pub const MARGIN_TOP: Self = Self(6);
    pub const MARGIN_RIGHT: Self = Self(7);
    pub const MARGIN_BOTTOM: Self = Self(8);
    pub const MARGIN_LEFT: Self = Self(9);
    pub const BORDER_TOP_WIDTH: Self = Self(10);
    pub const BORDER_RIGHT_WIDTH: Self = Self(11);
    pub const BORDER_BOTTOM_WIDTH: Self = Self(12);
    pub const BORDER_LEFT_WIDTH: Self = Self(13);
    pub const DISPLAY: Self = Self(14);
    pub const POSITION: Self = Self(15);
    pub const FLEX_DIRECTION: Self = Self(16);
    pub const FLEX_GROW: Self = Self(17);
    pub const FLEX_SHRINK: Self = Self(18);
    pub const FLEX_BASIS: Self = Self(19);
    pub const JUSTIFY_CONTENT: Self = Self(20);
    pub const ALIGN_ITEMS: Self = Self(21);
    pub const ALIGN_SELF: Self = Self(22);
    pub const GAP: Self = Self(23);
    pub const GRID_TEMPLATE_COLUMNS: Self = Self(24);
    pub const GRID_TEMPLATE_ROWS: Self = Self(25);
    pub const FONT_SIZE: Self = Self(26);
    pub const FONT_FAMILY: Self = Self(27);
    pub const FONT_WEIGHT: Self = Self(28);
    pub const LINE_HEIGHT: Self = Self(29);
    pub const OVERFLOW: Self = Self(30);
    pub const COLOR: Self = Self(31);
    pub const BACKGROUND_COLOR: Self = Self(32);
}

/// What a computation depends on
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Dependency {
    /// Style property of a node
    StyleProperty(NodeKey, PropertyId),

    /// Size of parent container
    ParentSize(NodeKey),

    /// Size of a specific child
    ChildSize(NodeKey, usize),

    /// Sizes of all children
    AllChildrenSizes(NodeKey),

    /// Position of parent (for absolute positioning)
    ParentPosition(NodeKey),

    /// Viewport dimensions
    ViewportSize,

    /// Text content of a node
    TextContent(NodeKey),
}

/// Interned handle to a dependency set
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DependencySetId(u32);

/// Dependency set with interning for memory efficiency
#[derive(Debug, Default)]
pub struct DependencyInterner {
    sets: Vec<Arc<HashSet<Dependency>>>,
    set_to_id: HashMap<u64, DependencySetId>,
}

impl DependencyInterner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern a dependency set and return its ID
    pub fn intern(&mut self, deps: HashSet<Dependency>) -> DependencySetId {
        // Hash the set for quick lookup
        let mut hasher = DefaultHasher::new();
        // Sort dependencies for consistent hashing
        let mut sorted: Vec<_> = deps.iter().collect();
        sorted.sort_by_key(|dep| format!("{dep:?}"));
        for dep in &sorted {
            dep.hash(&mut hasher);
        }
        let hash = hasher.finish();

        // Check if we already have this set
        if let Some(&id) = self.set_to_id.get(&hash) {
            return id;
        }

        // New set - intern it
        let id = DependencySetId(self.sets.len() as u32);
        self.sets.push(Arc::new(deps));
        self.set_to_id.insert(hash, id);
        id
    }

    /// Get the dependency set for an ID
    pub fn get(&self, id: DependencySetId) -> Option<&Arc<HashSet<Dependency>>> {
        self.sets.get(id.0 as usize)
    }
}

/// Tracks what each node's layout depends on
#[derive(Debug, Default)]
pub struct DependencyGraph {
    /// Forward: node -> dependencies
    dependencies: HashMap<NodeKey, DependencySetId>,

    /// Reverse: dependency -> nodes that depend on it
    dependents: HashMap<Dependency, HashSet<NodeKey>>,

    /// Dependency set interner
    interner: DependencyInterner,
}

impl DependencyGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that a node depends on a set of dependencies
    pub fn set_dependencies(&mut self, node: NodeKey, deps: HashSet<Dependency>) {
        // Remove old dependencies if any
        self.remove_node(node);

        // Intern the new set
        let dep_id = self.interner.intern(deps.clone());
        self.dependencies.insert(node, dep_id);

        // Build reverse index
        for dep in deps {
            self.dependents.entry(dep).or_default().insert(node);
        }
    }

    /// Get all nodes that depend on a specific dependency
    pub fn get_dependents(&self, dep: &Dependency) -> Option<&HashSet<NodeKey>> {
        self.dependents.get(dep)
    }

    /// Get the dependency set for a node
    pub fn get_dependencies(&self, node: NodeKey) -> Option<&Arc<HashSet<Dependency>>> {
        let dep_id = self.dependencies.get(&node)?;
        self.interner.get(*dep_id)
    }

    /// Remove a node from the graph
    pub fn remove_node(&mut self, node: NodeKey) {
        if let Some(dep_id) = self.dependencies.remove(&node)
            && let Some(deps) = self.interner.get(dep_id)
        {
            // Clone deps to avoid borrow conflict
            let deps_vec: Vec<_> = deps.iter().cloned().collect();
            // Remove from reverse index
            for dep in deps_vec {
                self.remove_dependent_for_dep(&dep, node);
            }
        }
    }

    /// Remove a node from a dependency's dependent list
    fn remove_dependent_for_dep(&mut self, dep: &Dependency, node: NodeKey) {
        if let Some(dependents) = self.dependents.get_mut(dep) {
            dependents.remove(&node);
            if dependents.is_empty() {
                self.dependents.remove(dep);
            }
        }
    }

    /// Find all nodes that need recomputation when a dependency changes
    pub fn invalidate(&self, changed: &Dependency) -> HashSet<NodeKey> {
        self.dependents.get(changed).cloned().unwrap_or_default()
    }

    /// Find all nodes affected by multiple dependency changes
    pub fn invalidate_many(&self, changes: &[Dependency]) -> HashSet<NodeKey> {
        let mut affected = HashSet::new();
        for change in changes {
            if let Some(nodes) = self.dependents.get(change) {
                affected.extend(nodes);
            }
        }
        affected
    }
}

/// Context that records dependencies during computation
#[derive(Debug)]
pub struct DependencyTracker {
    current_deps: HashSet<Dependency>,
    enabled: bool,
}

impl DependencyTracker {
    pub fn new() -> Self {
        Self {
            current_deps: HashSet::new(),
            enabled: true,
        }
    }

    /// Start tracking dependencies for a computation
    pub fn start(&mut self) {
        self.current_deps.clear();
        self.enabled = true;
    }

    /// Stop tracking and return collected dependencies
    pub fn finish(&mut self) -> HashSet<Dependency> {
        self.enabled = false;
        mem::take(&mut self.current_deps)
    }

    /// Record that computation depends on this
    pub fn record(&mut self, dep: Dependency) {
        if self.enabled {
            self.current_deps.insert(dep);
        }
    }

    /// Check if tracking is active
    pub fn is_tracking(&self) -> bool {
        self.enabled
    }
}

impl Default for DependencyTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::iter;

    /// Test dependency interning.
    ///
    /// # Panics
    ///
    /// Panics if dependency interning does not work as expected.
    #[test]
    fn test_dependency_interning() {
        let mut interner = DependencyInterner::new();

        let deps1: HashSet<_> = [
            Dependency::StyleProperty(NodeKey::ROOT, PropertyId::WIDTH),
            Dependency::ParentSize(NodeKey::ROOT),
        ]
        .into_iter()
        .collect();

        let deps2 = deps1.clone();

        let id1 = interner.intern(deps1);
        let id2 = interner.intern(deps2);

        // Same sets should get same ID
        assert_eq!(id1, id2);
        assert_eq!(interner.sets.len(), 1);
    }

    /// Test dependency graph.
    ///
    /// # Panics
    ///
    /// Panics if dependency graph invalidation does not work as expected.
    #[test]
    fn test_dependency_graph() {
        let mut graph = DependencyGraph::new();

        let node_a = NodeKey::ROOT;
        let node_b = NodeKey::ROOT;

        let deps1: HashSet<_> =
            iter::once(Dependency::StyleProperty(node_a, PropertyId::WIDTH)).collect();

        let deps2: HashSet<_> =
            iter::once(Dependency::StyleProperty(node_a, PropertyId::WIDTH)).collect();

        graph.set_dependencies(node_a, deps1);
        graph.set_dependencies(node_b, deps2);

        // Both depend on same property
        let change = Dependency::StyleProperty(node_a, PropertyId::WIDTH);
        let affected = graph.invalidate(&change);

        // Since both nodes are ROOT (same key), only 1 unique node
        assert!(!affected.is_empty());
        assert!(affected.contains(&node_a));
    }
}
