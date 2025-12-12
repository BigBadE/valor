//! Layout context with automatic dependency tracking.
//!
//! The `LayoutContext` wraps all reads during layout computation, automatically
//! recording dependencies. When a layout algorithm reads a style property or
//! child size, the context records that dependency.

use crate::core::dependencies::{Dependency, DependencyTracker, PropertyId};
use crate::core::style_interning::StyleInterner;
use css::style_types::ComputedStyle;
use css_core::LayoutRect;
use js::NodeKey;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Result of a layout computation
#[derive(Debug, Clone)]
pub struct LayoutResult {
    pub inline_size: f32,
    pub block_size: f32,
    pub bfc_offset: BfcOffset,
    pub baseline: Option<f32>,
}

/// Block Formatting Context offset
#[derive(Debug, Clone, Copy, Default)]
pub struct BfcOffset {
    pub inline_offset: f32,
    pub block_offset: f32,
}

impl BfcOffset {
    pub fn new(inline: f32, block: f32) -> Self {
        Self {
            inline_offset: inline,
            block_offset: block,
        }
    }
}

/// Viewport dimensions for culling
#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
}

/// Context for layout computation with automatic dependency tracking
pub struct LayoutContext<'interner> {
    /// Dependency tracker
    tracker: DependencyTracker,

    /// Style interner (read-only reference)
    style_interner: &'interner StyleInterner,

    /// Cached layout results
    layout_cache: HashMap<NodeKey, LayoutResult>,

    /// Parent-child relationships
    children: HashMap<NodeKey, Vec<NodeKey>>,

    /// Viewport for intersection testing
    viewport: Viewport,

    /// Currently computing node (for recording dependencies)
    current_node: Option<NodeKey>,
}

impl<'interner> LayoutContext<'interner> {
    pub fn new(style_interner: &'interner StyleInterner, viewport: Viewport) -> Self {
        Self {
            tracker: DependencyTracker::new(),
            style_interner,
            layout_cache: HashMap::new(),
            children: HashMap::new(),
            viewport,
            current_node: None,
        }
    }

    /// Start computing layout for a node
    pub fn begin_layout(&mut self, node: NodeKey) {
        self.current_node = Some(node);
        self.tracker.start();
    }

    /// Finish computing layout and return recorded dependencies
    pub fn end_layout(&mut self) -> HashSet<Dependency> {
        self.current_node = None;
        self.tracker.finish()
    }

    /// Read a style property (automatically records dependency)
    pub fn read_style_property(
        &mut self,
        node: NodeKey,
        property: PropertyId,
    ) -> Option<StylePropertyValue> {
        // Record dependency
        self.tracker
            .record(Dependency::StyleProperty(node, property));

        // Get style and extract property
        let handle = self.style_interner.get_node_style(node)?;
        let style = self.style_interner.get(handle)?;

        Some(extract_property(style, property))
    }

    /// Read style handle for a node
    pub fn read_style(&mut self, node: NodeKey) -> Option<&Arc<ComputedStyle>> {
        let handle = self.style_interner.get_node_style(node)?;
        self.style_interner.get(handle)
    }

    /// Get full `ComputedStyle` (records dependency on all properties read from it)
    pub fn get_computed_style(&mut self, node: NodeKey) -> Option<&Arc<ComputedStyle>> {
        self.read_style(node)
    }

    /// Read parent's content size (automatically records dependency)
    pub fn read_parent_size(&mut self, parent: NodeKey) -> Option<(f32, f32)> {
        self.tracker.record(Dependency::ParentSize(parent));

        let result = self.layout_cache.get(&parent)?;
        Some((result.inline_size, result.block_size))
    }

    /// Read child's size (automatically records dependency)
    pub fn read_child_size(&mut self, parent: NodeKey, child_index: usize) -> Option<(f32, f32)> {
        self.tracker
            .record(Dependency::ChildSize(parent, child_index));

        let children = self.children.get(&parent)?;
        let child = children.get(child_index)?;

        let result = self.layout_cache.get(child)?;
        Some((result.inline_size, result.block_size))
    }

    /// Read all children sizes (automatically records dependency)
    pub fn read_all_children_sizes(&mut self, parent: NodeKey) -> Vec<(f32, f32)> {
        self.tracker.record(Dependency::AllChildrenSizes(parent));

        let Some(children) = self.children.get(&parent) else {
            return Vec::new();
        };

        children
            .iter()
            .filter_map(|&child| {
                let result = self.layout_cache.get(&child)?;
                Some((result.inline_size, result.block_size))
            })
            .collect()
    }

    /// Get children of a node
    pub fn get_children(&self, node: NodeKey) -> Option<&[NodeKey]> {
        self.children.get(&node).map(Vec::as_slice)
    }

    /// Set children for a node
    pub fn set_children(&mut self, node: NodeKey, children: Vec<NodeKey>) {
        self.children.insert(node, children);
    }

    /// Store layout result
    pub fn store_result(&mut self, node: NodeKey, result: LayoutResult) {
        self.layout_cache.insert(node, result);
    }

    /// Get cached layout result without recording dependency
    pub fn get_cached_result(&self, node: NodeKey) -> Option<&LayoutResult> {
        self.layout_cache.get(&node)
    }

    /// Check if node intersects viewport (records viewport dependency)
    pub fn intersects_viewport(&mut self, node: NodeKey) -> bool {
        self.tracker.record(Dependency::ViewportSize);

        let Some(result) = self.layout_cache.get(&node) else {
            return true; // Conservative: assume visible if not laid out
        };

        let node_rect = LayoutRect {
            x: result.bfc_offset.inline_offset,
            y: result.bfc_offset.block_offset,
            width: result.inline_size,
            height: result.block_size,
        };

        // Simple intersection test
        let viewport_rect = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: self.viewport.width,
            height: self.viewport.height,
        };

        intersects(&node_rect, &viewport_rect)
    }

    /// Get viewport dimensions
    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    /// Clear cache for a node
    pub fn invalidate_node(&mut self, node: NodeKey) {
        self.layout_cache.remove(&node);
    }
}

/// Extract a specific property value from `ComputedStyle`
#[derive(Debug, Clone)]
pub enum StylePropertyValue {
    Length(f32),
    Auto,
    Percentage(f32),
    Display(String),
    Position(String),
    Overflow(String),
}

fn extract_property(style: &ComputedStyle, property: PropertyId) -> StylePropertyValue {
    // Simplified extraction - real implementation would handle all property types
    match property {
        PropertyId::DISPLAY => StylePropertyValue::Display(format!("{:?}", style.display)),
        PropertyId::POSITION => StylePropertyValue::Position("static".to_string()),
        PropertyId::OVERFLOW => StylePropertyValue::Overflow("visible".to_string()),
        _ => StylePropertyValue::Auto,
    }
}

fn intersects(rect_a: &LayoutRect, rect_b: &LayoutRect) -> bool {
    !(rect_a.x + rect_a.width < rect_b.x
        || rect_b.x + rect_b.width < rect_a.x
        || rect_a.y + rect_a.height < rect_b.y
        || rect_b.y + rect_b.height < rect_a.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test dependency tracking.
    ///
    /// # Panics
    ///
    /// Panics if dependency tracking does not work as expected.
    #[test]
    fn test_dependency_tracking() {
        let interner = StyleInterner::new();
        let viewport = Viewport {
            width: 1024.0,
            height: 768.0,
        };
        let mut ctx = LayoutContext::new(&interner, viewport);

        let node = NodeKey::ROOT;
        let parent = NodeKey::ROOT;

        ctx.begin_layout(node);
        ctx.read_style_property(node, PropertyId::WIDTH);
        ctx.read_parent_size(parent);
        let deps = ctx.end_layout();

        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&Dependency::StyleProperty(node, PropertyId::WIDTH)));
        assert!(deps.contains(&Dependency::ParentSize(parent)));
    }
}
