//! Used values resolution scaffolding.
//!
//! This module introduces a placeholder for resolving computed styles into
//! used values that require layout context (e.g., percentage resolution,
//! box-sizing). For now, it provides a minimal API and resolves only a
//! small subset to keep call-sites stable.

use crate::{ComputedStyle, Edges, SizeSpecified};

/// Context required to resolve used values, typically provided by layout.
/// - containing_block_width/height are in CSS pixels.
/// - box_sizing_border_box is a simplified flag assumed by early layout.
#[derive(Debug, Clone, Copy)]
pub struct UsedValuesContext {
    pub containing_block_width: f32,
    pub containing_block_height: f32,
    pub box_sizing_border_box: bool,
}

impl Default for UsedValuesContext {
    fn default() -> Self {
        Self { containing_block_width: 0.0, containing_block_height: 0.0, box_sizing_border_box: true }
    }
}

/// Minimal set of resolved used values the layouter may consume.
#[derive(Debug, Clone, Copy)]
pub struct UsedValues {
    pub width: f32,
    pub height: f32,
    pub margin: Edges,
    pub padding: Edges,
}

impl Default for UsedValues {
    fn default() -> Self {
        Self { width: 0.0, height: 0.0, margin: Edges::default(), padding: Edges::default() }
    }
}

/// Resolve a subset of used values from computed style using the provided context.
///
/// Current behavior (placeholder):
/// - width/height: px kept as-is; percentages resolved against containing block; auto → 0.
/// - margin/padding: already px in our simplified pipeline; copied through.
pub fn resolve_used_values(computed: &ComputedStyle, context: & UsedValuesContext) -> UsedValues {
    fn resolve(spec: SizeSpecified, base: f32) -> f32 {
        match spec {
            SizeSpecified::Auto => 0.0,
            SizeSpecified::Px(px) => px,
            SizeSpecified::Percent(p) => p * base,
        }
    }
    let mut width = resolve(computed.width, context.containing_block_width);
    let mut height = resolve(computed.height, context.containing_block_height);
    // Apply min/max clamps if present
    if let Some(minw) = computed.min_width {
        let minw_px = resolve(minw, context.containing_block_width);
        if width < minw_px { width = minw_px; }
    }
    if let Some(maxw) = computed.max_width {
        let maxw_px = resolve(maxw, context.containing_block_width);
        if width > maxw_px { width = maxw_px; }
    }
    if let Some(minh) = computed.min_height {
        let minh_px = resolve(minh, context.containing_block_height);
        if height < minh_px { height = minh_px; }
    }
    if let Some(maxh) = computed.max_height {
        let maxh_px = resolve(maxh, context.containing_block_height);
        if height > maxh_px { height = maxh_px; }
    }
    UsedValues { width, height, margin: computed.margin, padding: computed.padding }
}
