//! CSS selector match caching.
//! Spec: Section 3 — Matching is stable unless DOM or attributes change

use crate::{Combinator, ComplexSelector};
use core::hash::{Hash as _, Hasher as _};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;

/// A simple per-element, per-selector cache.
/// Users should call `invalidate_for_element` on attribute/class/id changes.
/// Spec: Section 3 — Matching is stable unless DOM or attributes change
#[derive(Default)]
pub struct MatchCache {
    /// Per-element per-selector memoized match results.
    store: HashMap<(u64, u64), bool>,
}

impl MatchCache {
    /// Cache a result.
    /// Spec: Section 3 — Caching is an optimization and not mandated by spec.
    #[inline]
    pub fn set(&mut self, element_key: u64, selector_key: u64, matched: bool) {
        self.store.insert((element_key, selector_key), matched);
    }

    /// Get a cached result.
    #[inline]
    pub fn get(&self, element_key: u64, selector_key: u64) -> Option<bool> {
        self.store.get(&(element_key, selector_key)).copied()
    }

    /// Invalidate cached results for an element (e.g., when class/attr/id changes).
    #[inline]
    pub fn invalidate_for_element(&mut self, element_key: u64) {
        self.store
            .retain(|&(element_key2, _), _| element_key2 != element_key);
    }
}

/// Build a stable key for a selector to use with `MatchCache`.
/// In production you might hash the serialized form. Here we use a simple hasher.
pub fn calc_selector_key(sel: &ComplexSelector) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Stable hash of the structure
    for simple in &sel.first.simples {
        simple.hash(&mut hasher);
    }
    for pair in &sel.rest {
        let combinator = pair.0;
        let comp = &pair.1;
        match combinator {
            Combinator::Descendant => 0u8.hash(&mut hasher),
            Combinator::Child => 1u8.hash(&mut hasher),
            Combinator::AdjacentSibling => 2u8.hash(&mut hasher),
            Combinator::GeneralSibling => 3u8.hash(&mut hasher),
        }
        for simple2 in &comp.simples {
            simple2.hash(&mut hasher);
        }
    }
    hasher.finish()
}
