//! Tree utilities: traversal and document utilities
//!
//! Spec: CSS 2.2 §9.4.1 and related tree handling

use crate::{LayoutNodeKind, Layouter};
use js::NodeKey;

#[inline]
/// Return the tag name for a block node, or `None` if the node is not a block.
///
/// Spec: CSS 2.2 §9.4.1 — identify element boxes participating in BFC.
pub fn tag_of(layouter: &Layouter, key: NodeKey) -> Option<String> {
    let kind = layouter.nodes.get(&key)?.clone();
    match kind {
        LayoutNodeKind::Block { tag } => Some(tag),
        _ => None,
    }
}

#[inline]
/// Find the first block-level node under `start` using a depth-first search.
///
/// Spec: CSS 2.2 §9.4.1 — block formatting.
pub fn find_first_block_under(layouter: &Layouter, start: NodeKey) -> Option<NodeKey> {
    if matches!(
        layouter.nodes.get(&start),
        Some(&LayoutNodeKind::Block { .. })
    ) {
        return Some(start);
    }
    if let Some(child_list) = layouter.children.get(&start) {
        for child_key in child_list {
            if let Some(found) = find_first_block_under(layouter, *child_key) {
                return Some(found);
            }
        }
    }
    None
}

#[inline]
/// Returns true if the subtree under `key` contains any inline text nodes with non-empty text.
pub fn has_inline_text_descendant(layouter: &Layouter, key: NodeKey) -> bool {
    if let Some(children) = layouter.children.get(&key) {
        for child in children {
            if layouter
                .text_by_node
                .get(child)
                .is_some_and(|text| !text.is_empty())
            {
                return true;
            }
            if has_inline_text_descendant(layouter, *child) {
                return true;
            }
        }
    }
    false
}

#[inline]
/// Choose the layout root. Prefer `body` under `html` when present; otherwise first block.
pub fn choose_layout_root(layouter: &Layouter) -> Option<NodeKey> {
    // Find the document root, then prefer body under html when present.
    let doc_key = layouter
        .nodes
        .iter()
        .find_map(|(key, kind)| matches!(kind, &LayoutNodeKind::Document).then_some(*key))?;
    let first_block = find_first_block_under(layouter, doc_key)?;
    let is_html = tag_of(layouter, first_block).is_some_and(|tag| tag.eq_ignore_ascii_case("html"));
    if !is_html {
        return Some(first_block);
    }
    if let Some(children) = layouter.children.get(&first_block) {
        for child in children {
            let is_body =
                tag_of(layouter, *child).is_some_and(|tag| tag.eq_ignore_ascii_case("body"));
            if is_body {
                return Some(*child);
            }
        }
    }
    Some(first_block)
}
