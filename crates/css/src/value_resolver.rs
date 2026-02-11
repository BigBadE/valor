//! CSS length value resolution.
//!
//! Converts CSS `LengthValue` (px, em, rem, etc.) to `Subpixel` pixels.
//! Takes a `StylerAccess` to query font-size, line-height, and viewport context.

use lightningcss::properties::PropertyId;
use lightningcss::values::length::LengthValue;
use rewrite_core::{NodeId, SingleRelationship, StylerAccess, Subpixel};

/// Resolve a CSS `LengthValue` to pixels.
pub fn resolve_length(value: &LengthValue, styler: &impl StylerAccess) -> Subpixel {
    match value {
        // Absolute lengths
        LengthValue::Px(v) => *v as Subpixel,
        LengthValue::In(v) => (v * 96.0) as Subpixel,
        LengthValue::Cm(v) => (v * 96.0 / 2.54) as Subpixel,
        LengthValue::Mm(v) => (v * 96.0 / 25.4) as Subpixel,
        LengthValue::Q(v) => (v * 96.0 / 101.6) as Subpixel,
        LengthValue::Pt(v) => (v * 96.0 / 72.0) as Subpixel,
        LengthValue::Pc(v) => (v * 96.0 / 6.0) as Subpixel,

        // Font-relative lengths
        LengthValue::Em(v) => {
            let fs = query_font_size(styler);
            (v * fs) as Subpixel
        }
        LengthValue::Rem(v) => {
            let fs = query_root_font_size(styler);
            (v * fs) as Subpixel
        }
        LengthValue::Lh(v) => {
            let lh = query_line_height(styler);
            (v * lh) as Subpixel
        }
        LengthValue::Rlh(v) => {
            let lh = query_root_line_height(styler);
            (v * lh) as Subpixel
        }

        // Font-metric-dependent lengths — require glyph measurement (not yet implemented)
        LengthValue::Ex(_)
        | LengthValue::Rex(_)
        | LengthValue::Cap(_)
        | LengthValue::Rcap(_)
        | LengthValue::Ch(_)
        | LengthValue::Rch(_)
        | LengthValue::Ic(_)
        | LengthValue::Ric(_) => 0,

        // Viewport-relative lengths (1 unit = 1% of viewport dimension)
        LengthValue::Vw(v) | LengthValue::Lvw(v) | LengthValue::Svw(v) | LengthValue::Dvw(v) => {
            (v * styler.viewport_width() as f32 / 100.0) as Subpixel
        }
        LengthValue::Vh(v) | LengthValue::Lvh(v) | LengthValue::Svh(v) | LengthValue::Dvh(v) => {
            (v * styler.viewport_height() as f32 / 100.0) as Subpixel
        }
        LengthValue::Vi(v) | LengthValue::Svi(v) | LengthValue::Lvi(v) | LengthValue::Dvi(v) => {
            (v * styler.viewport_width() as f32 / 100.0) as Subpixel
        }
        LengthValue::Vb(v) | LengthValue::Svb(v) | LengthValue::Lvb(v) | LengthValue::Dvb(v) => {
            (v * styler.viewport_height() as f32 / 100.0) as Subpixel
        }
        LengthValue::Vmin(v)
        | LengthValue::Svmin(v)
        | LengthValue::Lvmin(v)
        | LengthValue::Dvmin(v) => {
            let min = styler.viewport_width().min(styler.viewport_height());
            (v * min as f32 / 100.0) as Subpixel
        }
        LengthValue::Vmax(v)
        | LengthValue::Svmax(v)
        | LengthValue::Lvmax(v)
        | LengthValue::Dvmax(v) => {
            let max = styler.viewport_width().max(styler.viewport_height());
            (v * max as f32 / 100.0) as Subpixel
        }

        // Container query lengths — not yet implemented
        LengthValue::Cqw(_)
        | LengthValue::Cqh(_)
        | LengthValue::Cqi(_)
        | LengthValue::Cqb(_)
        | LengthValue::Cqmin(_)
        | LengthValue::Cqmax(_) => 0,
    }
}

/// Query the current node's font-size in px. Defaults to 16.
fn query_font_size(styler: &impl StylerAccess) -> f32 {
    styler
        .get_property(&PropertyId::FontSize)
        .map_or(16.0, |v| v as f32)
}

/// Query the root node's font-size in px. Defaults to 16.
fn query_root_font_size<T: StylerAccess>(styler: &T) -> f32 {
    let root = navigate_to_root(styler);
    root.get_property(&PropertyId::FontSize)
        .map_or(16.0, |v| v as f32)
}

/// Query the current node's line-height in px.
/// Falls back to font-size if line-height is not set.
fn query_line_height(styler: &impl StylerAccess) -> f32 {
    styler
        .get_property(&PropertyId::LineHeight)
        .map_or_else(|| query_font_size(styler), |v| v as f32)
}

/// Query the root node's line-height in px.
/// Falls back to root font-size if line-height is not set.
fn query_root_line_height<T: StylerAccess>(styler: &T) -> f32 {
    let root = navigate_to_root(styler);
    root.get_property(&PropertyId::LineHeight)
        .map_or_else(|| query_root_font_size(styler), |v| v as f32)
}

/// Navigate to the root node's styler.
fn navigate_to_root<T: StylerAccess>(styler: &T) -> T {
    let mut current = styler.related(SingleRelationship::Parent);
    while current.node_id() != NodeId::ROOT {
        current = current.related(SingleRelationship::Parent);
    }
    current
}
