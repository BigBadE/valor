//! Main renderer.

use lightningcss::properties::{Property, PropertyId};
use rewrite_core::{
    Axis, Database, DomBroadcast, Formula, NodeId, ResolveContext, Subpixel, Subscriber,
};
use rewrite_css::{CssPropertyResolver, Styler};
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

/// Create a `CssPropertyResolver` from shared state.
fn make_resolver(
    styler: &Arc<Styler>,
    db: &Arc<Database>,
    vw: u32,
    vh: u32,
) -> CssPropertyResolver {
    CssPropertyResolver::new(styler.clone(), db.clone(), vw, vh)
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

    /// Read cached layout values for a node.
    pub fn get_node(&self, node: NodeId) -> ComputedBox {
        let vw = self.ctx.viewport_width;
        let vh = self.ctx.viewport_height;
        let resolver = make_resolver(&self.styler, &self.db, vw, vh);
        let mut result = ComputedBox::default();

        if let Some(formula) = size_query(node, &resolver, Axis::Horizontal) {
            result.width = self.ctx.get_cached(formula, node);
        }
        if let Some(formula) = size_query(node, &resolver, Axis::Vertical) {
            result.height = self.ctx.get_cached(formula, node);
        }
        if let Some(formula) = offset_query(node, &resolver, Axis::Horizontal) {
            result.x = self.ctx.get_cached(formula, node);
        }
        if let Some(formula) = offset_query(node, &resolver, Axis::Vertical) {
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
        let resolver = make_resolver(&self.styler, &self.db, vw, vh);
        let nf = self.formulas.entry(node).or_default();
        let mut result = ComputedBox::default();

        let width_formula = size_query(node, &resolver, Axis::Horizontal);
        nf.width = width_formula;
        if let Some(formula) = width_formula {
            result.width = self.ctx.resolve(formula, node, &resolver);
        }

        let height_formula = size_query(node, &resolver, Axis::Vertical);
        nf.height = height_formula;
        if let Some(formula) = height_formula {
            result.height = self.ctx.resolve(formula, node, &resolver);
        }

        let offset_x_formula = offset_query(node, &resolver, Axis::Horizontal);
        nf.offset_x = offset_x_formula;
        if let Some(formula) = offset_x_formula {
            result.x = self.ctx.resolve(formula, node, &resolver);
        }

        let offset_y_formula = offset_query(node, &resolver, Axis::Vertical);
        nf.offset_y = offset_y_formula;
        if let Some(formula) = offset_y_formula {
            result.y = self.ctx.resolve(formula, node, &resolver);
        }

        // Resolve box-model properties (margin, padding, border) so they
        // are cached for later reads.
        for prop_id in &Self::BOX_MODEL_PROPS {
            if let Some(formula) = property_query(node, &resolver, prop_id) {
                self.ctx.resolve(formula, node, &resolver);
            }
        }

        result
    }

    /// Handle a new DOM node being created.
    pub fn on_node_created(&mut self, node: NodeId, _parent: NodeId) {

        self.resolve_node(node);
        self.propagate_changes(node);
    }

    /// Handle a property change on a node.
    pub fn on_property_change(&mut self, node: NodeId, property: &Property<'static>) {
        let prop_id = property.property_id();
        let group = rewrite_core::classify_property(&prop_id);
        if matches!(group, Some(rewrite_core::PropertyGroup::Background)) {
            return;
        }

        // Check if any layout formula on this node reads the changed property.
        // If the node has no formulas yet, resolve it fully (first time).
        if let Some(nf) = self.formulas.get(&node) {
            let any_affected = [nf.width, nf.height, nf.offset_x, nf.offset_y]
                .iter()
                .flatten()
                .any(|f| f.depends_on_css_property(&prop_id));
            if !any_affected {
                return;
            }
        }

        let old_values = self.get_node(node);

        self.resolve_node(node);
        let new_values = self.get_node(node);

        let changed = old_values.width != new_values.width
            || old_values.height != new_values.height
            || old_values.x != new_values.x
            || old_values.y != new_values.y;

        if changed {
            self.propagate_changes(node);
        }

        // Inherited properties (font-size, etc.) affect descendants via
        // inheritance even if this node's layout values didn't change.
        // A wrapper div's size doesn't change when font-size changes,
        // but its text node grandchildren measure differently.
        if matches!(group, Some(rewrite_core::PropertyGroup::Text)) {
            self.propagate_inherited_down(node);
        }
    }

    /// Propagate inherited property changes to ALL descendants.
    /// Inherited properties bypass intermediate nodes — a font-size
    /// change on a grandparent affects text nodes even if the parent
    /// div's layout values are unchanged.
    fn propagate_inherited_down(&mut self, node: NodeId) {
        for child in self.db.dom_children(node) {
            let old_values = self.get_node(child);
            self.resolve_node(child);
            let new_values = self.get_node(child);

            // Always recurse — inheritance goes through entire subtree.
            self.propagate_inherited_down(child);

            // If size changed, propagate to parent and siblings.
            if old_values.width != new_values.width
                || old_values.height != new_values.height
            {
                self.propagate_changes(child);
            }
        }
    }

    /// Propagate changes from a node to all dependents.
    fn propagate_changes(&mut self, node: NodeId) {
        if let Some(parent) = self.db.dom_parent(node) {
            self.re_resolve_and_propagate(parent);
        }
        if let Some(parent) = self.db.dom_parent(node) {
            for sibling in self.db.dom_children(parent) {
                if sibling != node {
                    self.re_resolve_and_propagate(sibling);
                }
            }
        }
        for child in self.db.dom_children(node) {
            self.re_resolve_and_propagate(child);
        }
    }

    /// Invalidate, re-resolve, and propagate if values changed.
    fn re_resolve_and_propagate(&mut self, node: NodeId) {
        let old_values = self.get_node(node);

        self.resolve_node(node);
        let new_values = self.get_node(node);

        if old_values.width != new_values.width
            || old_values.height != new_values.height
            || old_values.x != new_values.x
            || old_values.y != new_values.y
        {
            self.propagate_changes(node);
        }
    }


    /// Read a cached box-model property value.
    pub fn get_property(&self, node: NodeId, prop_id: &PropertyId<'static>) -> Option<Subpixel> {
        let vw = self.ctx.viewport_width;
        let vh = self.ctx.viewport_height;
        let resolver = make_resolver(&self.styler, &self.db, vw, vh);
        let formula = property_query(node, &resolver, prop_id)?;
        self.ctx.get_cached(formula, node)
    }

    /// Resolve all nodes, reusing cached values from incremental updates.
    pub fn resolve_nodes(&mut self, nodes: &[NodeId]) {
        for &node in nodes {
            self.resolve_node(node);
        }
    }

    /// Clear all cached layout values. Used for benchmarking to force
    /// a complete re-resolution.
    pub fn clear_cache(&mut self) {
        self.ctx.clear_cache();
        self.formulas.clear();
    }
}

/// Renderer that receives property notifications and manages layout/GPU state.
pub struct Renderer {
    layout: Mutex<LayoutState>,
    viewport_width: AtomicU32,
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

    pub fn set_viewport(&self, width: u32, height: u32) {
        self.viewport_width.store(width, Ordering::Relaxed);
        self.viewport_height.store(height, Ordering::Relaxed);
    }

    pub fn viewport_width(&self) -> u32 {
        self.viewport_width.load(Ordering::Relaxed)
    }

    pub fn viewport_height(&self) -> u32 {
        self.viewport_height.load(Ordering::Relaxed)
    }
}

impl Subscriber for Renderer {
    fn on_property(&self, node: NodeId, property: &Property<'static>) {
        let mut layout = self.layout.lock().expect("lock poisoned");
        layout.on_property_change(node, property);
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
