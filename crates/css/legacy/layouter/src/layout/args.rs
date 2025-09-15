//! Argument and lookup structures used during layout computation.

use js::NodeKey;
use std::collections::HashMap;
use style_engine::ComputedStyle;
use crate::LayoutNodeKind;

/// Shared geometry and typography parameters for a layout pass.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ComputeGeomArgs {
    /// Viewport content width in CSS pixels used for block formatting.
    pub viewport_width: i32,
    /// Body margin in CSS pixels (applied symmetrically horizontally).
    pub body_margin: i32,
    /// Default line height in CSS pixels used when style is unavailable.
    pub line_height: i32,
    /// Approximate glyph width used for simple text measurement.
    pub char_width: i32,
    /// Vertical gap between stacked blocks.
    pub v_gap: i32,
}

/// Read-only maps used by the layout algorithm to access the mirrored DOM.
pub(crate) struct LayoutMaps<'a> {
    /// Node kind by key.
    pub kind_by_key: &'a HashMap<NodeKey, LayoutNodeKind>,
    /// Children list by key.
    pub children_by_key: &'a HashMap<NodeKey, Vec<NodeKey>>,
    /// Optional computed style map (absent when style engine is disabled in tests).
    pub computed_by_key: Option<&'a HashMap<NodeKey, ComputedStyle>>,
    /// Attributes by node key (lowercased names), used for things like <img alt> heuristics.
    pub attrs_by_key: &'a HashMap<NodeKey, HashMap<String, String>>,
}
