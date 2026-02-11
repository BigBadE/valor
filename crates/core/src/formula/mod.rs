//! Formula computation graphs for layout values.
//!
//! This module provides:
//! - `GenericFormula<T>`: Computation graph parameterized by a styler type
//! - `GenericFormulaList<T>`: Multi-value sources from related nodes
//! - `ResolveContext<T>`: Memoized evaluation with relationship resolution
//!
//! Key design:
//! - Formulas are pure arithmetic over values from self/parent/children
//! - The styler type `T` provides CSS property access and tree navigation
//! - Concrete formula types are defined by the CSS crate as type aliases

mod resolver;

pub use resolver::{ResolveContext, StylerAccess};

use lightningcss::properties::PropertyId;

use crate::{MultiRelationship, SingleRelationship, Subpixel};

// ============================================================================
// Operations
// ============================================================================

/// Arithmetic operations for formulas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Add,
    Sub,
    Mul,
    Div,
}

// ============================================================================
// Query function type
// ============================================================================

/// Query function type - takes a styler reference and returns a formula.
/// Returns None if the query lacks confidence (missing CSS properties).
pub type QueryFn<T> = fn(&T) -> Option<&'static GenericFormula<T>>;

// ============================================================================
// Formula List
// ============================================================================

/// A source of multiple values from related nodes.
pub enum GenericFormulaList<T: 'static> {
    /// Run a query on each node in a multi-relationship.
    Related(MultiRelationship, QueryFn<T>),

    /// Evaluate a formula on each related node.
    Map(MultiRelationship, &'static GenericFormula<T>),
}

/// Aggregation operations that reduce a list to a single value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aggregation {
    Sum,
    Max,
    Min,
    Average,
}

// ============================================================================
// Formula - Computation graph
// ============================================================================

/// A formula describing how to compute a layout value.
///
/// Generic over `T`, the styler type that provides CSS property access
/// and tree navigation. The CSS crate defines a concrete type alias.
pub enum GenericFormula<T: 'static> {
    // ========================================================================
    // Leaf values
    // ========================================================================
    /// A constant value.
    Constant(Subpixel),

    /// Read a CSS property value (needs unit conversion).
    CssValue(PropertyId<'static>),

    /// Read a CSS property value, returning a default if unset.
    /// Used for properties like padding/border whose CSS initial value is 0.
    CssValueOrDefault(PropertyId<'static>, Subpixel),

    /// Run a query on a related node to get a formula, then resolve it
    /// in that node's context.
    Related(SingleRelationship, QueryFn<T>),

    /// Aggregate a list into a single value.
    Aggregate(Aggregation, &'static GenericFormulaList<T>),

    /// Count of items in a list.
    Count(&'static GenericFormulaList<T>),

    // ========================================================================
    // Structural values
    // ========================================================================
    /// Index of the current node among its siblings (0-based).
    SiblingIndex,

    // ========================================================================
    // Viewport values
    // ========================================================================
    /// The viewport width in pixels.
    ViewportWidth,

    /// The viewport height in pixels.
    ViewportHeight,

    // ========================================================================
    // Arithmetic operations
    // ========================================================================
    /// Binary operation: a op b
    Op(
        Operation,
        &'static GenericFormula<T>,
        &'static GenericFormula<T>,
    ),
}
