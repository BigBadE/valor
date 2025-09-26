//! CSS Display 3 — §3 Display order and tree-abiding
//! Spec: <https://www.w3.org/TR/css-display-3/#order>
//
use core::hash::BuildHasher;
use css_orchestrator::style_model::ComputedStyle;
use js::NodeKey;
use std::collections::HashMap;

#[inline]
/// Return children in tree-abiding, order-modified document order.
///
/// Spec: CSS Display 3 — §3 Display Order and Tree-Abiding
///   <https://www.w3.org/TR/css-display-3/#order>
///
/// Notes:
/// - This crate’s MVP currently preserves DOM order. Reordering features like `order` for flex/grid
///   are handled within the respective layout modules (flex/grid) and not here.
/// - Callers should first apply §2.5 normalization (`display: none/contents`) via
///   `display::normalize_children` before calling this helper.
pub fn tree_abiding_children<SChildren: BuildHasher, SStyles: BuildHasher>(
    children_by_parent: &HashMap<NodeKey, Vec<NodeKey>, SChildren>,
    _styles: &HashMap<NodeKey, ComputedStyle, SStyles>,
    parent: NodeKey,
) -> Vec<NodeKey> {
    // MVP: preserve DOM order. Future: incorporate order-modified document order as needed
    // by layout models that opt into reordering.
    children_by_parent.get(&parent).cloned().unwrap_or_default()
}

// Tests for §3 are covered by integration fixtures; avoid small unit tests per project policy.
