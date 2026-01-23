//! Query for resolving CSS values to subpixels.

use super::InheritedCssPropertyQuery;
use crate::{CssKeyword, CssValue, LengthValue};
use rewrite_core::{Database, DependencyContext, NodeId, Query};

/// Query that resolves a CSS property value to subpixels.
///
/// This query:
/// - Reads the CSS property value with inheritance support
/// - Resolves relative units (em, %, etc.) by querying dependencies
/// - Returns the final computed value in subpixels
pub struct CssValueQuery;

impl Query for CssValueQuery {
    type Key = (NodeId, String); // (node, property_name)
    type Value = ResolvedValue;

    fn execute(db: &Database, key: Self::Key, ctx: &mut DependencyContext) -> Self::Value {
        let (node, property) = key;

        // Get the CSS value with inheritance support
        let css_value = db.query::<InheritedCssPropertyQuery>((node, property.clone()), ctx);

        // Resolve to subpixels
        resolve_value(&css_value, node, &property, db, ctx)
    }
}

/// Resolved CSS value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedValue {
    /// Concrete subpixel value
    Subpixels(i32),
    /// Auto keyword (needs special handling by caller)
    Auto,
}

impl ResolvedValue {
    /// Get subpixels, returning 0 for Auto
    pub fn subpixels_or_zero(self) -> i32 {
        match self {
            ResolvedValue::Subpixels(sp) => sp,
            ResolvedValue::Auto => 0,
        }
    }

    /// Check if this is Auto
    pub fn is_auto(self) -> bool {
        matches!(self, ResolvedValue::Auto)
    }
}

/// Resolve a CSS value to subpixels.
fn resolve_value(
    value: &CssValue,
    node: NodeId,
    property: &str,
    db: &Database,
    ctx: &mut DependencyContext,
) -> ResolvedValue {
    match value {
        CssValue::Length(length) => {
            ResolvedValue::Subpixels(resolve_length(length, node, property, db, ctx))
        }

        CssValue::Percentage(pct) => {
            // Query containing block size for this property
            let cb_size = get_containing_block_size(node, property, db, ctx);
            ResolvedValue::Subpixels((cb_size as f32 * pct) as i32)
        }

        CssValue::Number(num) => {
            // For flex-grow/flex-shrink, store as fixed-point
            ResolvedValue::Subpixels((num * 64.0) as i32)
        }

        CssValue::Integer(int) => ResolvedValue::Subpixels(int * 64),

        CssValue::Keyword(CssKeyword::Auto) => ResolvedValue::Auto,

        CssValue::Keyword(_) => {
            // Other keywords default to 0
            ResolvedValue::Subpixels(0)
        }

        _ => {
            // Complex values not yet supported
            ResolvedValue::Subpixels(0)
        }
    }
}

/// Resolve a length value to subpixels.
fn resolve_length(
    length: &LengthValue,
    node: NodeId,
    property: &str,
    db: &Database,
    ctx: &mut DependencyContext,
) -> i32 {
    match length {
        LengthValue::Px(px) => (px * 64.0) as i32,

        LengthValue::Em(em) => {
            // Em units are relative to the font-size
            // If we're currently resolving font-size itself, use parent's font-size
            // Otherwise use this node's font-size
            let font_size_node = if property == super::properties::FONT_SIZE {
                // For font-size property, em is relative to parent's font-size
                let parents = db.resolve_relationship(node, rewrite_core::Relationship::Parent);
                parents.first().copied().unwrap_or(node)
            } else {
                // For other properties, em is relative to this node's font-size
                node
            };

            let font_size = db
                .query::<CssValueQuery>(
                    (font_size_node, super::properties::FONT_SIZE.to_string()),
                    ctx,
                )
                .subpixels_or_zero();

            // font_size is in subpixels, convert back to px, multiply by em, convert to subpixels
            (em * (font_size as f32 / 64.0) * 64.0) as i32
        }

        LengthValue::Rem(rem) => {
            // Query root element ID from RootElementInput
            let root_node = db
                .get_input::<super::RootElementInput>(&())
                .unwrap_or_else(|| NodeId::new(0));

            // Query root element's font-size
            let root_font_size = db
                .query::<CssValueQuery>((root_node, super::properties::FONT_SIZE.to_string()), ctx)
                .subpixels_or_zero();

            // If root has no font-size set, use 16px default
            let root_size = if root_font_size == 0 {
                16.0 * 64.0
            } else {
                root_font_size as f32
            };

            (rem * root_size / 64.0 * 64.0) as i32
        }

        LengthValue::Vw(vw) => {
            let viewport = db
                .get_input::<super::ViewportInput>(&())
                .unwrap_or_default();
            (vw / 100.0 * viewport.width * 64.0) as i32
        }

        LengthValue::Vh(vh) => {
            let viewport = db
                .get_input::<super::ViewportInput>(&())
                .unwrap_or_default();
            (vh / 100.0 * viewport.height * 64.0) as i32
        }

        LengthValue::Percent(pct) => {
            // Same as percentage value
            let cb_size = get_containing_block_size(node, property, db, ctx);
            (cb_size as f32 * pct) as i32
        }

        LengthValue::Vmin(vmin) => {
            let viewport = db
                .get_input::<super::ViewportInput>(&())
                .unwrap_or_default();
            let min = viewport.width.min(viewport.height);
            (vmin / 100.0 * min * 64.0) as i32
        }

        LengthValue::Vmax(vmax) => {
            let viewport = db
                .get_input::<super::ViewportInput>(&())
                .unwrap_or_default();
            let max = viewport.width.max(viewport.height);
            (vmax / 100.0 * max * 64.0) as i32
        }

        // Other units (ch, ex, cm, mm, in, pt, pc) - approximate for now
        _ => 0,
    }
}

/// Get the containing block size for percentage resolution.
///
/// Different properties use different reference dimensions:
/// - width, padding-left/right, margin-left/right: containing block width
/// - height, padding-top/bottom, margin-top/bottom: containing block height (usually)
/// - Note: padding/margin percentages ALL use width, even for vertical properties
fn get_containing_block_size(
    node: NodeId,
    property: &str,
    db: &Database,
    ctx: &mut DependencyContext,
) -> i32 {
    use super::LayoutHeightQuery;
    use super::LayoutWidthQuery;

    // Get parent node
    let parents = db.resolve_relationship(node, rewrite_core::Relationship::Parent);
    let parent = parents.first().copied();

    if let Some(parent) = parent {
        // For padding and margin, percentages ALWAYS resolve against width
        if property.starts_with("padding") || property.starts_with("margin") {
            // Query parent's layout width (content box)
            return db.query::<LayoutWidthQuery>(parent, ctx);
        }

        // For width properties, use parent width
        if property == "width" || property == "max-width" || property == "min-width" {
            // Query parent's layout width
            return db.query::<LayoutWidthQuery>(parent, ctx);
        }

        // For height properties, use parent height
        if property == "height" || property == "max-height" || property == "min-height" {
            // Query parent's layout height
            return db.query::<LayoutHeightQuery>(parent, ctx);
        }
    }

    // No containing block or unknown property
    0
}
