#![feature(generic_const_exprs)]
#![allow(incomplete_features)]

//! Core infrastructure for formula-based layout computation.
//!
//! This crate provides:
//! - `Database`: CSS property storage and query caching
//! - `Query`: Decision trees for formula selection based on CSS properties
//! - `Formula`: Computation graphs for layout values
//! - `NodeId`: Unique identifiers for DOM nodes
//!
//! Key design principles:
//! - Queries branch on CSS properties to select formulas
//! - Formulas are pure arithmetic evaluated against concrete values
//! - Everything is const - built at compile time

/// Unique identifier for a DOM node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(u64);

impl NodeId {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    pub const fn from_raw(id: u64) -> Self {
        Self(id)
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// The Document node ID - root of the entire tree.
pub const DOCUMENT_NODE_ID: NodeId = NodeId::new(0);

// ============================================================================
// Directional Types (used across properties and values)
// ============================================================================

/// Single node relationship (one node).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SingleRelationship {
    /// The current node.
    Self_,
    /// The parent node.
    Parent,
}

/// Multiple node relationship (zero or more nodes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MultiRelationship {
    /// All child nodes.
    Children,
    /// All previous siblings.
    PrevSiblings,
    /// All next siblings.
    NextSiblings,
    /// All siblings (both prev and next).
    Siblings,
}

/// Any relationship to other nodes in the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Relationship {
    Single(SingleRelationship),
    Multi(MultiRelationship),
}

/// Physical box edge directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Edge {
    Top = 0,
    Right = 1,
    Bottom = 2,
    Left = 3,
}

/// Physical corner positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Corner {
    TopLeft = 0,
    TopRight = 1,
    BottomRight = 2,
    BottomLeft = 3,
}

/// Axis directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Axis {
    Horizontal = 0, // Width, Row, X, Inline
    Vertical = 1,   // Height, Column, Y, Block
}

/// 3D axis for transforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Axis3D {
    X = 0,
    Y = 1,
    Z = 2,
}

/// Boundary type (min/max constraints).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Boundary {
    Min = 0,
    Max = 1,
}

/// Positional edge (start/end).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Position {
    Start = 0,
    End = 1,
}

pub mod formula;
pub mod property;
pub mod storage;
pub mod value;
pub mod work_queue;

pub use Relationship::*;
pub use formula::{Formula, FormulaList};
pub use property::*;
pub use storage::Database;
pub use value::*;
pub use work_queue::{Priority, WorkQueue};

/// A query function that selects which formula to use based on CSS properties.
pub type Query = fn(&mut ScopedDb) -> &'static Formula;

/// Scoped database access for a specific node.
pub struct ScopedDb<'a> {
    db: &'a Database,
    parent: NodeId,
    node: NodeId,
    branch_deps: smallvec::SmallVec<[(SingleRelationship, CssLayoutProperty); 4]>,
}

impl<'a> ScopedDb<'a> {
    /// Read a CSS layout property keyword (for Query branching).
    pub fn css(&mut self, rel: SingleRelationship, prop: CssLayoutProperty) -> Keyword {
        // Track this as a branch dependency
        self.branch_deps.push((rel, prop));

        // Resolve the related node
        let target_node = match rel {
            SingleRelationship::Self_ => self.node,
            SingleRelationship::Parent => self.parent,
        };

        self.db.get_layout_keyword(target_node, prop)
    }
}
