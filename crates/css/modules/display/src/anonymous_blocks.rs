//! Anonymous block synthesis scaffolding.
//! Spec: CSS 2.2 §9.4.1 — Anonymous block boxes
//!   <https://www.w3.org/TR/CSS22/visuren.html#anonymous-block-level>

use core::hash::BuildHasher;
use js::NodeKey;
use std::collections::HashMap;
use style_engine::{ComputedStyle, Display};

/// Kind of anonymous-block-related run found during analysis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AnonymousRunKind {
    /// A contiguous run of inline-level children that would be wrapped in an anonymous block.
    InlineRun,
    /// A single block-level child (delimiter between inline runs).
    Block,
}

/// Describes a contiguous run of children for anonymous-block synthesis.
#[derive(Clone, Debug)]
pub struct AnonymousChildRun {
    /// Start (inclusive) index into the flattened children list.
    pub start: usize,
    /// End (exclusive) index into the flattened children list.
    pub end: usize,
    /// Kind of the run.
    pub kind: AnonymousRunKind,
}

#[inline]
/// Compute anonymous block candidate runs from a flattened list of child nodes.
/// This is a pure analysis pass: it does not synthesize nodes.
///
/// Rules (CSS 2.2 §9.4.1): inline-level children adjacent to block-level children
/// are wrapped into anonymous block boxes so block formatting can proceed.
pub fn build_anonymous_block_runs<S: BuildHasher>(
    flat_children: &[NodeKey],
    styles: &HashMap<NodeKey, ComputedStyle, S>,
) -> Vec<AnonymousChildRun> {
    let mut out: Vec<AnonymousChildRun> = Vec::new();
    if flat_children.is_empty() {
        return out;
    }
    let mut index = 0;
    let len = flat_children.len();
    while index < len {
        let Some(&key_current) = flat_children.get(index) else {
            break;
        };
        let is_block_level = styles.get(&key_current).is_none_or(|style_ref| {
            matches!(
                style_ref.display,
                Display::Block | Display::Flex | Display::InlineFlex
            )
        });
        if is_block_level {
            out.push(AnonymousChildRun {
                start: index,
                end: index.saturating_add(1),
                kind: AnonymousRunKind::Block,
            });
            index = index.saturating_add(1);
            continue;
        }
        // Inline run
        let start_index = index;
        index = index.saturating_add(1);
        while index < len {
            let Some(&key_iter) = flat_children.get(index) else {
                break;
            };
            let iter_is_block = styles.get(&key_iter).is_none_or(|style_ref| {
                matches!(
                    style_ref.display,
                    Display::Block | Display::Flex | Display::InlineFlex
                )
            });
            if iter_is_block {
                break;
            }
            index = index.saturating_add(1);
        }
        out.push(AnonymousChildRun {
            start: start_index,
            end: index,
            kind: AnonymousRunKind::InlineRun,
        });
    }
    out
}
