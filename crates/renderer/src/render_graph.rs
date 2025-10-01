//! Render graph for orchestrating multi-pass rendering with proper dependencies.
//!
//! This module determines the execution order of render passes and when command
//! buffers need to be submitted for resource state transitions.

use crate::compositor::{OpacityGroup, Rect};
use crate::display_list::{DisplayItem, DisplayList};

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
    passes: Vec<RenderPass>,
    dependencies: Vec<Dependency>,
}

impl RenderGraph {
    /// Build a render graph from a display list.
    pub fn build_from_display_list(
        dl: &DisplayList,
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
            .map(|g| (g.start_index, g.end_index))
            .collect();

        passes.push(RenderPass::Main {
            items: dl.items.clone(),
            exclude_ranges,
            composites,
        });

        // Pass N+1: Text rendering
        let text_items: Vec<DisplayItem> = dl
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
    pub fn passes(&self) -> &[RenderPass] {
        &self.passes
    }

    /// Get all dependencies.
    pub fn dependencies(&self) -> &[Dependency] {
        &self.dependencies
    }

    /// Check if a submission is needed between two passes.
    ///
    /// For D3D12, we need to submit after all offscreen passes complete
    /// to ensure proper resource state transitions (RENDER_TARGET → SHADER_RESOURCE).
    pub fn needs_submission_after(&self, pass_id: PassId) -> bool {
        // Check if any dependency originates from this pass
        self.dependencies.iter().any(|dep| dep.from_pass == pass_id)
    }

    /// Get the number of offscreen passes.
    pub fn offscreen_pass_count(&self) -> usize {
        self.passes
            .iter()
            .filter(|p| matches!(p, RenderPass::OffscreenOpacity { .. }))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compositor::OpacityCompositor;
    use crate::display_list::StackingContextBoundary;

    #[test]
    fn render_graph_no_opacity() {
        let mut dl = DisplayList::new();
        dl.items.push(DisplayItem::Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
            color: [1.0, 0.0, 0.0, 1.0],
        });

        let compositor = OpacityCompositor::collect_from_display_list(&dl);
        let graph = RenderGraph::build_from_display_list(&dl, compositor.groups(), true);

        // Should have: Clear + Main + (maybe Text)
        assert!(graph.passes().len() >= 2);
        assert!(matches!(graph.passes()[0], RenderPass::Clear));
        assert!(matches!(graph.passes()[1], RenderPass::Main { .. }));
    }

    #[test]
    fn render_graph_with_opacity() {
        let mut dl = DisplayList::new();
        dl.items.push(DisplayItem::BeginStackingContext {
            boundary: StackingContextBoundary::Opacity { alpha: 0.5 },
        });
        dl.items.push(DisplayItem::Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
            color: [1.0, 0.0, 0.0, 1.0],
        });
        dl.items.push(DisplayItem::EndStackingContext);

        let compositor = OpacityCompositor::collect_from_display_list(&dl);
        let graph = RenderGraph::build_from_display_list(&dl, compositor.groups(), true);

        // Should have: Clear + OffscreenOpacity + Main
        assert!(graph.offscreen_pass_count() == 1);
        assert!(graph.dependencies().len() == 1);
    }
}
