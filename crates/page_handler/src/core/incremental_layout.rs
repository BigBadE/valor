//! Incremental layout engine with fine-grained dependency tracking.
//!
//! This engine:
//! - Automatically tracks what each layout depends on
//! - Only recomputes layouts when dependencies change
//! - Supports style → layout → paint cascade
//! - Culls off-screen content
//! - Parallelizes independent subtrees

use crate::core::dependencies::{Dependency, DependencyGraph, PropertyId};
use crate::core::layout_context::Viewport;
use crate::core::style_interning::{StyleInterner, StyleInternerStats};
use crate::utilities::snapshots::LayoutNodeKind;
use anyhow::Result;
use css::style_types::ComputedStyle;
use css_core::{LayoutDatabase, LayoutRect, LayoutUnit};
use js::{DOMUpdate, NodeKey};
// TODO: Re-enable when parallelization is implemented
// use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

/// Snapshot of the layout tree structure: (node, kind, children).
pub type LayoutTreeSnapshot = Vec<(NodeKey, LayoutNodeKind, Vec<NodeKey>)>;

/// The incremental layout engine
pub struct IncrementalLayoutEngine {
    /// Style interner for memory efficiency
    style_interner: StyleInterner,

    /// Dependency graph tracking what depends on what
    dependency_graph: DependencyGraph,

    /// Nodes that need layout computation
    dirty_nodes: HashSet<NodeKey>,

    /// Viewport dimensions
    viewport: Viewport,

    /// Generation counter for cache invalidation
    generation: u64,

    /// DOM structure data for layout
    children: HashMap<NodeKey, Vec<NodeKey>>,
    tags: HashMap<NodeKey, String>,
    text_nodes: HashMap<NodeKey, String>,
    attrs: HashMap<NodeKey, HashMap<String, String>>,
    root: Option<NodeKey>,

    /// Cached layout results
    layout_cache: HashMap<NodeKey, LayoutRect>,

    /// Parallel layout database (Phase 4)
    layout_database: Option<LayoutDatabase>,
}

impl IncrementalLayoutEngine {
    pub fn new(viewport_width: f32, viewport_height: f32) -> Self {
        // Try to create parallel layout database
        let layout_database = LayoutDatabase::new(viewport_width, viewport_height).ok();

        Self {
            style_interner: StyleInterner::new(),
            dependency_graph: DependencyGraph::new(),
            dirty_nodes: HashSet::new(),
            viewport: Viewport {
                width: viewport_width,
                height: viewport_height,
            },
            generation: 0,
            children: HashMap::new(),
            tags: HashMap::new(),
            text_nodes: HashMap::new(),
            attrs: HashMap::new(),
            root: None,
            layout_cache: HashMap::new(),
            layout_database,
        }
    }

    /// Create a new layout engine that shares the QueryDatabase with StyleDatabase.
    ///
    /// This ensures that layout queries can access DOM structure populated by style queries.
    pub fn new_shared(
        shared_db: std::sync::Arc<valor_query::QueryDatabase>,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Self {
        // Create layout database with shared QueryDatabase
        let layout_database =
            LayoutDatabase::new_shared(shared_db, viewport_width, viewport_height).ok();

        Self {
            style_interner: StyleInterner::new(),
            dependency_graph: DependencyGraph::new(),
            dirty_nodes: HashSet::new(),
            viewport: Viewport {
                width: viewport_width,
                height: viewport_height,
            },
            generation: 0,
            children: HashMap::new(),
            tags: HashMap::new(),
            text_nodes: HashMap::new(),
            attrs: HashMap::new(),
            root: None,
            layout_cache: HashMap::new(),
            layout_database,
        }
    }

    /// Create a new layout engine with shared QueryDatabase and ParallelRuntime.
    ///
    /// This avoids creating multiple rayon thread pools, which can cause hangs.
    pub fn new_shared_with_runtime(
        shared_db: std::sync::Arc<valor_query::QueryDatabase>,
        shared_runtime: std::sync::Arc<valor_query::parallel::ParallelRuntime>,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Self {
        // Create layout database with shared QueryDatabase and runtime
        let layout_database = match LayoutDatabase::new_shared_with_runtime(
            shared_db,
            shared_runtime,
            viewport_width,
            viewport_height,
        ) {
            Ok(db) => {
                log::info!("LayoutDatabase created successfully");
                Some(db)
            }
            Err(e) => {
                log::error!("Failed to create LayoutDatabase: {}", e);
                None
            }
        };

        Self {
            style_interner: StyleInterner::new(),
            dependency_graph: DependencyGraph::new(),
            dirty_nodes: HashSet::new(),
            viewport: Viewport {
                width: viewport_width,
                height: viewport_height,
            },
            generation: 0,
            children: HashMap::new(),
            tags: HashMap::new(),
            text_nodes: HashMap::new(),
            attrs: HashMap::new(),
            root: None,
            layout_cache: HashMap::new(),
            layout_database,
        }
    }

    /// Check if there are dirty nodes requiring layout
    pub fn has_dirty_nodes(&self) -> bool {
        !self.dirty_nodes.is_empty()
    }

    /// Apply a style change and invalidate affected layouts
    pub fn apply_style_change(&mut self, node: NodeKey, new_style: &ComputedStyle) {
        // Get old style handle and clone the old style (to avoid borrow checker issues)
        let old_handle = self.style_interner.get_node_style(node);
        let old_style_clone = old_handle
            .and_then(|style_handle| self.style_interner.get(style_handle))
            .map(|arc| arc.as_ref().clone());

        // Intern new style
        self.style_interner.set_node_style(node, new_style.clone());

        // Check if style actually changed by comparing values, not just handles
        // This ensures we invalidate even if the style interner reuses a handle
        let changed = old_style_clone.as_ref() != Some(new_style);

        if changed {
            self.invalidate_node(node);

            // Mark node as dirty in parallel layout database
            // Note: Styles are accessed via ComputedStyleQuery, not inputs
            if let Some(layout_db) = &mut self.layout_database {
                layout_db.mark_dirty(node);
            }
        }
    }

    /// Apply DOM updates
    pub fn apply_dom_updates(&mut self, updates: &[DOMUpdate]) {
        log::info!(
            "apply_dom_updates: Applying {} updates, dirty_nodes before: {}",
            updates.len(),
            self.dirty_nodes.len()
        );

        // Apply to parallel layout database for node tracking
        // Note: LayoutDatabase.apply_update only tracks nodes, it doesn't populate DOM inputs
        // since those are already populated by StyleDatabase in the shared QueryDatabase
        if let Some(layout_db) = &mut self.layout_database {
            for update in updates {
                layout_db.apply_update(update.clone());
            }
        }

        for update in updates {
            match update {
                DOMUpdate::InsertElement {
                    node, parent, tag, ..
                } => {
                    // Track element structure
                    self.tags.insert(*node, tag.clone());
                    self.children.entry(*parent).or_default().push(*node);
                    // Set root to the html element, or first non-style/script element as fallback
                    if *parent == NodeKey::ROOT && tag == "html" {
                        // Always use html as root if present
                        self.root = Some(*node);
                    } else if *parent == NodeKey::ROOT
                        && self.root.is_none()
                        && tag != "style"
                        && tag != "script"
                    {
                        // Fallback for documents without <html> wrapper
                        self.root = Some(*node);
                    }
                    // New element needs layout
                    self.dirty_nodes.insert(*node);
                }
                DOMUpdate::InsertText {
                    node, parent, text, ..
                } => {
                    // Track text content
                    self.text_nodes.insert(*node, text.clone());
                    self.children.entry(*parent).or_default().push(*node);
                    // New text node needs layout
                    self.dirty_nodes.insert(*node);
                    self.invalidate_dependency(&Dependency::TextContent(*node));
                }
                DOMUpdate::RemoveNode { node } => {
                    // Remove from all tracking
                    self.style_interner.remove_node(*node);
                    self.dependency_graph.remove_node(*node);
                    self.dirty_nodes.remove(node);
                    self.tags.remove(node);
                    self.text_nodes.remove(node);
                    self.attrs.remove(node);
                    self.children.remove(node);
                    self.layout_cache.remove(node);
                }
                DOMUpdate::SetAttr {
                    node, name, value, ..
                } => {
                    // Track attributes
                    self.attrs
                        .entry(*node)
                        .or_default()
                        .insert(name.clone(), value.clone());
                    // Attribute changes might affect style
                    // Conservative: invalidate this node
                    self.invalidate_node(*node);
                }
                DOMUpdate::UpdateText { node, text } => {
                    // Update existing text node content in-place
                    self.text_nodes.insert(*node, text.clone());
                    // Text content changed, so this node needs layout
                    self.dirty_nodes.insert(*node);
                    self.invalidate_dependency(&Dependency::TextContent(*node));
                }
                DOMUpdate::EndOfDocument => {}
            }
        }
        log::info!(
            "apply_dom_updates: Done, dirty_nodes after: {}",
            self.dirty_nodes.len()
        );
    }

    /// Invalidate a specific node
    fn invalidate_node(&mut self, node: NodeKey) {
        self.dirty_nodes.insert(node);

        // Invalidate all nodes that depend on this node's properties
        // This is conservative - could be more precise with property-level tracking
        let style_deps: Vec<_> = (0..33)
            .map(|id| Dependency::StyleProperty(node, PropertyId(id)))
            .collect();

        for dep in style_deps {
            let affected = self.dependency_graph.invalidate(&dep);
            self.dirty_nodes.extend(affected);
        }
    }

    /// Invalidate a specific dependency
    fn invalidate_dependency(&mut self, dep: &Dependency) {
        let affected = self.dependency_graph.invalidate(dep);
        self.dirty_nodes.extend(affected);
    }

    /// Compute layouts using parallel query-based layout engine.
    ///
    /// # Errors
    /// Returns an error if layout computation fails.
    pub fn compute_layouts(&mut self) -> Result<HashMap<NodeKey, LayoutRect>> {
        if self.root.is_none() {
            return Ok(HashMap::new());
        }

        self.generation += 1;
        log::info!("RUNNING PARALLEL LAYOUT - generation {}", self.generation);

        // Use parallel layout database if available
        if let Some(layout_db) = &mut self.layout_database {
            let _executed = layout_db.recompute_layouts_parallel();

            // Get all layout results from query database
            let all_layouts = layout_db.get_all_layouts();

            // Convert query results to LayoutRect and update cache
            for (node, result) in all_layouts {
                let base_y = result
                    .bfc_offset
                    .block_offset
                    .unwrap_or(LayoutUnit::zero())
                    .to_px();

                // Use the full line-height for layout positioning
                // Half-leading is a rendering concept and should not affect layout
                let final_y = base_y;
                let final_height = result.block_size;

                let rect = LayoutRect {
                    x: result.bfc_offset.inline_offset.to_px(),
                    y: final_y,
                    width: result.inline_size,
                    height: final_height,
                };
                self.layout_cache.insert(node, rect);
            }

            log::info!(
                "PARALLEL LAYOUT COMPLETE - generation {}, total results: {}",
                self.generation,
                self.layout_cache.len()
            );
        } else {
            log::warn!("Parallel layout database not available, returning empty results");
        }

        // Clear dirty set
        self.dirty_nodes.clear();

        Ok(self.layout_cache.clone())
    }

    /// Get dirty node count
    pub fn dirty_count(&self) -> usize {
        self.dirty_nodes.len()
    }

    /// Get style interner stats
    pub fn style_stats(&self) -> StyleInternerStats {
        self.style_interner.stats()
    }

    /// Get layout rects (for compatibility with `LayoutManager`)
    pub fn rects(&self) -> &HashMap<NodeKey, LayoutRect> {
        &self.layout_cache
    }

    /// Get attrs map (for compatibility with `LayoutManager`)
    pub fn attrs_map(&self) -> &HashMap<NodeKey, HashMap<String, String>> {
        &self.attrs
    }

    /// Get snapshot of layout tree structure (for compatibility with `LayoutManager`)
    pub fn snapshot(&self) -> LayoutTreeSnapshot {
        let mut result = Vec::new();

        if let Some(root_key) = self.root {
            self.build_snapshot_recursive(root_key, &mut result);
        }

        log::info!(
            "snapshot() returning {} nodes, root={:?}, tags.len()={}, children.len()={}",
            result.len(),
            self.root,
            self.tags.len(),
            self.children.len()
        );

        result
    }

    fn build_snapshot_recursive(&self, node: NodeKey, result: &mut LayoutTreeSnapshot) {
        let children = self.children.get(&node).cloned().unwrap_or_default();

        let kind = self.tags.get(&node).map_or_else(
            || {
                self.text_nodes.get(&node).map_or_else(
                    || {
                        if node == NodeKey::ROOT {
                            LayoutNodeKind::Document
                        } else {
                            LayoutNodeKind::Block {
                                tag: String::from("div"),
                            }
                        }
                    },
                    |text| LayoutNodeKind::InlineText { text: text.clone() },
                )
            },
            |tag| LayoutNodeKind::Block { tag: tag.clone() },
        );

        result.push((node, kind, children.clone()));

        for child in children {
            self.build_snapshot_recursive(child, result);
        }
    }

    /// Set viewport dimensions (for compatibility with `LayoutManager`)
    pub fn set_viewport(&mut self, width: i32, height: i32) {
        let new_width = width as f32;
        let new_height = height as f32;

        // If viewport changed, invalidate layout cache to force recomputation
        if (self.viewport.width - new_width).abs() > 0.1
            || (self.viewport.height - new_height).abs() > 0.1
        {
            self.layout_cache.clear();

            // Update parallel layout database viewport
            if let Some(layout_db) = &mut self.layout_database {
                layout_db.set_viewport(new_width, new_height);
            }
        }

        self.viewport.width = new_width;
        self.viewport.height = new_height;
    }

    /// Set computed styles (for compatibility with `LayoutManager`)
    pub fn set_computed_styles(&mut self, styles: &HashMap<NodeKey, ComputedStyle>) {
        for (node, style) in styles {
            self.apply_style_change(*node, style);
        }
    }

    /// Get computed styles (for compatibility with `LayoutManager`)
    pub fn computed_styles(&self) -> HashMap<NodeKey, ComputedStyle> {
        let mut styles = HashMap::new();
        for (node, handle) in self.style_interner.node_styles_iter() {
            if let Some(style) = self.style_interner.get(*handle) {
                styles.insert(*node, (**style).clone());
            }
        }
        styles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test incremental layout.
    ///
    /// # Panics
    ///
    /// Panics if incremental layout does not work as expected.
    #[test]
    fn test_incremental_layout() {
        let mut engine = IncrementalLayoutEngine::new(1024.0, 768.0);

        let node = NodeKey::pack(0, 0, 1);
        let style = ComputedStyle::default();

        // First insert the element via DOM update
        engine.apply_dom_updates(&[DOMUpdate::InsertElement {
            parent: NodeKey::ROOT,
            node,
            tag: String::from("div"),
            pos: 0,
        }]);

        // Apply style
        engine.apply_style_change(node, &style);

        // Node should be dirty
        assert_eq!(engine.dirty_nodes.len(), 1);

        // Compute layouts - use unwrap_or_default to handle errors gracefully in tests
        let results = engine.compute_layouts().unwrap_or_default();

        // Should have computed one layout
        assert_eq!(results.len(), 1);

        // Node should no longer be dirty
        assert_eq!(engine.dirty_count(), 0);
    }

    /// Test invalidation.
    ///
    /// # Panics
    ///
    /// Panics if invalidation does not work as expected.
    #[test]
    fn test_invalidation() {
        let mut engine = IncrementalLayoutEngine::new(1024.0, 768.0);

        let node_a = NodeKey::ROOT;
        let node_b = NodeKey::ROOT;

        // Set up styles
        engine.apply_style_change(node_a, &ComputedStyle::default());
        engine.apply_style_change(node_b, &ComputedStyle::default());

        // Compute initial layouts - errors are OK in tests, they'll fail later assertions
        let _result = engine.compute_layouts().ok();

        // Change node_a's style
        let new_style = ComputedStyle::default();
        engine.apply_style_change(node_a, &new_style);

        // node_a should be dirty
        assert!(!engine.dirty_nodes.is_empty());
    }
}
