//! CSS Cascading and Inheritance Level 4 — Cascade, inheritance, and computed values.
//! Spec: <https://www.w3.org/TR/css-cascade-4/>

#![forbid(unsafe_code)]

use core::cmp::Ordering;
use css_orchestrator::types::Origin;
use css_selectors::Specificity;

/// Priority tuple used to order declarations in the cascade.
/// Spec: Section 5 — The cascade: sorting
#[derive(Clone, Copy, Debug)]
pub struct CascadePriority {
    /// Spec: Section 3 — Origins and importance
    pub origin: Origin,
    /// Spec: Section 3.3 — Importance
    pub important: bool,
    /// Spec: Section 6 — Specificity
    pub specificity: Specificity,
    /// Source order index, increasing with appearance in stylesheet.
    /// Spec: Section 5 — Source order as final tie-breaker
    pub source_order: u32,
}

impl CascadePriority {
    /// Construct a priority value from inputs.
    /// Spec: Sections 3, 5, 6
    #[inline]
    pub const fn new(
        origin: Origin,
        important: bool,
        specificity: Specificity,
        source_order: u32,
    ) -> Self {
        Self {
            origin,
            important,
            specificity,
            source_order,
        }
    }
}

/// Rank a candidate declaration into a `CascadePriority`.
/// Spec: Sections 3, 5, 6
pub const fn rank_candidate(
    origin: Origin,
    important: bool,
    specificity: Specificity,
    source_order: u32,
) -> CascadePriority {
    CascadePriority::new(origin, important, specificity, source_order)
}

/// Compare two `CascadePriority` values according to the cascade rules.
/// Lower comes first; return `Ordering::Greater` if `left` should win over `right`.
/// Spec: Section 5 — Sorting the cascade
pub fn compare_priority(left: &CascadePriority, right: &CascadePriority) -> Ordering {
    // Importance first (important wins)
    if left.important != right.important {
        return bool_order_desc(left.important, right.important);
    }

    // Origin order: UA < User < Author. Higher origin wins.
    // Map to an integer rank for easy comparison.
    let left_rank = origin_rank(left.origin);
    let right_rank = origin_rank(right.origin);
    if left_rank != right_rank {
        return left_rank.cmp(&right_rank);
    }

    // Specificity: higher wins
    if left.specificity != right.specificity {
        return left.specificity.cmp(&right.specificity);
    }

    // Source order: later wins
    left.source_order.cmp(&right.source_order)
}

/// Return ordering where true > false.
const fn bool_order_desc(a_true_wins: bool, b_true_wins: bool) -> Ordering {
    match (a_true_wins, b_true_wins) {
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        _ => Ordering::Equal,
    }
}

/// Rank origins: UA < User < Author.
const fn origin_rank(origin: Origin) -> i32 {
    match origin {
        Origin::UserAgent => 0,
        Origin::User => 1,
        Origin::Author => 2,
    }
}

/// Whether a property is inherited by default (subset).
/// Spec: Section 7 — Inheritance (MVP subset)
pub fn is_inherited_property(property_name: &str) -> bool {
    matches!(
        property_name.to_ascii_lowercase().as_str(),
        "font-size" | "font-family" | "color"
    )
}

/// Initial values for a subset of properties (string form for MVP glue).
/// Spec: Section 8 — Initial values (subset)
pub fn initial_value(property_name: &str) -> Option<&'static str> {
    match property_name.to_ascii_lowercase().as_str() {
        "font-size" => Some("medium"),
        "font-family" => Some("sans-serif"),
        "color" => Some("black"),
        _ => None,
    }
}

/// Resolve a property's value via inheritance fallback for MVP.
///
/// Returns either the declared value, the parent's value (if inherited), or the initial value.
/// Spec: Section 7 — Inheritance; Section 8 — Initial values
pub fn inherit_property(
    property_name: &str,
    declared_value: Option<String>,
    parent_computed_value: Option<String>,
) -> Option<String> {
    if let Some(value) = declared_value {
        return Some(value);
    }
    if is_inherited_property(property_name) && parent_computed_value.is_some() {
        return parent_computed_value;
    }
    initial_value(property_name).map(ToOwned::to_owned)
}
