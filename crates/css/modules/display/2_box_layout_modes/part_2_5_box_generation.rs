//! Spec: CSS Display 3 — §2 Box layout, generation, and the display property
//! <https://www.w3.org/TR/css-display-3/#box-layout>

use crate::chapter2::part_2_3_list_items::maybe_list_item_child;
use crate::chapter3::tree_abiding_children;
use crate::chapter4::is_visible_for_layout;
use core::hash::BuildHasher;
use css_orchestrator::style_model::{ComputedStyle, Display};
use js::NodeKey;
use std::collections::HashMap;

/// Normalize children for layout by applying a subset of CSS Display 3 box generation rules.
///
/// Spec: CSS Display 3 — §2.5 Box Generation: the `none` and `contents` keywords
///   <https://www.w3.org/TR/css-display-3/#box-generation>
#[inline]
pub fn normalize_children<SChildren: BuildHasher, SStyles: BuildHasher>(
    children_by_parent: &HashMap<NodeKey, Vec<NodeKey>, SChildren>,
    styles: &HashMap<NodeKey, ComputedStyle, SStyles>,
    parent: NodeKey,
) -> Vec<NodeKey> {
    // §3 Display order and tree-abiding: obtain children in order-modified document order.
    // MVP preserves DOM order.
    let children = tree_abiding_children(children_by_parent, styles, parent);
    if children.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(children.len());
    for child in children {
        let style = styles
            .get(&child)
            .cloned()
            .unwrap_or_else(ComputedStyle::default);
        if !is_visible_for_layout(&style) {
            // §4 Visibility (hook): invisible boxes still affect layout in many engines, but
            // for MVP our is_visible_for_layout always returns true; this call is a future hook.
            // If it ever returns false, skip as a conservative default.
            continue;
        }
        // §2.3 list-item seam: allow marker plumbing to hook in without branching here.
        let _is_list_item = maybe_list_item_child(&style);
        let display = style.display;
        match display {
            Display::None => {
                // Spec: §2.7 Box generation: display:none generates no box; skip subtree.
            }
            Display::Contents => {
                // Spec: §2.7 Box generation: display:contents participates via its children; lift grandchildren.
                let mut lifted = normalize_children(children_by_parent, styles, child);
                out.append(&mut lifted);
            }
            _ => out.push(child),
        }
    }
    out
}
