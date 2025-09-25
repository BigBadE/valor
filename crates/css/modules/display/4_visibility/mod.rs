//! CSS Display 3 — §4 Visibility
//! Spec: <https://www.w3.org/TR/css-display-3/#visibility>
//
use css_orchestrator::style_model::ComputedStyle;

#[inline]
/// Determine whether a box should be considered visible for layout purposes.
///
/// Spec: CSS Display 3 — §4 Visibility
///   <https://www.w3.org/TR/css-display-3/#visibility>
///
/// Notes:
/// - For MVP, layout is unaffected by `visibility` (paint-time feature). Our style engine does
///   not expose a `visibility` property yet. This helper returns `true` and serves as a single
///   integration point to adapt once the property is available.
pub const fn is_visible_for_layout(_: &ComputedStyle) -> bool {
    // MVP: treat everything that reached layout as visible.
    true
}
