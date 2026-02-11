//! Selectors Level 3 — Element matching and specificity.
//! Spec: <https://www.w3.org/TR/selectors-3/>
//!
//! This module implements a minimal subset needed for Speedometer:
//! - Type, class, id, and attribute equals selectors
//! - Combinators: descendant, child, adjacent sibling, general sibling
//! - Specificity calculation
//! - A simple match cache that can be invalidated on element changes
//!
//! Each function includes a reference to its corresponding section in the spec.

mod cache;
mod matcher;
mod parser;
mod specificity;

// Re-export public API
pub use cache::{MatchCache, calc_selector_key};
pub use matcher::{matches_complex, matches_compound, matches_selector_list};
pub use parser::{parse_complex_selector, parse_selector_list};
pub use specificity::{Specificity, specificity_of_complex, specificity_of_compound};

/// An adapter that abstracts DOM access for selector matching.
/// Implement this for your DOM layer.
///
/// Spec references:
/// - Section 3: Selectors overview and element matching
pub trait ElementAdapter {
    type Handle: Copy + Eq;

    /// Unique, stable key for caching per element.
    /// Spec: Section 3 — used only for caching purposes here.
    fn unique_key(&self, element: Self::Handle) -> u64;

    /// Parent element if any.
    /// Spec: Section 11 — Combinators (for tree relationships)
    fn parent(&self, element: Self::Handle) -> Option<Self::Handle>;

    /// Previous sibling element (skip non-elements if your DOM has mixed nodes).
    /// Spec: Section 11 — Sibling combinators
    fn previous_sibling_element(&self, element: Self::Handle) -> Option<Self::Handle>;

    /// Tag name in ASCII lowercase (per HTML parsing conventions).
    /// Spec: Section 5 — Type selectors
    fn tag_name(&self, element: Self::Handle) -> &str;

    /// Returns Some(id) if the element has an id attribute, else None.
    /// Spec: Section 7 — ID selectors
    fn element_id(&self, element: Self::Handle) -> Option<&str>;

    /// True if the element has the given class token.
    /// Spec: Section 6 — Class selectors
    fn has_class(&self, element: Self::Handle, class: &str) -> bool;

    /// Returns the attribute value if present.
    /// Spec: Section 8 — Attribute selectors
    fn attr(&self, element: Self::Handle, name: &str) -> Option<&str>;
}

/// Simple selectors (subset).
/// Spec: Section 5, 6, 7, 8
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SimpleSelector {
    /// Spec: Section 5 — Type selectors
    Type(String),
    /// Spec: Section 6 — Class selectors
    Class(String),
    /// Spec: Section 7 — ID selectors
    IdSelector(String),
    /// Spec: Section 8 — Attribute selectors [attr=value]
    AttrEquals { name: String, value: String },
    /// Universal selector '*'. For simplicity we parse but it's a no-op match.
    /// Spec: Section 5 — Universal selector
    Universal,
}

/// A compound selector is a sequence of simple selectors (no combinators).
/// Spec: Section 5 — Simple selector sequences
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CompoundSelector {
    pub simples: Vec<SimpleSelector>,
}

/// Combinators between compounds.
/// Spec: Section 11 — Combinators
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Combinator {
    Descendant,
    Child,
    AdjacentSibling,
    GeneralSibling,
}

/// A complex selector is one or more compounds separated by combinators.
/// Spec: Section 3, 11
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ComplexSelector {
    pub first: CompoundSelector,
    pub rest: Vec<(Combinator, CompoundSelector)>,
}

/// A selector list separated by commas.
/// Spec: Section 4 — Groups of selectors
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SelectorList {
    pub selectors: Vec<ComplexSelector>,
}
