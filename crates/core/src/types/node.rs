//! Node identifiers and pixel types.

/// Unique identifier for a DOM node. Index into DomTree's parallel vecs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

impl NodeId {
    pub const ROOT: Self = Self(0);
}

/// Subpixel value for layout computations. Currently i32 (whole pixels).
pub type Subpixel = i32;
