//! CSS length value resolution.
//!
//! Converts CSS `LengthValue` (px, em, rem, etc.) to `Subpixel` pixels.
//!
//! Two resolution modes:
//! - `resolve_length`: resolves relative units (em, rem, lh) against the
//!   current node's computed font-size / line-height. Used for most properties.
//! - `resolve_length_for_font_size`: resolves em/lh against the **parent's**
//!   computed values, per CSS spec (font-size: 2em means 2× the inherited
//!   font-size, not the element's own). Used when the property being resolved
//!   is `font-size` or `line-height` itself.

use lightningcss::properties::PropertyId;
use lightningcss::values::length::LengthValue;
use rewrite_core::{NodeId, PropertyResolver, Subpixel};

/// Node-scoped adapter for length resolution.
///
/// Holds a `NodeId` and `&dyn PropertyResolver` so the resolver functions
/// can query properties for the appropriate node without requiring `StylerAccess`.
pub struct NodeContext<'a> {
    pub node: NodeId,
    pub resolver: &'a dyn PropertyResolver,
}

/// Resolve a CSS `LengthValue` to pixels, using the current node's
/// font-size and line-height for relative units.
pub fn resolve_length(value: &LengthValue, ctx: &NodeContext<'_>) -> Subpixel {
    resolve_length_with_context(value, ctx.resolver, ctx.node, ctx.node)
}

/// Resolve a CSS `LengthValue` to pixels for the `font-size` property.
///
/// Per CSS spec, `em` units on `font-size` refer to the **inherited**
/// (parent's) font-size, not the element's own. This function uses the
/// parent for font-relative units to avoid infinite recursion.
pub fn resolve_length_for_font_size(value: &LengthValue, ctx: &NodeContext<'_>) -> Subpixel {
    let parent = ctx.resolver.parent(ctx.node).unwrap_or(NodeId(0));
    resolve_length_with_context(value, ctx.resolver, ctx.node, parent)
}

/// Core resolution: `resolver` provides viewport info and property access.
/// `node` is the current node (for viewport/root queries).
/// `font_node` is the node whose font-size/line-height is used for relative units.
fn resolve_length_with_context(
    value: &LengthValue,
    resolver: &dyn PropertyResolver,
    node: NodeId,
    font_node: NodeId,
) -> Subpixel {
    let _ = node; // viewport info comes from resolver, not node
    match value {
        // Absolute lengths — no context needed
        LengthValue::Px(v) => Subpixel::from_f32(*v),
        LengthValue::In(v) => Subpixel::from_f32(v * 96.0),
        LengthValue::Cm(v) => Subpixel::from_f32(v * 96.0 / 2.54),
        LengthValue::Mm(v) => Subpixel::from_f32(v * 96.0 / 25.4),
        LengthValue::Q(v) => Subpixel::from_f32(v * 96.0 / 101.6),
        LengthValue::Pt(v) => Subpixel::from_f32(v * 96.0 / 72.0),
        LengthValue::Pc(v) => Subpixel::from_f32(v * 96.0 / 6.0),

        // Font-relative lengths — resolved against font_node
        LengthValue::Em(v) => {
            let fs = query_font_size(font_node, resolver);
            Subpixel::from_f32(v * fs)
        }
        LengthValue::Rem(v) => {
            let fs = query_root_font_size(resolver);
            Subpixel::from_f32(v * fs)
        }
        LengthValue::Lh(v) => {
            let lh = query_line_height(font_node, resolver);
            Subpixel::from_f32(v * lh)
        }
        LengthValue::Rlh(v) => {
            let lh = query_root_line_height(resolver);
            Subpixel::from_f32(v * lh)
        }

        // Font-metric-dependent lengths — require glyph measurement (not yet implemented)
        LengthValue::Ex(_)
        | LengthValue::Rex(_)
        | LengthValue::Cap(_)
        | LengthValue::Rcap(_)
        | LengthValue::Ch(_)
        | LengthValue::Rch(_)
        | LengthValue::Ic(_)
        | LengthValue::Ric(_) => Subpixel::ZERO,

        // Viewport-relative lengths (1 unit = 1% of viewport dimension)
        LengthValue::Vw(v) | LengthValue::Lvw(v) | LengthValue::Svw(v) | LengthValue::Dvw(v) => {
            Subpixel::from_f32(v * resolver.viewport_width() as f32 / 100.0)
        }
        LengthValue::Vh(v) | LengthValue::Lvh(v) | LengthValue::Svh(v) | LengthValue::Dvh(v) => {
            Subpixel::from_f32(v * resolver.viewport_height() as f32 / 100.0)
        }
        LengthValue::Vi(v) | LengthValue::Svi(v) | LengthValue::Lvi(v) | LengthValue::Dvi(v) => {
            Subpixel::from_f32(v * resolver.viewport_width() as f32 / 100.0)
        }
        LengthValue::Vb(v) | LengthValue::Svb(v) | LengthValue::Lvb(v) | LengthValue::Dvb(v) => {
            Subpixel::from_f32(v * resolver.viewport_height() as f32 / 100.0)
        }
        LengthValue::Vmin(v)
        | LengthValue::Svmin(v)
        | LengthValue::Lvmin(v)
        | LengthValue::Dvmin(v) => {
            let min = resolver.viewport_width().min(resolver.viewport_height());
            Subpixel::from_f32(v * min as f32 / 100.0)
        }
        LengthValue::Vmax(v)
        | LengthValue::Svmax(v)
        | LengthValue::Lvmax(v)
        | LengthValue::Dvmax(v) => {
            let max = resolver.viewport_width().max(resolver.viewport_height());
            Subpixel::from_f32(v * max as f32 / 100.0)
        }

        // Container query lengths — not yet implemented
        LengthValue::Cqw(_)
        | LengthValue::Cqh(_)
        | LengthValue::Cqi(_)
        | LengthValue::Cqb(_)
        | LengthValue::Cqmin(_)
        | LengthValue::Cqmax(_) => Subpixel::ZERO,
    }
}

/// Query a node's font-size in px. Defaults to 16.
fn query_font_size(node: NodeId, resolver: &dyn PropertyResolver) -> f32 {
    resolver
        .get_property(node, &PropertyId::FontSize)
        .map_or(16.0, |v| v.to_f32())
}

/// Query the root element's font-size in px. Defaults to 16.
fn query_root_font_size(resolver: &dyn PropertyResolver) -> f32 {
    let root = find_root_element(resolver);
    resolver
        .get_property(root, &PropertyId::FontSize)
        .map_or(16.0, |v| v.to_f32())
}

/// Query a node's line-height in px.
/// Falls back to font-size if line-height is not set.
fn query_line_height(node: NodeId, resolver: &dyn PropertyResolver) -> f32 {
    resolver
        .get_property(node, &PropertyId::LineHeight)
        .map_or_else(|| query_font_size(node, resolver), |v| v.to_f32())
}

/// Query the root element's line-height in px.
/// Falls back to root font-size if line-height is not set.
fn query_root_line_height(resolver: &dyn PropertyResolver) -> f32 {
    let root = find_root_element(resolver);
    resolver
        .get_property(root, &PropertyId::LineHeight)
        .map_or_else(|| query_root_font_size(resolver), |v| v.to_f32())
}

/// Find the root element (first element child of NodeId::ROOT).
fn find_root_element(resolver: &dyn PropertyResolver) -> NodeId {
    resolver
        .children(NodeId::ROOT)
        .into_iter()
        .find(|&child| !resolver.is_intrinsic(child))
        .unwrap_or(NodeId::ROOT)
}
