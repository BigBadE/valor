//! CSS Display Module Level 3 — Box generation and the display property.
//! Spec: <https://www.w3.org/TR/css-display-3/>

use core::hash::BuildHasher;
use js::NodeKey;
use std::collections::HashMap;
use style_engine::{ComputedStyle, Display};

mod anonymous_blocks;
pub use anonymous_blocks::{AnonymousChildRun, build_anonymous_block_runs};

#[inline]
/// Normalize children for layout by applying a subset of CSS Display 3 box generation rules.
///
/// Spec: CSS Display 3 — Box generation and display types
///   <https://www.w3.org/TR/css-display-3/#box-generation>
///
/// Behavior:
/// - Skip `display: none` subtrees entirely.
/// - Treat `display: contents` nodes as pass-through by lifting their children.
/// - Return other nodes as-is (block/inline/flex). Inline handling is done by the caller.
pub fn normalize_children<SChildren: BuildHasher, SStyles: BuildHasher>(
    children_by_parent: &HashMap<NodeKey, Vec<NodeKey>, SChildren>,
    styles: &HashMap<NodeKey, ComputedStyle, SStyles>,
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
                let mut lifted = normalize_children(children_by_parent, styles, child);
                out.append(&mut lifted);
            }
            _ => out.push(child),
        }
    }
    out
}

#[inline]
/// Normalize children and also compute inline/block runs suitable for anonymous block synthesis.
/// This does not mutate any tree and does not synthesize nodes; it only describes runs.
///
/// Spec: CSS 2.2 §9.4.1 Anonymous block boxes
///   <https://www.w3.org/TR/CSS22/visuren.html#anonymous-block-level>
pub fn normalize_with_anonymous_runs<SChildren: BuildHasher, SStyles: BuildHasher>(
    children_by_parent: &HashMap<NodeKey, Vec<NodeKey>, SChildren>,
    styles: &HashMap<NodeKey, ComputedStyle, SStyles>,
    parent: NodeKey,
) -> (Vec<NodeKey>, Vec<AnonymousChildRun>) {
    let flat = normalize_children(children_by_parent, styles, parent);
    let runs = build_anonymous_block_runs(&flat, styles);
    (flat, runs)
}
