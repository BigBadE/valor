//! Main renderer.

use lightningcss::properties::{Property, PropertyId};
use rewrite_core::{
    Axis, Database, DomBroadcast, Formula, NodeId, ResolveContext, Subpixel, Subscriber,
};
use rewrite_css::{NodeStylerContext, Styler};
use rewrite_layout::{offset_query, property_query, size_query};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

/// Computed layout box values for a single node.
#[derive(Debug, Clone, Default)]
pub struct ComputedBox {
    pub width: Option<Subpixel>,
    pub height: Option<Subpixel>,
    pub x: Option<Subpixel>,
    pub y: Option<Subpixel>,
}

/// Per-node record of which formulas are currently active.
#[derive(Default)]
struct NodeFormulas {
    width: Option<&'static Formula>,
    height: Option<&'static Formula>,
    offset_x: Option<&'static Formula>,
    offset_y: Option<&'static Formula>,
}

/// Create a `NodeStylerContext` from shared state.
fn make_ctx(
    styler: &Arc<Styler>,
    db: &Arc<Database>,
    node: NodeId,
    vw: u32,
    vh: u32,
) -> NodeStylerContext {
    NodeStylerContext::new(styler.clone(), db.clone(), node, vw, vh)
}

/// Persistent layout state that owns a `ResolveContext` and tracks
/// formula assignments per node.
pub struct LayoutState {
    ctx: ResolveContext,
    formulas: HashMap<NodeId, NodeFormulas>,
    styler: Arc<Styler>,
    db: Arc<Database>,
}

impl LayoutState {
    /// Create a new layout state.
    pub fn new(
        styler: Arc<Styler>,
        db: Arc<Database>,
        viewport_width: u32,
        viewport_height: u32,
    ) -> Self {
        Self {
            ctx: ResolveContext::new(viewport_width, viewport_height),
            formulas: HashMap::new(),
            styler,
            db,
        }
    }

    /// Read cached layout values for a node. The node must have been
    /// previously resolved via `on_property_change` or `on_node_created`.
    pub fn get_node(&self, node: NodeId) -> ComputedBox {
        let vw = self.ctx.viewport_width;
        let vh = self.ctx.viewport_height;
        let query_ctx = make_ctx(&self.styler, &self.db, node, vw, vh);
        let mut result = ComputedBox::default();

        if let Some(formula) = size_query(&query_ctx, Axis::Horizontal) {
            result.width = self.ctx.get_cached(formula, node);
        }
        if let Some(formula) = size_query(&query_ctx, Axis::Vertical) {
            result.height = self.ctx.get_cached(formula, node);
        }
        if let Some(formula) = offset_query(&query_ctx, Axis::Horizontal) {
            result.x = self.ctx.get_cached(formula, node);
        }
        if let Some(formula) = offset_query(&query_ctx, Axis::Vertical) {
            result.y = self.ctx.get_cached(formula, node);
        }

        result
    }

    /// All box-model properties that serialization may read.
    const BOX_MODEL_PROPS: [PropertyId<'static>; 12] = [
        PropertyId::MarginTop,
        PropertyId::MarginRight,
        PropertyId::MarginBottom,
        PropertyId::MarginLeft,
        PropertyId::PaddingTop,
        PropertyId::PaddingRight,
        PropertyId::PaddingBottom,
        PropertyId::PaddingLeft,
        PropertyId::BorderTopWidth,
        PropertyId::BorderRightWidth,
        PropertyId::BorderBottomWidth,
        PropertyId::BorderLeftWidth,
    ];

    /// Full resolve of all layout dimensions and box-model properties for a node.
    pub fn resolve_node(&mut self, node: NodeId) -> ComputedBox {
        let vw = self.ctx.viewport_width;
        let vh = self.ctx.viewport_height;
        let query_ctx = make_ctx(&self.styler, &self.db, node, vw, vh);
        let nf = self.formulas.entry(node).or_default();
        let mut result = ComputedBox::default();

        let width_formula = size_query(&query_ctx, Axis::Horizontal);
        nf.width = width_formula;
        if let Some(formula) = width_formula {
            result.width = self.ctx.resolve(formula, node, &query_ctx);
        }

        let height_formula = size_query(&query_ctx, Axis::Vertical);
        nf.height = height_formula;
        if let Some(formula) = height_formula {
            result.height = self.ctx.resolve(formula, node, &query_ctx);
        }

        let offset_x_formula = offset_query(&query_ctx, Axis::Horizontal);
        nf.offset_x = offset_x_formula;
        if let Some(formula) = offset_x_formula {
            result.x = self.ctx.resolve(formula, node, &query_ctx);
        }

        let offset_y_formula = offset_query(&query_ctx, Axis::Vertical);
        nf.offset_y = offset_y_formula;
        if let Some(formula) = offset_y_formula {
            result.y = self.ctx.resolve(formula, node, &query_ctx);
        }

        // Resolve box-model properties (margin, padding, border) so they
        // are cached for later reads.
        for prop_id in &Self::BOX_MODEL_PROPS {
            if let Some(formula) = property_query(&query_ctx, prop_id) {
                self.ctx.resolve(formula, node, &query_ctx);
            }
        }

        result
    }

    /// Handle a new DOM node being created.
    ///
    /// Invalidates and resolves the new node, then propagates to its
    /// parent (whose child-dependent formulas like auto-height change).
    pub fn on_node_created(&mut self, node: NodeId, parent: NodeId) {
        // The node may have been partially resolved during on_property_change
        // calls that fired before the DOM parent link was established.
        // Invalidate to recompute with correct parent context.
        self.ctx.invalidate_node(node);
        self.resolve_node(node);

        // Parent's child-dependent formulas (auto-height, flex sizing) and
        // siblings' offset formulas may now be stale.
        let mut visited = HashSet::new();
        visited.insert(node);
        self.propagate_changes(parent, &mut visited);
    }

    /// Handle a property change on a node.
    ///
    /// Invalidates only this node's cached formulas, re-resolves, and
    /// propagates to nodes that depend on it.
    pub fn on_property_change(&mut self, node: NodeId, _property: &Property<'static>) {
        self.ctx.invalidate_node(node);
        self.resolve_node(node);

        let mut visited = HashSet::new();
        visited.insert(node);
        self.propagate_changes(node, &mut visited);
    }

    /// Propagate changes from a node to all nodes that depend on it.
    ///
    /// Walks parent, siblings, and children to find dependents, then
    /// invalidates and re-resolves each. If a dependent's values changed,
    /// recursively propagates further. Uses `visited` to prevent cycles.
    fn propagate_changes(&mut self, node: NodeId, visited: &mut HashSet<NodeId>) {
        let mut dependents: Vec<NodeId> = Vec::new();

        // Parent might depend on children
        if let Some(parent) = self.db.dom_parent(node) {
            if !visited.contains(&parent) {
                dependents.push(parent);
            }
        }

        // Siblings might depend on siblings
        if let Some(parent) = self.db.dom_parent(node) {
            for sibling in self.db.dom_children(parent) {
                if sibling != node && !visited.contains(&sibling) {
                    dependents.push(sibling);
                }
            }
        }

        // Children might depend on parent
        for child in self.db.dom_children(node) {
            if !visited.contains(&child) {
                dependents.push(child);
            }
        }

        for dependent in dependents {
            self.re_resolve_and_propagate(dependent, visited);
        }
    }

    /// Invalidate, re-resolve a node, and propagate if its values changed.
    fn re_resolve_and_propagate(&mut self, node: NodeId, visited: &mut HashSet<NodeId>) {
        if !visited.insert(node) {
            return; // Already visited
        }

        let old_values = self.get_node(node);
        self.ctx.invalidate_node(node);
        self.resolve_node(node);
        let new_values = self.get_node(node);

        if old_values.width != new_values.width
            || old_values.height != new_values.height
            || old_values.x != new_values.x
            || old_values.y != new_values.y
        {
            self.propagate_changes(node, visited);
        }
    }

    /// Read a cached box-model property value. Returns `None` if not cached.
    pub fn get_property(&self, node: NodeId, prop_id: &PropertyId<'static>) -> Option<Subpixel> {
        let vw = self.ctx.viewport_width;
        let vh = self.ctx.viewport_height;
        let query_ctx = make_ctx(&self.styler, &self.db, node, vw, vh);
        let formula = property_query(&query_ctx, prop_id)?;
        self.ctx.get_cached(formula, node)
    }

    /// Resolve all nodes, reusing cached values from incremental updates.
    ///
    /// Nodes already resolved during `on_property_change` /
    /// `on_node_created` will be cache hits. Only uncached or
    /// never-resolved nodes are computed fresh.
    pub fn resolve_nodes(&mut self, nodes: &[NodeId]) {
        for &node in nodes {
            self.resolve_node(node);
        }
    }
}

/// Renderer that receives property notifications and manages layout/GPU state.
pub struct Renderer {
    /// Persistent layout state with incremental resolution.
    layout: Mutex<LayoutState>,
    /// Viewport width in pixels.
    viewport_width: AtomicU32,
    /// Viewport height in pixels.
    viewport_height: AtomicU32,
}

impl Renderer {
    pub fn new(styler: Arc<Styler>, db: Arc<Database>) -> Self {
        Self {
            layout: Mutex::new(LayoutState::new(styler, db, 0, 0)),
            viewport_width: AtomicU32::new(0),
            viewport_height: AtomicU32::new(0),
        }
    }

    /// Set the viewport dimensions.
    pub fn set_viewport(&self, width: u32, height: u32) {
        self.viewport_width.store(width, Ordering::Relaxed);
        self.viewport_height.store(height, Ordering::Relaxed);
    }

    /// Get the current viewport width.
    pub fn viewport_width(&self) -> u32 {
        self.viewport_width.load(Ordering::Relaxed)
    }

    /// Get the current viewport height.
    pub fn viewport_height(&self) -> u32 {
        self.viewport_height.load(Ordering::Relaxed)
    }
}

impl Subscriber for Renderer {
    fn on_property(&self, node: NodeId, property: &Property<'static>) {
        let mut layout = self.layout.lock().expect("lock poisoned");
        layout.on_property_change(node, property);
        // TODO: Store computed values for GPU rendering
    }

    fn on_dom(&self, update: DomBroadcast) {
        match update {
            DomBroadcast::CreateNode { node, parent } => {
                let mut layout = self.layout.lock().expect("lock poisoned");
                layout.on_node_created(node, parent);
            }
        }
    }
}
