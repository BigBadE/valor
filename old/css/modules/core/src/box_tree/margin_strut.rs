//! Margin strut for tracking collapsing margins.
//!
//! Based on Chromium's `NGMarginStrut`:
//! Tracks "biggest positive" and "smallest negative" margins that haven't
//! collapsed yet as we descend the tree.

use super::LayoutUnit;

/// Margin strut accumulates margins during layout.
///
/// CSS margin collapsing combines adjacent margins. Instead of collapsing
/// immediately, we accumulate them in a strut and collapse when we know
/// where the box is actually positioned.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MarginStrut {
    /// Largest positive margin accumulated
    pub positive_margin: LayoutUnit,

    /// Smallest negative margin accumulated (most negative)
    pub negative_margin: LayoutUnit,
}

impl MarginStrut {
    /// Create a new empty margin strut.
    pub fn new() -> Self {
        Self {
            positive_margin: LayoutUnit::zero(),
            negative_margin: LayoutUnit::zero(),
        }
    }

    /// Append a margin to this strut.
    ///
    /// This follows CSS margin collapsing rules:
    /// - Multiple positive margins: use the largest
    /// - Multiple negative margins: use the most negative (smallest)
    /// - Mix of positive/negative: add them (they don't cancel in the strut)
    pub fn append(&mut self, margin: LayoutUnit) {
        if margin > LayoutUnit::zero() {
            self.positive_margin = self.positive_margin.max(margin);
        } else if margin < LayoutUnit::zero() {
            self.negative_margin = self.negative_margin.min(margin);
        }
    }

    /// Collapse the accumulated margins into a single value.
    ///
    /// Returns the final collapsed margin value.
    pub fn collapse(&self) -> LayoutUnit {
        self.positive_margin + self.negative_margin
    }

    /// Check if this strut has any margins.
    pub fn is_empty(&self) -> bool {
        self.positive_margin == LayoutUnit::zero() && self.negative_margin == LayoutUnit::zero()
    }
}
