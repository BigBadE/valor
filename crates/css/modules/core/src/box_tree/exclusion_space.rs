//! Exclusion space for float positioning.
//!
//! Based on Chromium's "shelves algorithm" for efficient float placement.
//! Floats create exclusions that affect where other content can be placed.

use super::LayoutUnit;
use super::constraint_space::BfcOffset;
use css_orchestrator::style_model::{Clear, Float};
use js::NodeKey;

/// Exclusion space tracks positioned floats within a BFC.
///
/// Uses a "shelves" algorithm: floats are organized into horizontal shelves
/// at different block offsets, allowing efficient queries for available space.
#[derive(Debug, Clone)]
pub struct ExclusionSpace {
    /// Left floats organized by block offset (shelves)
    left_floats: Vec<FloatExclusion>,

    /// Right floats organized by block offset
    right_floats: Vec<FloatExclusion>,

    /// The BFC block offset of the last shelf (max float bottom)
    last_shelf_offset: LayoutUnit,
}

/// A positioned float creating an exclusion.
#[derive(Debug, Clone)]
pub struct FloatExclusion {
    /// Which node this float belongs to
    pub node_key: NodeKey,

    /// Bounding box of the float (relative to BFC) in `LayoutUnit` (1/64px)
    pub inline_start: LayoutUnit,
    pub inline_end: LayoutUnit,
    pub block_start: LayoutUnit,
    pub block_end: LayoutUnit,

    /// Which side this float is on
    pub float_type: Float,
}

/// Float size information (used to reduce parameter count in `add_float`).
#[derive(Debug, Clone, Copy)]
pub struct FloatSize {
    /// Inline size of the float in `LayoutUnit` (1/64px)
    pub inline_size: LayoutUnit,
    /// Block size of the float in `LayoutUnit` (1/64px)
    pub block_size: LayoutUnit,
    /// Which side this float is on
    pub float_type: Float,
}

impl ExclusionSpace {
    /// Create an empty exclusion space.
    pub fn new() -> Self {
        Self {
            left_floats: Vec::new(),
            right_floats: Vec::new(),
            last_shelf_offset: LayoutUnit::zero(),
        }
    }

    /// Add a float to the exclusion space.
    pub fn add_float(&mut self, node_key: NodeKey, bfc_offset: BfcOffset, float_size: FloatSize) {
        let FloatSize {
            inline_size,
            block_size,
            float_type,
        } = float_size;
        let block_offset = bfc_offset.block_offset.unwrap_or(LayoutUnit::zero());
        let inline_offset = bfc_offset.inline_offset;

        let exclusion = FloatExclusion {
            node_key,
            inline_start: inline_offset,
            inline_end: inline_offset + inline_size,
            block_start: block_offset,
            block_end: block_offset + block_size,
            float_type,
        };

        match float_type {
            Float::Left => self.left_floats.push(exclusion),
            Float::Right => self.right_floats.push(exclusion),
            Float::None => {
                // Not a float, ignore
            }
        }

        // Update last shelf offset
        self.last_shelf_offset = self.last_shelf_offset.max(block_offset + block_size);
    }

    /// Get the bottom edge of the last (deepest) float.
    ///
    /// Returns the BFC block offset where all floats end.
    /// This is used to ensure containers extend to contain their floats.
    pub fn last_float_bottom(&self) -> LayoutUnit {
        self.last_shelf_offset
    }

    /// Get available inline size at a given block offset.
    ///
    /// Returns (`start_offset`, `available_width`) for placing content.
    pub fn available_inline_size_at_offset(
        &self,
        block_offset: LayoutUnit,
        container_inline_size: LayoutUnit,
    ) -> (LayoutUnit, LayoutUnit) {
        // Find left-most right edge among left floats at this offset
        let left_edge = self
            .left_floats
            .iter()
            .filter(|f| f.block_start <= block_offset && block_offset < f.block_end)
            .map(|f| f.inline_end)
            .max()
            .unwrap_or(LayoutUnit::zero());

        // Find right-most left edge among right floats at this offset
        let right_edge = self
            .right_floats
            .iter()
            .filter(|f| f.block_start <= block_offset && block_offset < f.block_end)
            .map(|f| f.inline_start)
            .min()
            .unwrap_or(container_inline_size);

        let available = (right_edge - left_edge).max(LayoutUnit::zero());
        (left_edge, available)
    }

    /// Get the clearance offset for a given clear value.
    ///
    /// Returns the block offset where an element with this clear value should start.
    pub fn clearance_offset(&self, clear: Clear) -> LayoutUnit {
        match clear {
            Clear::None => LayoutUnit::zero(),
            Clear::Left => self
                .left_floats
                .iter()
                .map(|f| f.block_end)
                .max()
                .unwrap_or(LayoutUnit::zero()),
            Clear::Right => self
                .right_floats
                .iter()
                .map(|f| f.block_end)
                .max()
                .unwrap_or(LayoutUnit::zero()),
            Clear::Both => self.last_shelf_offset,
        }
    }

    /// Check if there are any floats at or after the given offset.
    pub fn has_floats_after(&self, block_offset: LayoutUnit) -> bool {
        self.left_floats
            .iter()
            .chain(self.right_floats.iter())
            .any(|f| f.block_end > block_offset)
    }

    /// Get all floats (for debugging).
    pub fn all_floats(&self) -> impl Iterator<Item = &FloatExclusion> {
        self.left_floats.iter().chain(self.right_floats.iter())
    }
}

impl Default for ExclusionSpace {
    fn default() -> Self {
        Self::new()
    }
}
