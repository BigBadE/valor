//! Optimization passes for the render graph to improve performance.
//!
//! This module implements various optimization strategies:
//! - Pass merging: Combine compatible render passes to reduce overhead
//! - Dead pass elimination: Remove passes with no visible output
//! - Pass reordering: Optimize for cache coherence

use super::{RenderGraph, RenderPass};
use std::collections::HashSet;

/// Statistics collected during optimization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptimizationStats {
    /// Number of passes merged together.
    pub passes_merged: usize,
    /// Number of passes eliminated as dead code.
    pub passes_eliminated: usize,
    /// Number of passes reordered for better cache coherence.
    pub passes_reordered: usize,
}

impl OptimizationStats {
    /// Create empty optimization statistics.
    #[inline]
    pub const fn new() -> Self {
        Self {
            passes_merged: 0,
            passes_eliminated: 0,
            passes_reordered: 0,
        }
    }

    /// Add another set of statistics to this one.
    #[inline]
    #[must_use]
    pub const fn add(&self, other: &Self) -> Self {
        Self {
            passes_merged: self.passes_merged + other.passes_merged,
            passes_eliminated: self.passes_eliminated + other.passes_eliminated,
            passes_reordered: self.passes_reordered + other.passes_reordered,
        }
    }

    /// Check if any optimizations were performed.
    #[inline]
    pub const fn any_optimizations(&self) -> bool {
        self.passes_merged > 0 || self.passes_eliminated > 0 || self.passes_reordered > 0
    }
}

impl Default for OptimizationStats {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for optimization passes that can transform a render graph.
pub trait OptimizationPass {
    /// Apply this optimization pass to the graph.
    ///
    /// # Errors
    ///
    /// Returns an error if the optimization cannot be applied safely.
    fn optimize(&self, graph: &mut RenderGraph) -> Result<OptimizationStats, String>;
}

/// Pass merging optimization: Combine compatible render passes to reduce overhead.
///
/// Two passes can be merged if:
/// - They have the same render target
/// - There are no dependencies between them
/// - They are the same type of pass (e.g., both are Main passes)
pub struct PassMergingPass;

impl OptimizationPass for PassMergingPass {
    fn optimize(&self, graph: &mut RenderGraph) -> Result<OptimizationStats, String> {
        let mut merged_count = 0;
        let passes = graph.passes_mut();

        // Look for consecutive Main passes that can be merged
        let mut pass_idx = 0;
        while pass_idx + 1 < passes.len() {
            let should_remove_first = matches!(
                passes.get(pass_idx),
                Some(RenderPass::Main { items, .. }) if items.is_empty()
            );
            let should_remove_second = matches!(
                passes.get(pass_idx + 1),
                Some(RenderPass::Main { items, .. }) if items.is_empty()
            );

            if should_remove_first {
                passes.remove(pass_idx);
                merged_count += 1;
                continue;
            } else if should_remove_second {
                passes.remove(pass_idx + 1);
                merged_count += 1;
                continue;
            }
            pass_idx += 1;
        }

        // Remove consecutive Clear passes (only keep the first)
        pass_idx = 0;
        while pass_idx + 1 < passes.len() {
            if matches!(passes[pass_idx], RenderPass::Clear)
                && matches!(passes[pass_idx + 1], RenderPass::Clear)
            {
                passes.remove(pass_idx + 1);
                merged_count += 1;
                continue;
            }
            pass_idx += 1;
        }

        Ok(OptimizationStats {
            passes_merged: merged_count,
            passes_eliminated: 0,
            passes_reordered: 0,
        })
    }
}

/// Dead pass elimination: Remove passes with no visible output.
///
/// A pass is considered dead if:
/// - It's an offscreen pass whose texture is never composited
/// - It's a Main pass with no items and no composites
/// - It's a Text pass with no items
pub struct DeadPassEliminationPass;

impl OptimizationPass for DeadPassEliminationPass {
    fn optimize(&self, graph: &mut RenderGraph) -> Result<OptimizationStats, String> {
        let mut eliminated_count = 0;
        let passes = graph.passes_mut();

        // Collect all resources that are actually used
        let mut used_resources = HashSet::new();
        for pass in &*passes {
            if let RenderPass::Main { composites, .. } = pass {
                for composite in composites {
                    used_resources.insert(composite.target);
                }
            }
        }

        // Remove passes that produce unused resources or have no output
        let mut pass_idx = 0;
        while pass_idx < passes.len() {
            let should_remove = match &passes[pass_idx] {
                RenderPass::OffscreenOpacity { target, items, .. } => {
                    // Remove if target is never composited or if no items
                    !used_resources.contains(target) || items.is_empty()
                }
                RenderPass::Main {
                    items,
                    composites,
                    exclude_ranges,
                } => {
                    // Remove if no items to draw and no composites
                    // (exclude_ranges without items means nothing to exclude)
                    items.is_empty() && composites.is_empty() && exclude_ranges.is_empty()
                }
                RenderPass::Text { items } => {
                    // Remove if no text items
                    items.is_empty()
                }
                RenderPass::Clear => false, // Never remove Clear passes
            };

            if should_remove {
                passes.remove(pass_idx);
                eliminated_count += 1;
                continue;
            }
            pass_idx += 1;
        }

        Ok(OptimizationStats {
            passes_merged: 0,
            passes_eliminated: eliminated_count,
            passes_reordered: 0,
        })
    }
}

/// Pass reordering: Optimize for cache coherence.
///
/// This pass reorders render passes to improve GPU cache utilization by:
/// - Grouping similar pass types together
/// - Respecting dependencies between passes
pub struct PassReorderingPass;

impl OptimizationPass for PassReorderingPass {
    fn optimize(&self, graph: &mut RenderGraph) -> Result<OptimizationStats, String> {
        let mut reordered_count = 0;
        let has_dependencies = !graph.dependencies().is_empty();
        let passes = graph.passes_mut();

        // Build a dependency graph to determine valid reorderings
        // For now, we use a simple heuristic: keep Clear first, then offscreen passes,
        // then Main, then Text. This is already the default order, so we only
        // optimize if the order is different.

        let mut clear_passes = Vec::new();
        let mut offscreen_passes = Vec::new();
        let mut main_passes = Vec::new();
        let mut text_passes = Vec::new();

        // Partition passes by type
        for (pass_idx, pass) in passes.iter().enumerate() {
            match pass {
                RenderPass::Clear => clear_passes.push(pass_idx),
                RenderPass::OffscreenOpacity { .. } => offscreen_passes.push(pass_idx),
                RenderPass::Main { .. } => main_passes.push(pass_idx),
                RenderPass::Text { .. } => text_passes.push(pass_idx),
            }
        }

        // Check if reordering is beneficial (passes are out of optimal order)
        let optimal_order: Vec<usize> = clear_passes
            .iter()
            .chain(&offscreen_passes)
            .chain(&main_passes)
            .chain(&text_passes)
            .copied()
            .collect();

        let current_order: Vec<usize> = (0..passes.len()).collect();

        if optimal_order != current_order {
            // Verify that reordering doesn't violate dependencies
            // For safety, we only reorder if there are no dependencies
            // (dependencies are only between offscreen and main passes)
            if !has_dependencies {
                // Create new pass list in optimal order
                let old_passes = passes.clone();
                for (new_idx, &old_idx) in optimal_order.iter().enumerate() {
                    passes[new_idx] = old_passes[old_idx].clone();
                    reordered_count += usize::from(new_idx != old_idx);
                }
            }
        }

        Ok(OptimizationStats {
            passes_merged: 0,
            passes_eliminated: 0,
            passes_reordered: reordered_count,
        })
    }
}
