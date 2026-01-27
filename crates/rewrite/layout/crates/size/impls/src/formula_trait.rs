//! Trait for querying formulas from the size system.
//!
//! This allows layout modules like flex to build formulas that reference
//! size computations without creating circular dependencies.

use rewrite_core::{Axis, Formula, ScopedDb};

/// Trait for querying size formulas.
///
/// Implemented by the size system to provide formulas for various size queries.
/// Layout modules (flex, grid, etc.) use this trait to build formulas that
/// reference sizes without directly depending on the size implementation.
pub trait SizeFormulaProvider {
    /// Get the formula for computing an element's size on the given axis.
    fn size_formula(&self, scoped: &mut ScopedDb, axis: Axis) -> &'static Formula;

    /// Get the formula for computing an element's padding on the given edge.
    fn padding_formula(&self, scoped: &mut ScopedDb, edge: rewrite_core::Edge) -> &'static Formula;

    /// Get the formula for computing an element's border width on the given edge.
    fn border_formula(&self, scoped: &mut ScopedDb, edge: rewrite_core::Edge) -> &'static Formula;

    /// Get the formula for computing an element's margin on the given edge.
    fn margin_formula(&self, scoped: &mut ScopedDb, edge: rewrite_core::Edge) -> &'static Formula;
}

/// Trait for querying offset formulas.
///
/// Similar to SizeFormulaProvider but for position/offset queries.
pub trait OffsetFormulaProvider {
    /// Get the formula for computing an element's offset on the given axis.
    fn offset_formula(&self, scoped: &mut ScopedDb, axis: rewrite_core::Axis) -> &'static Formula;
}
