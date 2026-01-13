//! Style database with query-based computation.
//!
//! This module provides a high-level interface for style computation
//! using the valor_query system with parallel execution support.

use crate::queries::{
    ComputedStyleQuery, DomAttributesInput, DomChildrenInput, DomClassesInput, DomIdInput,
    DomParentInput, DomTagInput, DomTextInput, StylesheetInput,
};
use crate::style_model::ComputedStyle;
use crate::types::Stylesheet;
use anyhow::Result;
use js::{DOMUpdate, NodeKey};
use log::trace;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use valor_query::{QueryDatabase, parallel::ParallelRuntime};

/// Style computation database with parallel execution.
pub struct StyleDatabase {
    /// Core query database
    db: Arc<QueryDatabase>,

    /// Parallel runtime for executing queries
    runtime: Arc<ParallelRuntime>,

    /// Track which nodes exist
    nodes: HashSet<NodeKey>,

    /// Track dirty nodes that need style recomputation
    dirty_nodes: HashSet<NodeKey>,
}

impl StyleDatabase {
    /// Create a new style database.
    ///
    /// # Errors
    ///
    /// Returns an error if the parallel runtime cannot be created.
    pub fn new() -> Result<Self> {
        let db = Arc::new(QueryDatabase::new());
        let runtime = Arc::new(ParallelRuntime::new(None)?);

        Ok(Self {
            db,
            runtime,
            nodes: HashSet::new(),
            dirty_nodes: HashSet::new(),
        })
    }

    /// Create a new style database with a shared parallel runtime.
    ///
    /// This avoids creating multiple rayon thread pools.
    pub fn new_with_runtime(runtime: Arc<ParallelRuntime>) -> Result<Self> {
        let db = Arc::new(QueryDatabase::new());

        Ok(Self {
            db,
            runtime,
            nodes: HashSet::new(),
            dirty_nodes: HashSet::new(),
        })
    }

    /// Apply a DOM update to the style database.
    pub fn apply_update(&mut self, update: DOMUpdate) {
        match update {
            DOMUpdate::InsertElement {
                node, tag, parent, ..
            } => {
                self.nodes.insert(node);
                self.dirty_nodes.insert(node);

                // Set input data
                self.db.set_input::<DomTagInput>(node, tag);
                self.db.set_input::<DomParentInput>(node, Some(parent));

                // Update parent's children
                let mut children = (*self.db.input::<DomChildrenInput>(parent)).clone();
                children.push(node);
                self.db.set_input::<DomChildrenInput>(parent, children);
            }

            DOMUpdate::InsertText {
                node, text, parent, ..
            } => {
                self.nodes.insert(node);
                self.db.set_input::<DomTextInput>(node, Some(text));
                self.db.set_input::<DomParentInput>(node, Some(parent));

                let mut children = (*self.db.input::<DomChildrenInput>(parent)).clone();
                children.push(node);
                self.db.set_input::<DomChildrenInput>(parent, children);
            }

            DOMUpdate::SetAttr { node, name, value } => {
                self.dirty_nodes.insert(node);

                let mut attrs = (*self.db.input::<DomAttributesInput>(node)).clone();
                attrs.insert(name.clone(), value.clone());
                self.db.set_input::<DomAttributesInput>(node, attrs);

                // Handle special attributes
                if name == "id" {
                    self.db.set_input::<DomIdInput>(node, Some(value));
                } else if name == "class" {
                    let classes: Vec<String> = value.split_whitespace().map(String::from).collect();
                    self.db.set_input::<DomClassesInput>(node, classes);
                }
            }

            DOMUpdate::RemoveNode { node } => {
                self.nodes.remove(&node);
                self.dirty_nodes.remove(&node);
                // Inputs will be garbage collected when no queries reference them
            }

            DOMUpdate::UpdateText { node, text } => {
                self.db.set_input::<DomTextInput>(node, Some(text));
            }

            DOMUpdate::EndOfDocument => {}
        }
    }

    /// Replace the active stylesheet.
    pub fn replace_stylesheet(&mut self, sheet: Stylesheet) {
        self.db.set_input::<StylesheetInput>((), sheet);
        // Mark all nodes dirty since stylesheet changed
        self.dirty_nodes.extend(self.nodes.iter());
    }

    /// Compute styles for all dirty nodes in parallel.
    ///
    /// Returns true if any styles changed.
    pub fn recompute_styles_parallel(&mut self) -> bool {
        if self.dirty_nodes.is_empty() {
            log::info!("StyleDatabase: No dirty nodes, skipping style recomputation");
            return false;
        }

        log::info!(
            "StyleDatabase: Recomputing styles for {} dirty nodes out of {} total nodes",
            self.dirty_nodes.len(),
            self.nodes.len()
        );

        // Group dirty nodes by depth for parallel waves
        let nodes_by_depth = self.group_by_depth(&self.dirty_nodes);

        // Process each depth wave in parallel
        for (_depth, nodes) in nodes_by_depth {
            self.runtime.pool().install(|| {
                use rayon::prelude::*;
                nodes.par_iter().for_each(|&node| {
                    // Query will be memoized automatically
                    let _style = self.db.query::<ComputedStyleQuery>(node);
                });
            });
        }

        self.dirty_nodes.clear();
        true
    }

    /// Get a computed style snapshot for all nodes.
    pub fn computed_snapshot(&self) -> HashMap<NodeKey, ComputedStyle> {
        let mut snapshot = HashMap::new();

        for &node in &self.nodes {
            let style = self.db.query::<ComputedStyleQuery>(node);
            snapshot.insert(node, (*style).clone());
        }

        snapshot
    }

    /// Group nodes by their depth in the tree for parallel processing.
    ///
    /// Parents must be styled before children (inheritance), so we process
    /// in waves by depth.
    fn group_by_depth(&self, nodes: &HashSet<NodeKey>) -> Vec<(usize, Vec<NodeKey>)> {
        use std::collections::BTreeMap;

        let mut by_depth: BTreeMap<usize, Vec<NodeKey>> = BTreeMap::new();

        for &node in nodes {
            let depth = self.compute_depth(node);
            by_depth.entry(depth).or_default().push(node);
        }

        by_depth.into_iter().collect()
    }

    /// Compute the depth of a node in the tree.
    fn compute_depth(&self, mut node: NodeKey) -> usize {
        let mut depth = 0;

        while let Some(parent) = (*self.db.input::<DomParentInput>(node)).as_ref() {
            if *parent == NodeKey::ROOT {
                break;
            }
            node = *parent;
            depth += 1;
        }

        depth
    }

    /// Get the query database (for testing/debugging).
    #[allow(dead_code)]
    pub fn query_db(&self) -> &QueryDatabase {
        &self.db
    }

    /// Get a cloned Arc to the query database for sharing with other subsystems.
    pub fn shared_query_db(&self) -> Arc<QueryDatabase> {
        Arc::clone(&self.db)
    }

    /// Get a cloned Arc to the parallel runtime for sharing with other subsystems.
    pub fn shared_runtime(&self) -> Arc<ParallelRuntime> {
        Arc::clone(&self.runtime)
    }

    /// Get all computed styles for all nodes.
    ///
    /// Queries the style for each tracked node and returns a map.
    pub fn get_all_styles(
        &self,
    ) -> std::collections::HashMap<js::NodeKey, crate::style_model::ComputedStyle> {
        use crate::queries::ComputedStyleQuery;
        let mut styles = std::collections::HashMap::new();

        for &node in &self.nodes {
            let style = self.db.query::<ComputedStyleQuery>(node);
            styles.insert(node, (*style).clone());
        }

        styles
    }
}
