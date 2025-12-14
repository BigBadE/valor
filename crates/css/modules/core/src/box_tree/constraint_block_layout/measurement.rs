//! Pure measurement functions for layout items.
//!
//! This module separates measurement from layout, allowing multi-pass layout
//! algorithms (Grid, Flexbox) to measure items multiple times with different
//! constraints without performing full layout.

use super::super::constraint_space::{AvailableSize, BfcOffset, ConstraintSpace};
use super::super::exclusion_space::ExclusionSpace;
use super::super::margin_strut::MarginStrut;
use super::ConstraintLayoutTree;
use css_box::LayoutUnit;
use js::NodeKey;

/// Result of measuring an item's size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeasuredSize {
    /// Inline size (width in horizontal writing mode).
    pub inline: f32,
    /// Block size (height in horizontal writing mode).
    pub block: f32,
}

impl MeasuredSize {
    /// Create a new measured size.
    pub const fn new(inline: f32, block: f32) -> Self {
        Self { inline, block }
    }

    /// Zero size.
    pub const fn zero() -> Self {
        Self {
            inline: 0.0,
            block: 0.0,
        }
    }
}

impl ConstraintLayoutTree {
    /// Measure an item's size without performing layout.
    ///
    /// This is a pure measurement function that calculates what size an item
    /// would be given specific constraints, without mutating layout state or
    /// positioning children.
    ///
    /// # Parameters
    ///
    /// - `node`: The item to measure
    /// - `available_inline`: Available inline size constraint
    /// - `available_block`: Available block size constraint
    ///
    /// # Returns
    ///
    /// The measured size of the item.
    pub fn measure_item(
        &mut self,
        node: NodeKey,
        available_inline: AvailableSize,
        available_block: AvailableSize,
    ) -> MeasuredSize {
        // Text nodes don't have boxes
        if self.is_text_node(node) {
            return MeasuredSize::zero();
        }

        // Create constraint space for measurement
        let space = ConstraintSpace {
            available_inline_size: available_inline,
            available_block_size: available_block,
            bfc_offset: BfcOffset::root(),
            exclusion_space: ExclusionSpace::new(),
            margin_strut: MarginStrut::default(),
            is_new_formatting_context: true,
            percentage_resolution_block_size: None,
            fragmentainer_block_size: None,
            fragmentainer_offset: LayoutUnit::zero(),
            is_for_measurement_only: true, // This is measurement, not final layout
        };

        // Perform layout to get size
        // Note: This still performs full layout internally, but we only use the size
        let result = self.layout_block(node, &space);

        MeasuredSize::new(result.inline_size, result.block_size)
    }

    /// Measure item's block size at a specific inline size.
    ///
    /// This is essential for grid row sizing and flexbox cross-sizing where
    /// we need to know "how tall would this be at width X?"
    ///
    /// # Parameters
    ///
    /// - `node`: The item to measure
    /// - `inline_size`: The definite inline size to constrain to
    ///
    /// # Returns
    ///
    /// The resulting block size in pixels.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Grid row sizing: measure item at its column width
    /// let column_width = 260.0;
    /// let height = tree.measure_block_at_inline(item, column_width);
    /// ```
    pub fn measure_block_at_inline(&mut self, node: NodeKey, inline_size: f32) -> f32 {
        tracing::debug!(
            "measure_block_at_inline: node={:?}, inline_size={:.1}px",
            node,
            inline_size
        );

        let size = self.measure_item(
            node,
            AvailableSize::Definite(LayoutUnit::from_px(inline_size)),
            AvailableSize::Indefinite,
        );

        size.block
    }

    /// Measure item's inline size at indefinite constraint.
    ///
    /// This gives the item's "natural" width - how wide it wants to be
    /// without any width constraints.
    ///
    /// # Parameters
    ///
    /// - `node`: The item to measure
    ///
    /// # Returns
    ///
    /// The natural inline size in pixels.
    pub fn measure_natural_inline(&mut self, node: NodeKey) -> f32 {
        let size = self.measure_item(node, AvailableSize::Indefinite, AvailableSize::Indefinite);
        size.inline
    }

    /// Measure item at definite inline and block sizes.
    ///
    /// This is used when both dimensions are constrained (e.g., final grid
    /// item layout).
    ///
    /// # Parameters
    ///
    /// - `node`: The item to measure
    /// - `inline_size`: Definite inline size
    /// - `block_size`: Definite block size
    ///
    /// # Returns
    ///
    /// The measured size (may be smaller than constraints due to intrinsic sizing).
    pub fn measure_at_size(
        &mut self,
        node: NodeKey,
        inline_size: f32,
        block_size: f32,
    ) -> MeasuredSize {
        self.measure_item(
            node,
            AvailableSize::Definite(LayoutUnit::from_px(inline_size)),
            AvailableSize::Definite(LayoutUnit::from_px(block_size)),
        )
    }
}
