//! Spec: CSS Display 3 — §2.3 Generating Marker Boxes: the `list-item` keyword
//! <https://www.w3.org/TR/css-display-3/#list-items>
//!
//! MVP scaffolding: `style_engine` does not currently expose a `list-item` display value,
//! and marker/counter handling lives outside this module. This file provides a
//! future-proof integration seam that callers can invoke without branching on
//! engine details.

use style_engine::ComputedStyle;

#[inline]
/// Return true if the element should behave as a list-item (i.e., generate a `::marker`).
///
/// MVP: returns false until style computation exposes `list-item`. When that lands,
/// this function will switch to checking the computed display and wiring marker synthesis.
pub const fn maybe_list_item_child(_style: &ComputedStyle) -> bool {
    false
}
