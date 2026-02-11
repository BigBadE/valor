//! Parallel work scheduler for query execution.
//!
//! Provides a work-stealing scheduler for executing independent queries
//! in parallel across multiple threads.

use js::NodeKey;
use rayon::prelude::*;
use std::collections::BTreeMap;

/// Identifies a formatting context for parallel layout boundaries.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct FormattingContextId(pub u64);

/// Work unit for parallel execution.
#[derive(Clone, Debug)]
pub enum WorkUnit {
    /// Style computation for a subtree rooted at the given node.
    StyleSubtree {
        /// Root of the subtree to style
        root: NodeKey,
        /// Depth in the tree (for ordering)
        depth: usize,
    },

    /// Layout computation for a formatting context.
    LayoutFC {
        /// Formatting context ID
        fc_id: FormattingContextId,
        /// Root node of the formatting context
        root: NodeKey,
    },

    /// Paint for a stacking context.
    PaintStackingContext {
        /// Stacking context root
        root: NodeKey,
        /// Z-index for ordering
        z_index: i32,
    },
}

/// Parallel execution runtime using Rayon.
pub struct ParallelRuntime {
    /// Rayon thread pool
    pool: rayon::ThreadPool,
}

impl ParallelRuntime {
    /// Create a new parallel runtime with the specified number of threads.
    ///
    /// If `num_threads` is None, uses the number of CPU cores.
    ///
    /// # Errors
    ///
    /// Returns an error if the thread pool cannot be created.
    pub fn new(num_threads: Option<usize>) -> anyhow::Result<Self> {
        let mut builder = rayon::ThreadPoolBuilder::new();

        if let Some(num) = num_threads {
            builder = builder.num_threads(num);
        }

        let pool = builder.build()?;

        Ok(Self { pool })
    }

    /// Execute work units in parallel where possible.
    ///
    /// Work units are partitioned into waves based on their dependencies.
    /// Within each wave, units are executed in parallel.
    pub fn execute_parallel<F, R>(&self, units: Vec<WorkUnit>, execute: F) -> Vec<R>
    where
        F: Fn(&WorkUnit) -> R + Send + Sync,
        R: Send,
    {
        let waves = self.partition_into_waves(units);

        let mut results = Vec::new();

        for wave in waves {
            let wave_results: Vec<R> = self
                .pool
                .install(|| wave.par_iter().map(&execute).collect());
            results.extend(wave_results);
        }

        results
    }

    /// Execute style computation in parallel waves by tree depth.
    ///
    /// Returns results in the order they were computed (depth-first).
    pub fn execute_style_waves<F, R>(&self, units: Vec<WorkUnit>, execute: F) -> Vec<R>
    where
        F: Fn(&WorkUnit) -> R + Send + Sync,
        R: Send,
    {
        // Group by depth
        let mut by_depth: BTreeMap<usize, Vec<WorkUnit>> = BTreeMap::new();

        for unit in units {
            if let WorkUnit::StyleSubtree { depth, .. } = unit {
                by_depth.entry(depth).or_default().push(unit);
            }
        }

        let mut results = Vec::new();

        // Process depth waves in order (parents before children)
        for (_depth, wave) in by_depth {
            let wave_results: Vec<R> = self
                .pool
                .install(|| wave.par_iter().map(&execute).collect());
            results.extend(wave_results);
        }

        results
    }

    /// Partition work units into independent waves.
    ///
    /// Units within a wave have no dependencies on each other and can
    /// be executed in parallel.
    fn partition_into_waves(&self, units: Vec<WorkUnit>) -> Vec<Vec<WorkUnit>> {
        match units.first() {
            Some(WorkUnit::StyleSubtree { .. }) => {
                // Style: group by depth
                let mut by_depth: BTreeMap<usize, Vec<WorkUnit>> = BTreeMap::new();
                for unit in units {
                    if let WorkUnit::StyleSubtree { depth, .. } = unit {
                        by_depth.entry(depth).or_default().push(unit);
                    }
                }
                by_depth.into_values().collect()
            }
            Some(WorkUnit::LayoutFC { .. }) => {
                // Layout: for now, assume all FCs are independent
                // In a more sophisticated version, we'd build a dependency graph
                vec![units]
            }
            Some(WorkUnit::PaintStackingContext { .. }) => {
                // Paint: can execute all in parallel, ordering happens at composite
                vec![units]
            }
            None => vec![],
        }
    }

    /// Get a reference to the underlying thread pool.
    #[inline]
    pub fn pool(&self) -> &rayon::ThreadPool {
        &self.pool
    }
}

impl Default for ParallelRuntime {
    fn default() -> Self {
        Self::new(None).expect("Failed to create default parallel runtime")
    }
}

/// Helper to group nodes by depth in the tree for parallel style computation.
pub fn group_nodes_by_depth(
    nodes: &[NodeKey],
    get_depth: impl Fn(NodeKey) -> usize,
) -> BTreeMap<usize, Vec<NodeKey>> {
    let mut by_depth = BTreeMap::new();

    for &node in nodes {
        let depth = get_depth(node);
        by_depth.entry(depth).or_insert_with(Vec::new).push(node);
    }

    by_depth
}
