//! Display/box-tree helpers for the layouter.
//!
//! This module provides small utilities to build a display-aware child list and
//! whitespace collapsing for inline text. It implements a subset of CSS Display
//! 3 sufficient for our block layout MVP.

use js::NodeKey;
use std::collections::HashMap;
use style_engine::ComputedStyle;

// Use the production display normalization from the display module.
use css_display::normalize_children as display_normalize_children;

/// Flatten children for layout by applying a subset of CSS Display box generation rules.
/// - Skip display:none subtrees entirely.
/// - Treat display:contents nodes as pass-through by lifting their children.
/// - Return other nodes as-is (block/inline/flex). Inline handling is done by the caller.
pub fn flatten_display_children(
    children_by_parent: &HashMap<NodeKey, Vec<NodeKey>>,
    styles: &HashMap<NodeKey, ComputedStyle>,
    parent: NodeKey,
) -> Vec<NodeKey> {
    display_normalize_children(children_by_parent, styles, parent)
}

// Inline whitespace collapsing is not used by the current block-only layout path.
