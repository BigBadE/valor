//! Main renderer.

use lightningcss::properties::{Property, PropertyId};
use rewrite_core::{
    Axis, Database, DomBroadcast, Formula, NodeId, ResolveContext, Subpixel, Subscriber,
    classify_property,
};
use rewrite_css::{NodeStylerContext, Styler};
use rewrite_layout::{offset_query, property_query, size_query};
use std::collections::HashMap;
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

    /// Handle a new DOM node being created. Resolves the node and
    /// re-resolves all ancestors whose sizes depend on children.
    pub fn on_node_created(&mut self, node: NodeId, parent: NodeId) {
        // Resolve this node — confident properties (inline styles, class
        // selectors) are already in the DB from restyle_node().
        self.resolve_node(node);

        // Re-resolve ancestors — their content-based sizes changed.
        self.re_resolve_ancestors(parent);
    }

    /// Handle a property change on a node.
    ///
    /// Re-resolves the node, affected descendants, ancestors, and neighbors.
    pub fn on_property_change(&mut self, node: NodeId, property: &Property<'static>) {
        let prop_id = property.property_id();

        // Re-resolve this node with current properties.
        self.ctx.invalidate_node(node);
        self.resolve_node(node);

        // Inherited properties cascade to all descendants.
        // Non-inherited only affect direct children.
        let inherited = classify_property(&prop_id).is_some_and(|group| group.is_inherited());
        if inherited {
            self.re_resolve_descendants(node);
        } else {
            let children = self.db.dom_children(node);
            for child in children {
                self.ctx.invalidate_node(child);
                self.resolve_node(child);
            }
        }

        // Re-resolve ancestors — this node's size may have changed.
        if let Some(parent) = self.db.dom_parent(node) {
            self.re_resolve_ancestors(parent);
        }

        // Re-resolve sparse tree neighbors.
        let neighbors = self.db.neighbors(node, &prop_id);
        for neighbor in neighbors {
            self.ctx.invalidate_node(neighbor);
            self.resolve_node(neighbor);
        }
    }

    /// Re-resolve each ancestor from `start` up to the root.
    fn re_resolve_ancestors(&mut self, start: NodeId) {
        let mut ancestor = Some(start);
        while let Some(anc) = ancestor {
            self.ctx.invalidate_node(anc);
            self.resolve_node(anc);
            ancestor = self.db.dom_parent(anc);
        }
    }

    /// Recursively invalidate and re-resolve all descendants (post-order).
    fn re_resolve_descendants(&mut self, node: NodeId) {
        let children = self.db.dom_children(node);
        for child in children {
            self.re_resolve_descendants(child);
        }
        // Re-resolve after children so content-based sizes are correct.
        self.ctx.invalidate_node(node);
        self.resolve_node(node);
    }

    /// Read a cached box-model property value. Returns `None` if not cached.
    pub fn get_property(&self, node: NodeId, prop_id: &PropertyId<'static>) -> Option<Subpixel> {
        let vw = self.ctx.viewport_width;
        let vh = self.ctx.viewport_height;
        let query_ctx = make_ctx(&self.styler, &self.db, node, vw, vh);
        let formula = property_query(&query_ctx, prop_id)?;
        self.ctx.get_cached(formula, node)
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
