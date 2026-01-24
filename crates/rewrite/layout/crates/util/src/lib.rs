//! Layout utility types and traits for the dispatch system.
//!
//! This crate defines the core abstractions that allow layout modules to call
//! each other without circular crate dependencies.

/// Layout type enumeration - specifies which layout algorithm to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayoutType {
    /// Flexbox layout
    Flex,
    /// CSS Grid layout
    Grid,
    /// Block layout (normal flow)
    Block,
    /// Inline layout
    Inline,
    /// Float layout
    Float,
}

/// Axis enumeration - specifies inline or block direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, rewrite_macros::Markers)]
pub enum Axis {
    /// Inline axis (horizontal in horizontal writing mode)
    Inline,
    /// Block axis (vertical in horizontal writing mode)
    Block,
}

/// Dispatcher trait - provides the ability to dispatch queries.
///
/// Implemented by dispatcher types to route queries to the appropriate layout module.
/// Also passed to layout modules so they can recursively query.
pub trait Dispatcher<T> {
    /// The type returned by queries.
    type Returns;

    /// Perform a query with a scoped database context.
    /// The dispatcher queries display property from ScopedDb to determine layout type.
    fn query(scoped: &mut rewrite_core::ScopedDb, param: T) -> Self::Returns;
}
