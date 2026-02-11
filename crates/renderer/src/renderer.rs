//! Main renderer.

use lightningcss::properties::Property;
use rewrite_core::{Axis, DomBroadcast, NodeId, ResolveContext, Subpixel, Subscriber};
use rewrite_css::{NodeStylerContext, Styler, affects_position, affects_size};
use rewrite_layout::{offset_query, size_query};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

/// Computed layout box values for a single node.
#[derive(Debug, Clone, Default)]
pub struct ComputedBox {
    pub width: Option<Subpixel>,
    pub height: Option<Subpixel>,
    pub x: Option<Subpixel>,
    pub y: Option<Subpixel>,
}

/// Create a static-lifetime node styler context for formula resolution.
/// The caller must ensure `styler` outlives the resolve context.
fn make_ctx(styler: &Styler, node: NodeId, vw: u32, vh: u32) -> NodeStylerContext<'static> {
    NodeStylerContext::new(styler, node, vw, vh).into_static()
}

/// Resolve layout values for a node given a property change.
///
/// Returns a `ComputedBox` with whichever fields could be resolved
/// (size and/or position) based on the property that changed.
pub fn resolve_layout(
    styler: &Styler,
    node: NodeId,
    property: &Property<'static>,
    viewport_width: u32,
    viewport_height: u32,
) -> ComputedBox {
    let query_ctx = make_ctx(styler, node, viewport_width, viewport_height);
    let mut result = ComputedBox::default();

    if affects_size(property) {
        if let Some(formula_h) = size_query(&query_ctx, Axis::Horizontal) {
            let mut ctx = ResolveContext::new(
                viewport_width,
                viewport_height,
                make_ctx(styler, node, viewport_width, viewport_height),
            );
            result.width = ctx.resolve(formula_h, node);
        }
        if let Some(formula_v) = size_query(&query_ctx, Axis::Vertical) {
            let mut ctx = ResolveContext::new(
                viewport_width,
                viewport_height,
                make_ctx(styler, node, viewport_width, viewport_height),
            );
            result.height = ctx.resolve(formula_v, node);
        }
    }

    if affects_position(property) {
        if let Some(formula_h) = offset_query(&query_ctx, Axis::Horizontal) {
            let mut ctx = ResolveContext::new(
                viewport_width,
                viewport_height,
                make_ctx(styler, node, viewport_width, viewport_height),
            );
            result.x = ctx.resolve(formula_h, node);
        }
        if let Some(formula_v) = offset_query(&query_ctx, Axis::Vertical) {
            let mut ctx = ResolveContext::new(
                viewport_width,
                viewport_height,
                make_ctx(styler, node, viewport_width, viewport_height),
            );
            result.y = ctx.resolve(formula_v, node);
        }
    }

    result
}

/// Resolve all layout dimensions for a node unconditionally.
///
/// Unlike `resolve_layout`, this doesn't check which property changed —
/// it attempts to resolve width, height, x, and y. Use this when a node
/// has been attached to the tree and all its styles are already available.
pub fn resolve_all_layout(
    styler: &Styler,
    node: NodeId,
    viewport_width: u32,
    viewport_height: u32,
) -> ComputedBox {
    let query_ctx = make_ctx(styler, node, viewport_width, viewport_height);
    let mut result = ComputedBox::default();

    if let Some(formula_h) = size_query(&query_ctx, Axis::Horizontal) {
        let mut ctx = ResolveContext::new(
            viewport_width,
            viewport_height,
            make_ctx(styler, node, viewport_width, viewport_height),
        );
        result.width = ctx.resolve(formula_h, node);
    }
    if let Some(formula_v) = size_query(&query_ctx, Axis::Vertical) {
        let mut ctx = ResolveContext::new(
            viewport_width,
            viewport_height,
            make_ctx(styler, node, viewport_width, viewport_height),
        );
        result.height = ctx.resolve(formula_v, node);
    }
    if let Some(formula_h) = offset_query(&query_ctx, Axis::Horizontal) {
        let mut ctx = ResolveContext::new(
            viewport_width,
            viewport_height,
            make_ctx(styler, node, viewport_width, viewport_height),
        );
        result.x = ctx.resolve(formula_h, node);
    }
    if let Some(formula_v) = offset_query(&query_ctx, Axis::Vertical) {
        let mut ctx = ResolveContext::new(
            viewport_width,
            viewport_height,
            make_ctx(styler, node, viewport_width, viewport_height),
        );
        result.y = ctx.resolve(formula_v, node);
    }

    result
}

/// Renderer that receives property notifications and manages layout/GPU state.
pub struct Renderer {
    /// Styler reference for CSS property queries.
    styler: Arc<Styler>,
    /// Viewport width in pixels.
    viewport_width: AtomicU32,
    /// Viewport height in pixels.
    viewport_height: AtomicU32,
}

impl Renderer {
    pub fn new(styler: Arc<Styler>) -> Self {
        Self {
            styler,
            viewport_width: AtomicU32::new(0),
            viewport_height: AtomicU32::new(0),
        }
    }

    /// Get the styler for property queries.
    pub fn styler(&self) -> &Styler {
        &self.styler
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
        let _computed = resolve_layout(
            &self.styler,
            node,
            property,
            self.viewport_width(),
            self.viewport_height(),
        );
        // TODO: Store computed values for GPU rendering
    }

    fn on_dom(&self, update: DomBroadcast) {
        match update {
            DomBroadcast::CreateNode { node: _, parent: _ } => {
                // TODO: Handle new node
            }
        }
    }
}
