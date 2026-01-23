/// Node relationship (for dependency tracking).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Relationship {
    /// Parent node (returns viewport properties for root).
    Parent,
    /// Next sibling in document order.
    NextSibling,
    /// Previous sibling in document order.
    PreviousSibling,
    /// All children.
    Children,
    /// All siblings (excluding self).
    Siblings,
    /// All previous siblings.
    PreviousSiblings,
    /// All next siblings.
    NextSiblings,
    /// All ancestors.
    Ancestors,
    /// All descendants.
    Descendants,
}
