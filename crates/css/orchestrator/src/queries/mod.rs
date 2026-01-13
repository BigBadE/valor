//! Query-based style computation.
//!
//! This module provides query implementations for the style system,
//! enabling automatic memoization and incremental recomputation.

pub mod dom_inputs;
pub mod style_queries;

pub use dom_inputs::{
    DomAttributesInput, DomChildrenInput, DomClassesInput, DomIdInput, DomParentInput, DomTagInput,
    DomTextInput,
};
pub use style_queries::{
    ComputedStyleQuery, InheritedStyleQuery, MatchingRulesQuery, StylesheetInput,
};
