//! Node relationship types.

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
