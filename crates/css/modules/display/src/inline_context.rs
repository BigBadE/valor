//! Inline formatting context (MVP): line box grouping and inline fragments
//! Spec: CSS 2.2 §9.4.2 — Inline formatting context; CSS Display 3 §2
//!   <https://www.w3.org/TR/CSS22/visuren.html#inline-formatting>
//!   <https://www.w3.org/TR/css-display-3/>

use crate::chapter2::part_2_1_outer_inner::is_block_level_outer;
use crate::chapter2::part_2_7_transformations::used_display_for_child;
use core::hash::BuildHasher;
use css_orchestrator::style_model::ComputedStyle;
use js::NodeKey;
use std::collections::HashMap;

/// A minimal inline fragment representing a single inline-level child.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineFragment {
    pub node: NodeKey,
}

/// A minimal line box consisting of a sequence of inline fragments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineBox {
    pub fragments: Vec<InlineFragment>,
}

/// Build line boxes from a flat list of children by grouping contiguous inline-level items.
/// This variant accepts a predicate to skip nodes (e.g., ignorable whitespace text runs).
///
/// MVP behavior:
/// - Produces one `LineBox` per contiguous inline run after applying `skip_predicate`.
/// - Block-level items break lines and are excluded from the inline context result.
/// - Whitespace collapsing can be implemented by passing a predicate that returns true for
///   ignorable whitespace text nodes.
pub fn build_inline_context_with_filter<S, F>(
    flat_children: &[NodeKey],
    styles: &HashMap<NodeKey, ComputedStyle, S>,
    parent_style: Option<&ComputedStyle>,
    skip_predicate: F,
) -> Vec<LineBox>
where
    S: BuildHasher,
    F: Fn(NodeKey) -> bool,
{
    let mut out: Vec<LineBox> = Vec::new();
    if flat_children.is_empty() {
        return out;
    }
    let mut current_line: Vec<InlineFragment> = Vec::new();
    for node in flat_children {
        if skip_predicate(*node) {
            continue;
        }
        let is_block = styles.get(node).is_none_or(|style_ref| {
            let used = used_display_for_child(style_ref, parent_style, false);
            is_block_level_outer(used)
        });
        if is_block {
            if !current_line.is_empty() {
                out.push(LineBox {
                    fragments: current_line,
                });
                current_line = Vec::new();
            }
            continue;
        }
        current_line.push(InlineFragment { node: *node });
    }
    if !current_line.is_empty() {
        out.push(LineBox {
            fragments: current_line,
        });
    }
    out
}

/// Build line boxes from a flat list of children by grouping contiguous inline-level items.
///
/// MVP behavior:
/// - Produces one `LineBox` per contiguous inline run.
/// - Block-level items break lines and are excluded from the inline context result.
/// - Whitespace collapsing and text shaping are out of scope here; handled by higher layers.
pub fn build_inline_context<S: BuildHasher>(
    flat_children: &[NodeKey],
    styles: &HashMap<NodeKey, ComputedStyle, S>,
    parent_style: Option<&ComputedStyle>,
) -> Vec<LineBox> {
    // Default predicate: do not skip any nodes.
    build_inline_context_with_filter(flat_children, styles, parent_style, |_| false)
}

// Tests covered by fixtures; avoid small unit tests per project policy.
