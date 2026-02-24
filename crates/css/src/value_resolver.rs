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
use rewrite_core::{SingleRelationship, StylerAccess, Subpixel};

/// Resolve a CSS `LengthValue` to pixels, using the current node's
/// font-size and line-height for relative units.
pub fn resolve_length(value: &LengthValue, styler: &dyn StylerAccess) -> Subpixel {
    resolve_length_with_context(value, styler, styler)
}

/// Resolve a CSS `LengthValue` to pixels for the `font-size` property.
///
/// Per CSS spec, `em` units on `font-size` refer to the **inherited**
/// (parent's) font-size, not the element's own. This function uses the
/// parent's styler for font-relative units to avoid infinite recursion.
pub fn resolve_length_for_font_size(value: &LengthValue, styler: &dyn StylerAccess) -> Subpixel {
    let parent = styler.related(SingleRelationship::Parent);
    resolve_length_with_context(value, styler, parent.as_ref())
}

/// Core resolution: `styler` provides viewport info, `font_ctx` provides
/// the font-size and line-height used to resolve relative units.
fn resolve_length_with_context(
    value: &LengthValue,
    styler: &dyn StylerAccess,
    font_ctx: &dyn StylerAccess,
) -> Subpixel {
    match value {
        // Absolute lengths — no context needed
        LengthValue::Px(v) => Subpixel::from_f32(*v),
        LengthValue::In(v) => Subpixel::from_f32(v * 96.0),
        LengthValue::Cm(v) => Subpixel::from_f32(v * 96.0 / 2.54),
        LengthValue::Mm(v) => Subpixel::from_f32(v * 96.0 / 25.4),
        LengthValue::Q(v) => Subpixel::from_f32(v * 96.0 / 101.6),
        LengthValue::Pt(v) => Subpixel::from_f32(v * 96.0 / 72.0),
        LengthValue::Pc(v) => Subpixel::from_f32(v * 96.0 / 6.0),

        // Font-relative lengths — resolved against font_ctx
        LengthValue::Em(v) => {
            let fs = query_font_size(font_ctx);
            Subpixel::from_f32(v * fs)
        }
        LengthValue::Rem(v) => {
            let fs = query_root_font_size(styler);
            Subpixel::from_f32(v * fs)
        }
        LengthValue::Lh(v) => {
            let lh = query_line_height(font_ctx);
            Subpixel::from_f32(v * lh)
        }
        LengthValue::Rlh(v) => {
            let lh = query_root_line_height(styler);
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
            Subpixel::from_f32(v * styler.viewport_width() as f32 / 100.0)
        }
        LengthValue::Vh(v) | LengthValue::Lvh(v) | LengthValue::Svh(v) | LengthValue::Dvh(v) => {
            Subpixel::from_f32(v * styler.viewport_height() as f32 / 100.0)
        }
        LengthValue::Vi(v) | LengthValue::Svi(v) | LengthValue::Lvi(v) | LengthValue::Dvi(v) => {
            Subpixel::from_f32(v * styler.viewport_width() as f32 / 100.0)
        }
        LengthValue::Vb(v) | LengthValue::Svb(v) | LengthValue::Lvb(v) | LengthValue::Dvb(v) => {
            Subpixel::from_f32(v * styler.viewport_height() as f32 / 100.0)
        }
        LengthValue::Vmin(v)
        | LengthValue::Svmin(v)
        | LengthValue::Lvmin(v)
        | LengthValue::Dvmin(v) => {
            let min = styler.viewport_width().min(styler.viewport_height());
            Subpixel::from_f32(v * min as f32 / 100.0)
        }
        LengthValue::Vmax(v)
        | LengthValue::Svmax(v)
        | LengthValue::Lvmax(v)
        | LengthValue::Dvmax(v) => {
            let max = styler.viewport_width().max(styler.viewport_height());
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

/// Query the node's font-size in px. Defaults to 16.
fn query_font_size(styler: &dyn StylerAccess) -> f32 {
    styler
        .get_property(&PropertyId::FontSize)
        .map_or(16.0, |v| v.to_f32())
}

/// Query the root element's font-size in px. Defaults to 16.
fn query_root_font_size(styler: &dyn StylerAccess) -> f32 {
    let root = styler.root();
    root.get_property(&PropertyId::FontSize)
        .map_or(16.0, |v| v.to_f32())
}

/// Query the node's line-height in px.
/// Falls back to font-size if line-height is not set.
fn query_line_height(styler: &dyn StylerAccess) -> f32 {
    styler
        .get_property(&PropertyId::LineHeight)
        .map_or_else(|| query_font_size(styler), |v| v.to_f32())
}

/// Query the root element's line-height in px.
/// Falls back to root font-size if line-height is not set.
fn query_root_line_height(styler: &dyn StylerAccess) -> f32 {
    let root = styler.root();
    root.get_property(&PropertyId::LineHeight)
        .map_or_else(|| query_root_font_size(styler), |v| v.to_f32())
}
