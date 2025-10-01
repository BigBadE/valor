use crate::snapshots::SnapshotSlice;
use core::hash::BuildHasher;
use css_core::LayoutNodeKind;
use js::NodeKey;
use std::collections::HashMap;

/// Type alias for attribute maps to reduce complexity.
type AttrsMap<S, S2> = HashMap<NodeKey, HashMap<String, String, S2>, S>;

/// Computes the next focusable element in tab order.
///
/// Elements with explicit positive `tabindex` are visited first in numeric order,
/// followed by naturally focusable elements (a, button, input, textarea) in document order.
/// Wraps to the beginning if no next element exists.
///
/// # Returns
///
/// Returns `Some(NodeKey)` for the next focusable element, or `None` if no focusable elements exist.
#[must_use]
pub fn next<S: BuildHasher, S2: BuildHasher>(
    snapshot: SnapshotSlice,
    attrs: &AttrsMap<S, S2>,
    current: Option<NodeKey>,
) -> Option<NodeKey> {
    let mut focusables: Vec<(i32, NodeKey)> = Vec::new();
    let mut natural: Vec<NodeKey> = Vec::new();
    for (key, kind, _children) in snapshot {
        let tabindex_opt = attrs
            .get(key)
            .and_then(|attr_map| attr_map.get("tabindex"))
            .and_then(|tabindex_str| tabindex_str.parse::<i32>().ok());
        let is_focusable_tag = match kind {
            LayoutNodeKind::Block { tag } => {
                let tag_lower = tag.to_ascii_lowercase();
                matches!(tag_lower.as_str(), "a" | "button" | "input" | "textarea")
            }
            _ => false,
        };
        if let Some(tabindex) = tabindex_opt {
            focusables.push((tabindex, *key));
        } else if is_focusable_tag {
            natural.push(*key);
        }
    }
    focusables.sort_by_key(|(tabindex, _)| *tabindex);
    let order: Vec<NodeKey> = if focusables.is_empty() {
        natural
    } else {
        focusables.into_iter().map(|(_, key)| key).collect()
    };
    if order.is_empty() {
        return None;
    }
    Some(current.map_or_else(
        || order[0],
        |current_key| {
            let pos = order
                .iter()
                .position(|key| *key == current_key)
                .unwrap_or(usize::MAX);
            let idx = if pos == usize::MAX || pos + 1 >= order.len() {
                0
            } else {
                pos + 1
            };
            order[idx]
        },
    ))
}

/// Computes the previous focusable element in tab order.
///
/// Elements with explicit positive `tabindex` are visited first in numeric order,
/// followed by naturally focusable elements (a, button, input, textarea) in document order.
/// Wraps to the end if no previous element exists.
///
/// # Returns
///
/// Returns `Some(NodeKey)` for the previous focusable element, or `None` if no focusable elements exist.
#[must_use]
pub fn prev<S: BuildHasher, S2: BuildHasher>(
    snapshot: SnapshotSlice,
    attrs: &AttrsMap<S, S2>,
    current: Option<NodeKey>,
) -> Option<NodeKey> {
    let mut focusables: Vec<(i32, NodeKey)> = Vec::new();
    let mut natural: Vec<NodeKey> = Vec::new();
    for (key, kind, _children) in snapshot {
        let tabindex_opt = attrs
            .get(key)
            .and_then(|attr_map| attr_map.get("tabindex"))
            .and_then(|tabindex_str| tabindex_str.parse::<i32>().ok());
        let is_focusable_tag = match kind {
            LayoutNodeKind::Block { tag } => {
                let tag_lower = tag.to_ascii_lowercase();
                matches!(tag_lower.as_str(), "a" | "button" | "input" | "textarea")
            }
            _ => false,
        };
        if let Some(tabindex) = tabindex_opt {
            focusables.push((tabindex, *key));
        } else if is_focusable_tag {
            natural.push(*key);
        }
    }
    focusables.sort_by_key(|(tabindex, _)| *tabindex);
    let order: Vec<NodeKey> = if focusables.is_empty() {
        natural
    } else {
        focusables.into_iter().map(|(_, key)| key).collect()
    };
    if order.is_empty() {
        return None;
    }
    Some(current.map_or_else(
        || order[order.len() - 1],
        |current_key| {
            let pos = order
                .iter()
                .position(|key| *key == current_key)
                .unwrap_or(usize::MAX);
            let idx = if pos == usize::MAX || pos == 0 {
                order.len() - 1
            } else {
                pos - 1
            };
            order[idx]
        },
    ))
}
