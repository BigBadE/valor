//! Node relationship types.

/// Single node relationship (one node).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SingleRelationship {
    /// The current node.
    Self_,
    /// The parent node.
    Parent,
    /// The immediately previous sibling in DOM order.
    /// Returns self if there is no previous sibling.
    PrevSibling,
    /// The nearest ancestor that is a block container (not inline).
    /// Used by block-in-inline layout: when a block element is inside
    /// an inline, it sizes and positions relative to the nearest block
    /// ancestor, not the inline parent.
    BlockContainer,
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
    /// All child nodes sorted by CSS `order` property (stable sort, DOM order as tiebreak).
    /// Used by flexbox layout where the `order` property controls visual ordering.
    OrderedChildren,
    /// All previous siblings in CSS `order`-sorted sequence.
    /// "Previous" means siblings that appear before this node when all siblings
    /// are sorted by `order` (with DOM order as tiebreak).
    OrderedPrevSiblings,
}

/// Any relationship to other nodes in the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Relationship {
    Single(SingleRelationship),
    Multi(MultiRelationship),
}
