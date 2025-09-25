use crate::snapshots::SnapshotSlice;
use css_core::LayoutNodeKind;
use js::NodeKey;
use std::collections::HashMap;

pub fn next(
    snapshot: SnapshotSlice,
    attrs: &HashMap<NodeKey, HashMap<String, String>>,
    current: Option<NodeKey>,
) -> Option<NodeKey> {
    let mut focusables: Vec<(i32, NodeKey)> = Vec::new();
    let mut natural: Vec<NodeKey> = Vec::new();
    for (key, kind, _children) in snapshot.iter() {
        let tabindex_opt = attrs
            .get(key)
            .and_then(|m| m.get("tabindex"))
            .and_then(|s| s.parse::<i32>().ok());
        let is_focusable_tag = match kind {
            LayoutNodeKind::Block { tag } => {
                let t = tag.to_ascii_lowercase();
                matches!(t.as_str(), "a" | "button" | "input" | "textarea")
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
    let order: Vec<NodeKey> = if !focusables.is_empty() {
        focusables.into_iter().map(|(_, key)| key).collect()
    } else {
        natural
    };
    if order.is_empty() {
        return None;
    }
    Some(match current {
        None => order[0],
        Some(current_key) => {
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
        }
    })
}

pub fn prev(
    snapshot: SnapshotSlice,
    attrs: &HashMap<NodeKey, HashMap<String, String>>,
    current: Option<NodeKey>,
) -> Option<NodeKey> {
    let mut focusables: Vec<(i32, NodeKey)> = Vec::new();
    let mut natural: Vec<NodeKey> = Vec::new();
    for (key, kind, _children) in snapshot.iter() {
        let tabindex_opt = attrs
            .get(key)
            .and_then(|m| m.get("tabindex"))
            .and_then(|s| s.parse::<i32>().ok());
        let is_focusable_tag = match kind {
            LayoutNodeKind::Block { tag } => {
                let t = tag.to_ascii_lowercase();
                matches!(t.as_str(), "a" | "button" | "input" | "textarea")
            }
            _ => false,
        };
        if let Some(tb) = tabindex_opt {
            focusables.push((tb, *key));
        } else if is_focusable_tag {
            natural.push(*key);
        }
    }
    focusables.sort_by_key(|(tb, _)| *tb);
    let order: Vec<NodeKey> = if !focusables.is_empty() {
        focusables.into_iter().map(|(_, k)| k).collect()
    } else {
        natural
    };
    if order.is_empty() {
        return None;
    }
    Some(match current {
        None => order[order.len() - 1],
        Some(cur) => {
            let pos = order
                .iter()
                .position(|key| *key == cur)
                .unwrap_or(usize::MAX);
            let idx = if pos == usize::MAX || pos == 0 {
                order.len() - 1
            } else {
                pos - 1
            };
            order[idx]
        }
    })
}
