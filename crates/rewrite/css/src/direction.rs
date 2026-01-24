/// Layout axis (block or inline).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, rewrite_macros::Markers)]
pub enum Axis {
    /// Block axis (vertical in horizontal writing mode).
    Block,
    /// Inline axis (horizontal in horizontal writing mode).
    Inline,
}

/// Logical direction along an axis (start/end).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, rewrite_macros::Markers)]
pub enum LogicalDirection {
    /// Start of the axis (top for block, left for inline in LTR).
    Start,
    /// End of the axis (bottom for block, right for inline in LTR).
    End,
}

impl LogicalDirection {
    /// Get the opposite direction.
    pub fn opposite(self) -> Self {
        match self {
            Self::Start => Self::End,
            Self::End => Self::Start,
        }
    }
}

/// Physical direction (absolute positioning).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhysicalDirection {
    Top,
    Right,
    Bottom,
    Left,
}
