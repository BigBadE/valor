//! CSS Display Module Level 3 — Box generation and the display property.
//! Spec: <https://www.w3.org/TR/css-display-3/>

use core::hash::BuildHasher;
use css_orchestrator::style_model::ComputedStyle;
use js::NodeKey;
use std::collections::HashMap;

mod anonymous_blocks;
pub use anonymous_blocks::{AnonymousChildRun, build_anonymous_block_runs};
mod inline_context;
pub use inline_context::{
    InlineFragment, LineBox, build_inline_context, build_inline_context_with_filter,
};

// Chapter modules mapped to the Display 3 spec structure.
// Spec: §2 — Box layout modes and the display property
#[path = "2_box_layout_modes/mod.rs"]
mod chapter2;
// Spec: §3 — Display order
#[path = "3_display_order/mod.rs"]
mod chapter3;
// Spec: §4 — Visibility
#[path = "4_visibility/mod.rs"]
mod chapter4;

/// Normalize children for layout by applying a subset of CSS Display 3 box generation rules.
///
/// Spec: CSS Display 3 — §2.5 Box Generation: the `none` and `contents` keywords
///   <https://www.w3.org/TR/css-display-3/#box-generation>
///   - Delegates to `chapter2::part_2_5_box_generation::normalize_children`
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
    // Delegate to §2 implementation.
    chapter2::part_2_5_box_generation::normalize_children(children_by_parent, styles, parent)
}

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
    let parent_style_opt = styles.get(&parent);
    let runs = build_anonymous_block_runs(&flat, styles, parent_style_opt);
    (flat, runs)
}
