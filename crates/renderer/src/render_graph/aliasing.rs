//! Texture aliasing optimization for efficient offscreen texture reuse.
//!
//! This module analyzes render pass dependencies to determine when offscreen
//! textures can be safely reused (aliased) because their lifetimes don't overlap.
//! This reduces peak GPU memory usage for complex pages with many stacking contexts.

use super::{PassId, RenderGraph, RenderPass, ResourceId};
use std::collections::HashMap;

/// Represents the lifetime of a resource in the render graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lifetime {
    /// First pass that writes to this resource.
    pub created_at: PassId,
    /// Last pass that reads from this resource.
    pub last_used_at: PassId,
}

impl Lifetime {
    /// Create a new lifetime span.
    #[inline]
    pub const fn new(created_at: PassId, last_used_at: PassId) -> Self {
        Self {
            created_at,
            last_used_at,
        }
    }

    /// Check if this lifetime overlaps with another lifetime.
    #[inline]
    pub fn overlaps(&self, other: &Self) -> bool {
        // Two lifetimes overlap if one starts before the other ends
        self.created_at.0 <= other.last_used_at.0 && other.created_at.0 <= self.last_used_at.0
    }
}

/// Alias group representing resources that can share the same GPU texture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasGroup {
    /// Resources in this alias group (can share the same texture).
    pub resources: Vec<ResourceId>,
    /// Maximum dimensions needed for this group (width, height).
    pub max_dimensions: (u32, u32),
}

/// Compute resource lifetimes from a render graph.
///
/// # Examples
///
/// ```
/// # use renderer::render_graph::{RenderGraph, PassId, ResourceId};
/// # use renderer::render_graph::aliasing::compute_lifetimes;
/// # use renderer::DisplayList;
/// # use renderer::compositor::OpacityCompositor;
/// let display_list = DisplayList::new();
/// let compositor = OpacityCompositor::collect_from_display_list(&display_list);
/// let graph = RenderGraph::build_from_display_list(&display_list, compositor.groups(), true);
/// let lifetimes = compute_lifetimes(&graph);
/// ```
pub fn compute_lifetimes(graph: &RenderGraph) -> HashMap<ResourceId, Lifetime> {
    let mut lifetimes: HashMap<ResourceId, Lifetime> = HashMap::new();

    // Scan all passes to find where resources are created and used
    for (pass_idx, pass) in graph.passes().iter().enumerate() {
        let pass_id = PassId(pass_idx);

        match pass {
            RenderPass::OffscreenOpacity { target, .. } => {
                // Resource is created in this pass
                lifetimes
                    .entry(*target)
                    .or_insert_with(|| Lifetime::new(pass_id, pass_id));
            }
            RenderPass::Main { composites, .. } => {
                // Resources are read (composited) in this pass
                for composite in composites {
                    if let Some(lifetime) = lifetimes.get_mut(&composite.target) {
                        // Extend lifetime to include this usage
                        lifetime.last_used_at = pass_id;
                    }
                }
            }
            RenderPass::Clear | RenderPass::Text { .. } => {
                // These passes don't create or use offscreen resources
            }
        }
    }

    lifetimes
}

/// Compute alias groups using greedy coloring algorithm.
///
/// This function groups resources that don't have overlapping lifetimes
/// so they can share the same GPU texture. It uses a greedy coloring
/// approach where each "color" represents a separate GPU texture.
///
/// # Algorithm
///
/// 1. Sort resources by creation time (earliest first)
/// 2. For each resource, try to assign it to an existing group
/// 3. If it overlaps with all existing groups, create a new group
///
/// # Examples
///
/// ```
/// # use renderer::render_graph::{RenderGraph, ResourceId};
/// # use renderer::render_graph::aliasing::{compute_lifetimes, compute_alias_groups};
/// # use renderer::DisplayList;
/// # use renderer::compositor::OpacityCompositor;
/// let display_list = DisplayList::new();
/// let compositor = OpacityCompositor::collect_from_display_list(&display_list);
/// let graph = RenderGraph::build_from_display_list(&display_list, compositor.groups(), true);
/// let lifetimes = compute_lifetimes(&graph);
/// let groups = compute_alias_groups(&graph, &lifetimes);
/// // Each group can use the same GPU texture
/// ```
pub fn compute_alias_groups(
    graph: &RenderGraph,
    lifetimes: &HashMap<ResourceId, Lifetime>,
) -> Vec<AliasGroup> {
    // Type alias to simplify complex type
    type ResourceWithDims = (ResourceId, Lifetime, (u32, u32));

    if lifetimes.is_empty() {
        return Vec::new();
    }

    // Collect resources with their dimensions
    let mut resources_with_dims: Vec<ResourceWithDims> = Vec::new();

    for pass in graph.passes() {
        if let RenderPass::OffscreenOpacity { target, bounds, .. } = pass
            && let Some(lifetime) = lifetimes.get(target)
        {
            let dims = (bounds.width.ceil() as u32, bounds.height.ceil() as u32);
            resources_with_dims.push((*target, *lifetime, dims));
        }
    }

    // Sort by creation time for greedy assignment
    resources_with_dims.sort_by_key(|(_, lifetime, _)| lifetime.created_at.0);

    let mut groups: Vec<AliasGroup> = Vec::new();

    // Greedy coloring: assign each resource to first compatible group
    for (resource, lifetime, dims) in resources_with_dims {
        let mut assigned = false;

        // Try to assign to an existing group
        for group in &mut groups {
            // Check if this resource overlaps with any resource in the group
            let can_assign = !group.resources.iter().any(|&other_resource| {
                lifetimes
                    .get(&other_resource)
                    .is_some_and(|other_lifetime| lifetime.overlaps(other_lifetime))
            });

            if can_assign {
                group.resources.push(resource);
                // Update max dimensions
                group.max_dimensions.0 = group.max_dimensions.0.max(dims.0);
                group.max_dimensions.1 = group.max_dimensions.1.max(dims.1);
                assigned = true;
                break;
            }
        }

        // Create new group if couldn't assign to existing
        if !assigned {
            groups.push(AliasGroup {
                resources: vec![resource],
                max_dimensions: dims,
            });
        }
    }

    groups
}

/// Statistics about texture aliasing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AliasingStats {
    /// Total number of offscreen resources.
    pub total_resources: usize,
    /// Number of alias groups (GPU textures needed).
    pub alias_groups: usize,
    /// Memory saved (estimated reduction in peak textures).
    pub textures_saved: usize,
}

impl AliasingStats {
    /// Compute aliasing statistics.
    #[inline]
    pub const fn from_groups(total_resources: usize, alias_groups: usize) -> Self {
        let textures_saved = total_resources.saturating_sub(alias_groups);
        Self {
            total_resources,
            alias_groups,
            textures_saved,
        }
    }

    /// Calculate memory savings percentage.
    #[inline]
    pub fn savings_percentage(&self) -> f32 {
        if self.total_resources == 0 {
            0.0
        } else {
            (self.textures_saved as f32 / self.total_resources as f32) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compositor::OpacityCompositor;
    use crate::display_list::{DisplayItem, DisplayList, StackingContextBoundary};

    /// Test lifetime overlap detection.
    ///
    /// # Panics
    /// Panics if the assertions fail.
    #[test]
    fn lifetime_overlap_detection() {
        let lifetime1 = Lifetime::new(PassId(0), PassId(2));
        let lifetime2 = Lifetime::new(PassId(1), PassId(3));
        let lifetime3 = Lifetime::new(PassId(3), PassId(5));

        assert!(lifetime1.overlaps(&lifetime2));
        assert!(lifetime2.overlaps(&lifetime1));
        assert!(lifetime2.overlaps(&lifetime3));
        assert!(!lifetime1.overlaps(&lifetime3));
    }

    /// Test computing lifetimes with no opacity groups.
    ///
    /// # Panics
    /// Panics if the assertions fail.
    #[test]
    fn compute_lifetimes_no_opacity() {
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
        let lifetimes = compute_lifetimes(&graph);

        assert!(lifetimes.is_empty());
    }

    /// Test computing lifetimes with opacity groups.
    ///
    /// # Panics
    /// Panics if the assertions fail.
    #[test]
    fn compute_lifetimes_with_opacity() {
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
        let lifetimes = compute_lifetimes(&graph);

        assert_eq!(lifetimes.len(), 1);
        assert!(
            lifetimes.contains_key(&ResourceId(0)),
            "Expected lifetime for ResourceId(0)"
        );
        let lifetime = &lifetimes[&ResourceId(0)];
        assert_eq!(lifetime.created_at.0, 1); // First offscreen pass
        assert!(lifetime.last_used_at.0 >= lifetime.created_at.0);
    }

    /// Test alias grouping with non-overlapping lifetimes.
    ///
    /// # Panics
    /// Panics if the assertions fail.
    #[test]
    fn alias_groups_non_overlapping() {
        // Create two opacity groups that don't overlap in time
        let mut display_list = DisplayList::new();

        // First opacity group
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

        // Second opacity group (non-overlapping)
        display_list.items.push(DisplayItem::BeginStackingContext {
            boundary: StackingContextBoundary::Opacity { alpha: 0.8 },
        });
        display_list.push(DisplayItem::Rect {
            x: 100.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
            color: [0.0, 1.0, 0.0, 1.0],
        });
        display_list.items.push(DisplayItem::EndStackingContext);

        let compositor = OpacityCompositor::collect_from_display_list(&display_list);
        let graph = RenderGraph::build_from_display_list(&display_list, compositor.groups(), true);
        let lifetimes = compute_lifetimes(&graph);
        let groups = compute_alias_groups(&graph, &lifetimes);

        // Both resources can share one texture (non-overlapping lifetimes)
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].resources.len(), 2);
    }
}
