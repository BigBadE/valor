//! Tree traversal and document utilities used by the orchestrator.

use crate::{LayoutNodeKind, Layouter};
use js::NodeKey;

/// Returns true if the node has any inline text descendant.
pub fn has_inline_text_descendant(layouter: &Layouter, key: NodeKey) -> bool {
    // Check direct children first
    if let Some(children) = layouter.children.get(&key) {
        for child_key in children {
            if matches!(
                layouter.nodes.get(child_key),
                Some(LayoutNodeKind::InlineText { .. })
            ) {
                return true;
            }
            // Recursively check descendants
            if has_inline_text_descendant(layouter, *child_key) {
                return true;
            }
        }
    }
    false
}

/// Choose the layout root. Prefer `body` under `html` when present; otherwise first block.
pub fn choose_layout_root(layouter: &Layouter) -> Option<NodeKey> {
    // Look for html element
    let mut html_key: Option<NodeKey> = None;
    for (key, kind) in &layouter.nodes {
        if let LayoutNodeKind::Block { tag } = kind {
            if tag == "html" {
                html_key = Some(*key);
                break;
            }
        }
    }

    // If we found html, look for body under it
    if let Some(html) = html_key {
        if let Some(children) = layouter.children.get(&html) {
            for child_key in children {
                if let Some(LayoutNodeKind::Block { tag }) = layouter.nodes.get(child_key) {
                    if tag == "body" {
                        return Some(*child_key);
                    }
                }
            }
        }
        // If no body, use html
        return Some(html);
    }

    // Otherwise, find first block-level element that's not the document root
    for (key, kind) in &layouter.nodes {
        if *key == NodeKey::ROOT {
            continue;
        }
        if matches!(kind, LayoutNodeKind::Block { .. }) {
            return Some(*key);
        }
    }

    None
}
