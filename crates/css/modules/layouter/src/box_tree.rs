//! Display/box-tree helpers for the layouter.
//!
//! This module provides small utilities to build a display-aware child list and
//! whitespace collapsing for inline text. It implements a subset of CSS Display
//! 3 sufficient for our block layout MVP.

use js::NodeKey;
use std::collections::HashMap;
use style_engine::{ComputedStyle, Display};

/// Flatten children for layout by applying a subset of CSS Display box generation rules.
/// - Skip display:none subtrees entirely.
/// - Treat display:contents nodes as pass-through by lifting their children.
/// - Return other nodes as-is (block/inline/flex). Inline handling is done by the caller.
pub fn flatten_display_children(
    children_by_parent: &HashMap<NodeKey, Vec<NodeKey>>,
    styles: &HashMap<NodeKey, ComputedStyle>,
    parent: NodeKey,
) -> Vec<NodeKey> {
    let Some(children) = children_by_parent.get(&parent).cloned() else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(children.len());
    for child in children {
        let display = styles
            .get(&child)
            .cloned()
            .unwrap_or_else(ComputedStyle::default)
            .display;
        match display {
            Display::None => {
                // Skip entirely
            }
            Display::Contents => {
                // Lift grandchildren
                let mut lifted = flatten_display_children(children_by_parent, styles, child);
                out.append(&mut lifted);
            }
            _ => out.push(child),
        }
    }
    out
}

// Inline whitespace collapsing is not used by the current block-only layout path.
