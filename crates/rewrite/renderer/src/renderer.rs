//! Main renderer that consumes DOM updates and manages rendering.

use rewrite_core::{Database, NodeId};

/// Main renderer that tracks render-relevant nodes and manages rendering.
pub struct Renderer {
    /// Reference to the layout database.
    db: Database,

    /// Set of nodes currently being tracked for rendering.
    tracked_nodes: std::collections::HashSet<NodeId>,

    /// Viewport dimensions.
    viewport_width: f32,
    viewport_height: f32,
}

impl Renderer {
    /// Create a new renderer.
    pub fn new(db: Database, viewport_width: f32, viewport_height: f32) -> Self {
        Self {
            db,
            tracked_nodes: std::collections::HashSet::new(),
            viewport_width,
            viewport_height,
        }
    }

    /// Resize the viewport.
    pub fn resize(&mut self, width: f32, height: f32) {
        self.viewport_width = width;
        self.viewport_height = height;

        // TODO: Re-evaluate all tracked nodes (viewport change affects visibility)
    }

    /// Get the current set of tracked nodes.
    pub fn tracked_nodes(&self) -> &std::collections::HashSet<NodeId> {
        &self.tracked_nodes
    }

    /// Get the database reference.
    pub fn database(&self) -> &Database {
        &self.db
    }
}
