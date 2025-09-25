//! Spec: CSS Display 3 — §2 Box layout, generation, and the display property
//! <https://www.w3.org/TR/css-display-3/#box-layout>

use core::hash::BuildHasher;
use js::NodeKey;
use std::collections::HashMap;
use style_engine::{ComputedStyle, Display};

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
