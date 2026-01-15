//! Layout database with query-based computation and parallel execution.
//!
//! This module provides a high-level interface for layout computation
//! using the valor_query system with parallel execution support for
//! independent formatting contexts.

use crate::LayoutUnit;
use crate::queries::layout_queries::ViewportInput;
use crate::queries::{FormattingContextQuery, FormattingContextType, LayoutResultQuery};
use anyhow::Result;
use css_orchestrator::queries::{DomChildrenInput, DomParentInput};
use js::{DOMUpdate, NodeKey};
use log::trace;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use valor_query::{QueryDatabase, parallel::ParallelRuntime};

/// Layout computation database with parallel execution by formatting context.
pub struct LayoutDatabase {
    /// Core query database
    db: Arc<QueryDatabase>,

    /// Parallel runtime for executing queries
    runtime: Arc<ParallelRuntime>,

    /// Track which nodes exist
    nodes: HashSet<NodeKey>,

    /// Track dirty nodes that need layout recomputation
    dirty_nodes: HashSet<NodeKey>,

    /// Root node (usually <html> element)
    root: Option<NodeKey>,

    /// Viewport dimensions
    viewport_width: f32,
    viewport_height: f32,

    /// Cached layout results (computed once per recompute cycle)
    layout_cache: HashMap<NodeKey, crate::queries::layout_queries::LayoutResult>,
}

impl LayoutDatabase {
    /// Create a new layout database.
    ///
    /// # Errors
    ///
    /// Returns an error if the parallel runtime cannot be created.
    pub fn new(viewport_width: f32, viewport_height: f32) -> Result<Self> {
        let db = Arc::new(QueryDatabase::new());
        let runtime = Arc::new(ParallelRuntime::new(None)?);

        // Set initial viewport dimensions
        let viewport = (
            LayoutUnit::from_px(viewport_width),
            LayoutUnit::from_px(viewport_height),
        );
        db.set_input::<ViewportInput>((), viewport);

        Ok(Self {
            db,
            runtime,
            nodes: HashSet::new(),
            dirty_nodes: HashSet::new(),
            root: None,
            viewport_width,
            viewport_height,
            layout_cache: HashMap::new(),
        })
    }

    /// Create a layout database that shares a QueryDatabase with StyleDatabase.
    ///
    /// This is the preferred way to create a LayoutDatabase, as it ensures
    /// that layout queries can access DOM structure populated by style queries.
    ///
    /// # Errors
    ///
    /// Returns an error if the parallel runtime cannot be created.
    pub fn new_shared(
        shared_db: Arc<QueryDatabase>,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Result<Self> {
        let runtime = Arc::new(ParallelRuntime::new(None)?);

        // Set initial viewport dimensions
        let viewport = (
            LayoutUnit::from_px(viewport_width),
            LayoutUnit::from_px(viewport_height),
        );
        shared_db.set_input::<ViewportInput>((), viewport);

        Ok(Self {
            db: shared_db,
            runtime,
            nodes: HashSet::new(),
            dirty_nodes: HashSet::new(),
            root: None,
            viewport_width,
            viewport_height,
            layout_cache: HashMap::new(),
        })
    }

    /// Create a layout database with shared QueryDatabase and ParallelRuntime.
    ///
    /// This avoids creating multiple rayon thread pools, which can cause hangs.
    pub fn new_shared_with_runtime(
        shared_db: Arc<QueryDatabase>,
        runtime: Arc<ParallelRuntime>,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Result<Self> {
        // Set initial viewport dimensions
        let viewport = (
            LayoutUnit::from_px(viewport_width),
            LayoutUnit::from_px(viewport_height),
        );
        shared_db.set_input::<ViewportInput>((), viewport);

        Ok(Self {
            db: shared_db,
            runtime,
            nodes: HashSet::new(),
            dirty_nodes: HashSet::new(),
            root: None,
            viewport_width,
            viewport_height,
            layout_cache: HashMap::new(),
        })
    }

    /// Apply a DOM update to track nodes and dirty state.
    ///
    /// Note: This does NOT populate DOM inputs (DomParentInput, DomChildrenInput)
    /// because those are already populated by StyleDatabase in the shared QueryDatabase.
    /// This method only tracks which nodes exist and which need layout recomputation.
    pub fn apply_update(&mut self, update: DOMUpdate) {
        match update {
            DOMUpdate::InsertElement { node, parent, .. } => {
                self.nodes.insert(node);
                self.dirty_nodes.insert(node);

                // Set root if this is html element under ROOT
                if parent == NodeKey::ROOT && self.root.is_none() {
                    self.root = Some(node);
                }
            }
            DOMUpdate::InsertText { node, .. } => {
                self.nodes.insert(node);
                self.dirty_nodes.insert(node);
            }
            DOMUpdate::RemoveNode { node } => {
                self.nodes.remove(&node);
                self.dirty_nodes.remove(&node);
            }
            DOMUpdate::SetAttr { node, .. } => {
                // Attribute changes might affect layout
                self.dirty_nodes.insert(node);
            }
            DOMUpdate::UpdateText { node, .. } => {
                // Text content changes affect layout
                self.dirty_nodes.insert(node);
            }
            DOMUpdate::EndOfDocument => {}
        }
    }

    /// Mark a node as dirty for layout recomputation.
    ///
    /// Note: Computed styles come from ComputedStyleQuery in the shared QueryDatabase,
    /// so we don't need to set them as inputs.
    pub fn mark_dirty(&mut self, node: NodeKey) {
        self.dirty_nodes.insert(node);
    }

    /// Set viewport dimensions.
    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport_width = width;
        self.viewport_height = height;

        let viewport = (LayoutUnit::from_px(width), LayoutUnit::from_px(height));
        self.db.set_input::<ViewportInput>((), viewport);

        // Viewport change affects all layouts
        self.dirty_nodes.extend(self.nodes.iter());
    }

    /// Group nodes by their tree depth for wave-based processing.
    fn group_by_depth(&self, nodes: &HashSet<NodeKey>) -> Vec<Vec<NodeKey>> {
        let mut depth_map: HashMap<usize, Vec<NodeKey>> = HashMap::new();

        for &node in nodes {
            let depth = self.compute_depth(node);
            depth_map.entry(depth).or_default().push(node);
        }

        // Convert to sorted vec
        let mut depths: Vec<_> = depth_map.into_iter().collect();
        depths.sort_by_key(|(depth, _)| *depth);
        depths.into_iter().map(|(_, nodes)| nodes).collect()
    }

    /// Compute the depth of a node in the tree.
    fn compute_depth(&self, node: NodeKey) -> usize {
        let mut depth = 0;
        let mut current = node;

        loop {
            let parent_opt = self.db.input::<DomParentInput>(current);
            match *parent_opt {
                Some(parent) if parent != NodeKey::ROOT => {
                    depth += 1;
                    current = parent;
                }
                _ => break,
            }
        }

        depth
    }

    /// Build the formatting context tree for parallel execution.
    fn build_fc_tree(&self) -> Vec<Vec<NodeKey>> {
        let mut fc_roots = Vec::new();

        // Find all formatting context roots
        for &node in &self.nodes {
            let fc_type = self.db.query::<FormattingContextQuery>(node);
            if !matches!(*fc_type, FormattingContextType::None) {
                fc_roots.push(node);
            }
        }

        // Group by depth - shallower FCs must complete before deeper ones
        let mut depth_map: HashMap<usize, Vec<NodeKey>> = HashMap::new();
        for node in fc_roots {
            let depth = self.compute_depth(node);
            depth_map.entry(depth).or_default().push(node);
        }

        // Convert to sorted waves
        let mut waves: Vec<_> = depth_map.into_iter().collect();
        waves.sort_by_key(|(depth, _)| *depth);
        waves.into_iter().map(|(_, nodes)| nodes).collect()
    }

    /// Compute layouts for all dirty nodes using per-node query-based layout.
    ///
    /// This uses the new incremental query system where each node's layout
    /// is computed via LayoutResultQuery, which recursively queries children.
    pub fn recompute_layouts_parallel(&mut self) -> bool {
        if self.root.is_none() {
            log::warn!("LayoutDatabase: No root node set, skipping layout");
            return false;
        }

        let root = self.root.unwrap();

        log::info!(
            "LayoutDatabase: Computing layouts for root {:?}, dirty_nodes={}, total_nodes={}",
            root,
            self.dirty_nodes.len(),
            self.nodes.len()
        );

        // Query the root layout - this will recursively query all children
        let root_result = self.db.query::<LayoutResultQuery>(root);

        log::info!(
            "LayoutDatabase: Root layout computed: width={}, height={}",
            root_result.inline_size,
            root_result.block_size
        );

        // Cache the root result
        self.layout_cache.clear();
        self.layout_cache.insert(root, (*root_result).clone());

        // Recursively cache all descendants
        self.cache_descendant_layouts(root);

        // Clear dirty nodes after successful layout
        self.dirty_nodes.clear();

        true
    }

    /// Recursively cache layout results for all descendants of a node.
    fn cache_descendant_layouts(&mut self, node: NodeKey) {
        let children_arc: Arc<Vec<NodeKey>> = self.db.input::<DomChildrenInput>(node);

        log::trace!(
            "cache_descendant_layouts: node={:?} has {} children in database",
            node,
            children_arc.len()
        );

        for &child in children_arc.iter() {
            // Query and cache child layout
            let child_result = self.db.query::<LayoutResultQuery>(child);
            self.layout_cache.insert(child, (*child_result).clone());

            // Recursively cache grandchildren
            self.cache_descendant_layouts(child);
        }
    }

    /// Get layout result for a node.
    pub fn get_layout(
        &self,
        node: NodeKey,
    ) -> Option<crate::queries::layout_queries::LayoutResult> {
        if !self.nodes.contains(&node) {
            return None;
        }

        let result = self.db.query::<LayoutResultQuery>(node);
        Some((*result).clone())
    }

    /// Get all layout results from cache.
    pub fn get_all_layouts(
        &self,
    ) -> HashMap<NodeKey, crate::queries::layout_queries::LayoutResult> {
        self.layout_cache.clone()
    }
}
