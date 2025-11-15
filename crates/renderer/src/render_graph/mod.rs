//! Render graph for orchestrating multi-pass rendering with proper dependencies.
//!
//! This module determines the execution order of render passes and when command
//! buffers need to be submitted for resource state transitions.

pub mod aliasing;
pub mod batching;
pub mod culling;
pub mod optimizer;

use crate::compositor::{OpacityGroup, Rect};
use crate::display_list::{DisplayItem, DisplayList};
pub use aliasing::{AliasGroup, AliasingStats, Lifetime, compute_alias_groups, compute_lifetimes};
pub use batching::{BatchType, BatchingStats, DrawBatch, batch_draw_calls};
pub use culling::{AABB, frustum_cull, occlusion_cull};
pub use optimizer::{
    DeadPassEliminationPass, OptimizationPass, OptimizationStats, PassMergingPass,
    PassReorderingPass,
};

/// Unique identifier for a render pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PassId(pub usize);

/// Unique identifier for a GPU resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResourceId(pub usize);

/// A single render pass in the graph.
#[derive(Debug, Clone)]
pub enum RenderPass {
    /// Clear the framebuffer.
    Clear,
    /// Render items to an offscreen texture for opacity compositing.
    OffscreenOpacity {
        group_id: usize,
        target: ResourceId,
        items: Vec<DisplayItem>,
        bounds: Rect,
        alpha: f32,
    },
    /// Main rendering pass with optional opacity composites.
    Main {
        items: Vec<DisplayItem>,
        exclude_ranges: Vec<(usize, usize)>,
        composites: Vec<OpacityComposite>,
    },
    /// Text rendering pass.
    Text { items: Vec<DisplayItem> },
}

/// Information about an opacity composite to apply in the main pass.
#[derive(Debug, Clone)]
pub struct OpacityComposite {
    pub group_id: usize,
    pub target: ResourceId,
    pub bounds: Rect,
    pub alpha: f32,
}

/// Dependency between two render passes.
#[derive(Debug, Clone)]
pub struct Dependency {
    pub from_pass: PassId,
    pub to_pass: PassId,
    pub resource: ResourceId,
}

/// High-level render graph that orchestrates multi-pass rendering.
pub struct RenderGraph {
    /// List of render passes to execute.
    passes: Vec<RenderPass>,
    /// Dependencies between passes.
    dependencies: Vec<Dependency>,
}

impl RenderGraph {
    /// Build a render graph from a display list.
    #[inline]
    pub fn build_from_display_list(
        display_list: &DisplayList,
        opacity_groups: &[OpacityGroup],
        needs_clear: bool,
    ) -> Self {
        let mut passes = Vec::new();
        let mut dependencies = Vec::new();

        // Pass 0: Clear (if needed)
        if needs_clear {
            passes.push(RenderPass::Clear);
        }

        let clear_offset = usize::from(needs_clear);

        // Passes 1..N: Offscreen opacity groups
        let mut composites = Vec::new();
        for (group_idx, group) in opacity_groups.iter().enumerate() {
            let resource_id = ResourceId(group_idx);

            let pass_id = PassId(passes.len());
            passes.push(RenderPass::OffscreenOpacity {
                group_id: group_idx,
                target: resource_id,
                items: group.items.clone(),
                bounds: group.bounds,
                alpha: group.alpha,
            });

            composites.push(OpacityComposite {
                group_id: group_idx,
                target: resource_id,
                bounds: group.bounds,
                alpha: group.alpha,
            });

            // Dependency: main pass depends on this offscreen pass
            dependencies.push(Dependency {
                from_pass: pass_id,
                to_pass: PassId(clear_offset + opacity_groups.len()),
                resource: resource_id,
            });
        }

        // Pass N: Main rendering
        let exclude_ranges: Vec<(usize, usize)> = opacity_groups
            .iter()
            .map(|group| (group.start_index, group.end_index))
            .collect();

        passes.push(RenderPass::Main {
            items: display_list.items.clone(),
            exclude_ranges,
            composites,
        });

        // Pass N+1: Text rendering
        let text_items: Vec<DisplayItem> = display_list
            .items
            .iter()
            .filter(|item| matches!(item, DisplayItem::Text { .. }))
            .cloned()
            .collect();

        if !text_items.is_empty() {
            passes.push(RenderPass::Text { items: text_items });
        }

        Self {
            passes,
            dependencies,
        }
    }

    /// Get all render passes in execution order.
    #[inline]
    pub fn passes(&self) -> &[RenderPass] {
        &self.passes
    }

    /// Get all dependencies.
    #[inline]
    pub fn dependencies(&self) -> &[Dependency] {
        &self.dependencies
    }

    /// Check if a submission is needed between two passes.
    ///
    /// For D3D12, we need to submit after all offscreen passes complete
    /// to ensure proper resource state transitions (`RENDER_TARGET` → `SHADER_RESOURCE`).
    #[inline]
    pub fn needs_submission_after(&self, pass_id: PassId) -> bool {
        // Check if any dependency originates from this pass
        self.dependencies.iter().any(|dep| dep.from_pass == pass_id)
    }

    /// Get the number of offscreen opacity passes.
    #[inline]
    pub fn offscreen_pass_count(&self) -> usize {
        self.passes
            .iter()
            .filter(|pass| matches!(pass, RenderPass::OffscreenOpacity { .. }))
            .count()
    }

    /// Get mutable access to render passes (used by optimization passes).
    #[inline]
    pub(crate) fn passes_mut(&mut self) -> &mut Vec<RenderPass> {
        &mut self.passes
    }

    /// Run all optimization passes on this render graph.
    ///
    /// This applies a series of optimizations to improve rendering performance:
    /// 1. Dead pass elimination - Remove passes with no visible output
    /// 2. Pass merging - Combine compatible passes to reduce overhead
    /// 3. Pass reordering - Optimize for cache coherence
    ///
    /// # Errors
    ///
    /// Returns an error if any optimization pass fails.
    ///
    /// # Examples
    ///
    /// ```
    /// # use renderer::display_list::DisplayList;
    /// # use renderer::compositor::OpacityCompositor;
    /// # use renderer::render_graph::RenderGraph;
    /// let display_list = DisplayList::new();
    /// let compositor = OpacityCompositor::collect_from_display_list(&display_list);
    /// let mut graph = RenderGraph::build_from_display_list(&display_list, compositor.groups(), true);
    /// let stats = graph.optimize().expect("Optimization should succeed");
    /// ```
    pub fn optimize(&mut self) -> Result<OptimizationStats, String> {
        let mut total_stats = OptimizationStats::new();

        // Pass 1: Dead pass elimination (remove unused passes first)
        let elimination_pass = DeadPassEliminationPass;
        let elimination_stats = elimination_pass.optimize(self)?;
        total_stats = total_stats.add(&elimination_stats);

        // Pass 2: Pass merging (combine compatible passes)
        let merging_pass = PassMergingPass;
        let merging_stats = merging_pass.optimize(self)?;
        total_stats = total_stats.add(&merging_stats);

        // Pass 3: Pass reordering (optimize for cache coherence)
        let reordering_pass = PassReorderingPass;
        let reordering_stats = reordering_pass.optimize(self)?;
        total_stats = total_stats.add(&reordering_stats);

        Ok(total_stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compositor::OpacityCompositor;
    use crate::display_list::StackingContextBoundary;

    /// Test render graph without opacity.
    ///
    /// # Panics
    /// Panics if the test assertions fail.
    #[test]
    fn render_graph_no_opacity() {
        let mut display_list = DisplayList::new();
        display_list.push(DisplayItem::Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
            color: [1.0, 0.0, 0.0, 1.0],
        });

        let compositor = OpacityCompositor::collect_from_display_list(&display_list);
        let graph = RenderGraph::build_from_display_list(&display_list, compositor.groups(), true);

        // Should have: Clear + Main + (maybe Text)
        assert!(graph.passes().len() >= 2);
        assert!(matches!(graph.passes()[0], RenderPass::Clear));
        assert!(matches!(graph.passes()[1], RenderPass::Main { .. }));
    }

    /// Test render graph with opacity.
    ///
    /// # Panics
    /// Panics if the test assertions fail.
    #[test]
    fn render_graph_with_opacity() {
        let mut display_list = DisplayList::new();
        display_list.items.push(DisplayItem::BeginStackingContext {
            boundary: StackingContextBoundary::Opacity { alpha: 0.5 },
        });
        display_list.push(DisplayItem::Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
            color: [1.0, 0.0, 0.0, 1.0],
        });
        display_list.items.push(DisplayItem::EndStackingContext);

        let compositor = OpacityCompositor::collect_from_display_list(&display_list);
        let graph = RenderGraph::build_from_display_list(&display_list, compositor.groups(), true);

        // Should have: Clear + OffscreenOpacity + Main
        assert!(graph.offscreen_pass_count() == 1);
        assert!(graph.dependencies().len() == 1);
    }
}
