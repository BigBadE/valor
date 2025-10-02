//! Stacking context management for CSS paint order.
//!
//! Implements CSS 2.2 Appendix E stacking order:
//! 1. Background and borders of root element
//! 2. Descendant blocks (in tree order), positioned elements (by z-index)
//! 3. Descendant floats
//! 4. Descendant inline content
//! 5. Descendant positioned elements (by z-index)

use std::cmp::Ordering;

/// Stacking context level for CSS paint order.
///
/// Per CSS 2.2 Appendix E, elements are painted in this order:
/// - Negative z-index stacking contexts (back to front)
/// - Block-level descendants in normal flow
/// - Floats
/// - Inline-level descendants
/// - Positioned descendants with z-index: auto or 0
/// - Positive z-index stacking contexts (back to front)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackingLevel {
    /// Background and borders of the stacking context root.
    RootBackgroundAndBorders,
    /// Negative z-index stacking contexts.
    NegativeZIndex(i32),
    /// Block-level descendants in normal flow.
    BlockDescendants,
    /// Floats.
    Floats,
    /// Inline-level content, non-positioned.
    InlineContent,
    /// Positioned descendants with z-index: auto or z-index: 0.
    PositionedZeroOrAuto,
    /// Positive z-index stacking contexts.
    PositiveZIndex(i32),
}

impl StackingLevel {
    /// Create a stacking level from a z-index value.
    #[must_use]
    pub const fn from_z_index(z_index: i32) -> Self {
        if z_index < 0 {
            Self::NegativeZIndex(z_index)
        } else if z_index == 0 {
            Self::PositionedZeroOrAuto
        } else {
            Self::PositiveZIndex(z_index)
        }
    }

    /// Get the sort key for this stacking level.
    ///
    /// Lower values paint first (behind).
    #[must_use]
    const fn sort_key(self) -> (i32, i32) {
        match self {
            Self::RootBackgroundAndBorders => (0, 0),
            Self::NegativeZIndex(z) => (1, z),
            Self::BlockDescendants => (2, 0),
            Self::Floats => (3, 0),
            Self::InlineContent => (4, 0),
            Self::PositionedZeroOrAuto => (5, 0),
            Self::PositiveZIndex(z) => (6, z),
        }
    }
}

impl PartialOrd for StackingLevel {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for StackingLevel {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sort_key().cmp(&other.sort_key())
    }
}

/// Represents a CSS stacking context.
///
/// A stacking context is established by:
/// - Root element
/// - Positioned elements with z-index other than auto
/// - Elements with opacity < 1
/// - Elements with transforms
/// - Elements with filters
/// - Elements with mix-blend-mode
#[derive(Debug, Clone)]
pub struct StackingContext {
    /// Stacking level within parent stacking context.
    pub level: StackingLevel,
    /// Tree order within siblings at the same level.
    pub tree_order: u32,
    /// Whether this establishes a new stacking context.
    pub establishes_stacking_context: bool,
    /// Opacity value (1.0 = opaque).
    pub opacity: f32,
    /// Clipping bounds (x, y, width, height).
    pub clip: Option<(f32, f32, f32, f32)>,
}

impl StackingContext {
    /// Create a new stacking context.
    #[must_use]
    pub const fn new(level: StackingLevel, tree_order: u32) -> Self {
        Self {
            level,
            tree_order,
            establishes_stacking_context: false,
            opacity: 1.0,
            clip: None,
        }
    }

    /// Create a root stacking context.
    #[must_use]
    pub const fn root() -> Self {
        Self {
            level: StackingLevel::RootBackgroundAndBorders,
            tree_order: 0,
            establishes_stacking_context: true,
            opacity: 1.0,
            clip: None,
        }
    }

    /// Set opacity and mark as establishing a stacking context.
    #[must_use]
    pub const fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        if opacity < 1.0 {
            self.establishes_stacking_context = true;
        }
        self
    }

    /// Set clip bounds.
    #[must_use]
    pub const fn with_clip(mut self, clip: (f32, f32, f32, f32)) -> Self {
        self.clip = Some(clip);
        self
    }

    /// Mark this as establishing a stacking context.
    #[must_use]
    pub const fn establishing_stacking_context(mut self) -> Self {
        self.establishes_stacking_context = true;
        self
    }
}

impl PartialEq for StackingContext {
    fn eq(&self, other: &Self) -> bool {
        self.level == other.level && self.tree_order == other.tree_order
    }
}

impl Eq for StackingContext {}

impl PartialOrd for StackingContext {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for StackingContext {
    fn cmp(&self, other: &Self) -> Ordering {
        self.level
            .cmp(&other.level)
            .then_with(|| self.tree_order.cmp(&other.tree_order))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stacking_order() {
        let root = StackingLevel::RootBackgroundAndBorders;
        let neg_10 = StackingLevel::NegativeZIndex(-10);
        let neg_1 = StackingLevel::NegativeZIndex(-1);
        let blocks = StackingLevel::BlockDescendants;
        let floats = StackingLevel::Floats;
        let inline = StackingLevel::InlineContent;
        let auto = StackingLevel::PositionedZeroOrAuto;
        let pos_1 = StackingLevel::PositiveZIndex(1);
        let pos_10 = StackingLevel::PositiveZIndex(10);

        assert!(root < neg_10);
        assert!(neg_10 < neg_1);
        assert!(neg_1 < blocks);
        assert!(blocks < floats);
        assert!(floats < inline);
        assert!(inline < auto);
        assert!(auto < pos_1);
        assert!(pos_1 < pos_10);
    }

    #[test]
    fn tree_order_breaks_ties() {
        let ctx1 = StackingContext::new(StackingLevel::BlockDescendants, 0);
        let ctx2 = StackingContext::new(StackingLevel::BlockDescendants, 1);
        assert!(ctx1 < ctx2);
    }

    #[test]
    fn opacity_establishes_context() {
        let ctx = StackingContext::new(StackingLevel::BlockDescendants, 0).with_opacity(0.5);
        assert!(ctx.establishes_stacking_context);
        assert_eq!(ctx.opacity, 0.5);
    }
}
