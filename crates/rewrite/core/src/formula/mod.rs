//! Formula computation graphs for layout values.
//!
//! This module provides:
//! - `Formula`: Computation graph for calculating layout values
//! - `PropId`: Unique identifiers for CSS properties
//!
//! Key design:
//! - Formulas are pure arithmetic over values from self/parent/children
//! - Everything is const - built at compile time, no runtime allocation

use crate::{CssValueProperty, MultiRelationship, SingleRelationship};

// ============================================================================
// Formula List Operations
// ============================================================================

/// Operations over lists of related nodes.
#[derive(Debug, Clone, Copy)]
pub enum FormulaList {
    /// Read a CSS value property from multiple related nodes (returns a list).
    RelatedValue(MultiRelationship, CssValueProperty),

    /// Sum of a theorem evaluated on related nodes.
    Sum(&'static Formula),

    /// Maximum of a theorem evaluated on related nodes.
    Max(&'static Formula),

    /// Minimum of a theorem evaluated on related nodes.
    Min(&'static Formula),

    /// Average of a theorem evaluated on related nodes.
    Average(&'static Formula),
}

// ============================================================================
// Formula - Computation graph
// ============================================================================

/// A formula describing how to compute a layout value.
///
/// Formulas are evaluated against a specific node to produce a concrete value.
/// They can reference:
/// - CSS properties of the node
/// - Results of other queries on self/parent/children
/// - Structural values like children count or sibling index
#[derive(Debug, Clone, Copy)]
pub enum Formula {
    // ========================================================================
    // Leaf values
    // ========================================================================
    /// A constant value.
    Constant(i32),

    /// Read a CSS value property.
    Value(CssValueProperty),

    /// Read a CSS value property from a single related node.
    RelatedValue(SingleRelationship, CssValueProperty),

    /// Apply a list operation.
    List(FormulaList),

    /// Count of related nodes.
    Count(FormulaList),

    // ========================================================================
    // Structural values
    // ========================================================================
    /// Index of the current node among its siblings (0-based).
    SiblingIndex,

    // ========================================================================
    // Arithmetic operations
    // ========================================================================
    /// Addition: a + b
    Add(&'static Formula, &'static Formula),

    /// Subtraction: a - b
    Sub(&'static Formula, &'static Formula),

    /// Multiplication: a * b
    Mul(&'static Formula, &'static Formula),

    /// Division: a / b (returns 0 if b is 0)
    Div(&'static Formula, &'static Formula),
}

// ============================================================================
// Formula constructors
// ============================================================================

impl Formula {
    /// Create a constant formula.
    pub const fn constant(value: i32) -> Self {
        Formula::Constant(value)
    }

    /// Create a formula that reads a CSS value property.
    pub const fn value(prop: CssValueProperty) -> Self {
        Formula::Value(prop)
    }

    /// Create a formula that reads a CSS value property from a single related node.
    pub const fn related_value(rel: SingleRelationship, prop: CssValueProperty) -> Self {
        Formula::RelatedValue(rel, prop)
    }

    /// Create a formula that applies a list operation.
    pub const fn list(list_op: FormulaList) -> Self {
        Formula::List(list_op)
    }

    /// Create a formula that counts a list.
    pub const fn count(list_op: FormulaList) -> Self {
        Formula::Count(list_op)
    }

    /// Create a formula for sibling index.
    pub const fn sibling_index() -> Self {
        Formula::SiblingIndex
    }
}
