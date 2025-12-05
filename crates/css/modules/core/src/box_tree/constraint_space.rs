//! Constraint space for top-down layout propagation.
//!
//! Based on Chromium's `LayoutNG` constraint space model:
//! - Carries input constraints from parent to child (available size, BFC offset, etc.)
//! - Enables single-pass layout with minimal re-layout
//! - Handles float positioning through exclusion space
//! - Resolves margin collapsing and BFC offset interactions

use super::LayoutUnit;
use super::exclusion_space::ExclusionSpace;
use super::margin_strut::MarginStrut;

/// Input constraints passed from parent to child during layout.
///
/// This is the "constraint space" - it contains all the information a child
/// needs from its parent to compute layout correctly in a single pass.
#[derive(Debug, Clone)]
pub struct ConstraintSpace {
    /// Available inline size (width in horizontal writing mode)
    pub available_inline_size: AvailableSize,

    /// Available block size (height in horizontal writing mode)
    pub available_block_size: AvailableSize,

    /// The BFC (Block Formatting Context) block offset.
    /// This is the block-start edge of the current box relative to the BFC root.
    /// Critical for float positioning and margin collapsing.
    pub bfc_offset: BfcOffset,

    /// Exclusion space containing positioned floats.
    /// Cloned/shared between siblings, modified when new floats are added.
    pub exclusion_space: ExclusionSpace,

    /// Margin strut - accumulated margins that haven't collapsed yet.
    /// Used to track margin collapsing state as we descend the tree.
    pub margin_strut: MarginStrut,

    /// Whether this establishes a new formatting context.
    pub is_new_formatting_context: bool,

    /// Percentage resolution size (for resolving % widths/heights)
    pub percentage_resolution_block_size: Option<LayoutUnit>,

    /// Whether we're in a fragmentation context (e.g., multi-column, print)
    pub fragmentainer_block_size: Option<LayoutUnit>,
    pub fragmentainer_offset: LayoutUnit,
}

/// Available size can be definite (fixed), indefinite (auto), or constrained by min/max.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvailableSize {
    /// Fixed size (e.g., parent has width: 400px)
    Definite(LayoutUnit),

    /// Indefinite size (e.g., parent has width: auto)
    Indefinite,

    /// Min-content size (shrink to fit minimum)
    MinContent,

    /// Max-content size (expand to fit maximum)
    MaxContent,
}

impl AvailableSize {
    /// Get the size as `LayoutUnit`, using fallback for indefinite.
    pub fn resolve(&self, fallback: LayoutUnit) -> LayoutUnit {
        match self {
            Self::Definite(size) => *size,
            _ => fallback,
        }
    }

    /// Check if this is a definite size.
    pub fn is_definite(&self) -> bool {
        matches!(self, Self::Definite(_))
    }
}

/// Block Formatting Context offset.
///
/// Represents where this box is positioned relative to its containing BFC.
/// Critical for:
/// - Float positioning (floats are positioned relative to BFC)
/// - Margin collapsing (needs to know BFC offset to resolve)
/// - Clearance (clear property positions below floats in BFC)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BfcOffset {
    /// Inline offset (X in horizontal mode) - typically 0 or parent's inline offset
    pub inline_offset: LayoutUnit,

    /// Block offset (Y in horizontal mode) - this is the key value
    /// May be None if we haven't resolved margin collapsing yet
    pub block_offset: Option<LayoutUnit>,
}

impl BfcOffset {
    /// Create a new BFC offset.
    pub fn new(inline_offset: LayoutUnit, block_offset: Option<LayoutUnit>) -> Self {
        Self {
            inline_offset,
            block_offset,
        }
    }

    /// Root BFC offset (0, 0).
    pub fn root() -> Self {
        Self {
            inline_offset: LayoutUnit::zero(),
            block_offset: Some(LayoutUnit::zero()),
        }
    }

    /// Check if block offset is resolved.
    pub fn is_resolved(&self) -> bool {
        self.block_offset.is_some()
    }
}

impl ConstraintSpace {
    /// Create initial constraint space for the root element.
    pub fn new_for_root(icb_width: LayoutUnit, icb_height: LayoutUnit) -> Self {
        Self {
            available_inline_size: AvailableSize::Definite(icb_width),
            available_block_size: AvailableSize::Definite(icb_height),
            bfc_offset: BfcOffset::root(),
            exclusion_space: ExclusionSpace::new(),
            margin_strut: MarginStrut::default(),
            is_new_formatting_context: true, // Root establishes BFC
            percentage_resolution_block_size: Some(icb_height),
            fragmentainer_block_size: None,
            fragmentainer_offset: LayoutUnit::zero(),
        }
    }

    /// Create a child constraint space based on this parent space.
    ///
    /// This is where we propagate constraints down the tree.
    #[must_use]
    pub fn create_child_space(
        &self,
        available_inline: AvailableSize,
        available_block: AvailableSize,
        child_establishes_bfc: bool,
    ) -> Self {
        Self {
            available_inline_size: available_inline,
            available_block_size: available_block,

            // If child establishes new BFC, reset offset to unresolved
            // Otherwise inherit parent's BFC offset
            bfc_offset: if child_establishes_bfc {
                BfcOffset::new(LayoutUnit::zero(), None)
            } else {
                self.bfc_offset
            },

            // Clone exclusion space (modified as we add floats)
            exclusion_space: self.exclusion_space.clone(),

            // Reset margin strut if new BFC
            margin_strut: if child_establishes_bfc {
                MarginStrut::default()
            } else {
                self.margin_strut
            },

            is_new_formatting_context: child_establishes_bfc,
            percentage_resolution_block_size: self.percentage_resolution_block_size,
            fragmentainer_block_size: self.fragmentainer_block_size,
            fragmentainer_offset: self.fragmentainer_offset,
        }
    }
}

/// Result of laying out a box - output constraints.
///
/// This is what a box returns after layout, containing its final size
/// and any modifications to exclusion space (new floats added).
#[derive(Debug, Clone)]
pub struct LayoutResult {
    /// The final border-box size of this box (using f32 for sub-pixel precision)
    pub inline_size: f32,
    pub block_size: f32,

    /// The BFC offset where this box was actually placed
    /// (may differ from input if margin collapsing occurred)
    pub bfc_offset: BfcOffset,

    /// Updated exclusion space after laying out this box
    /// (includes any floats this box or its children added)
    pub exclusion_space: ExclusionSpace,

    /// Outgoing margin strut (for next sibling)
    pub end_margin_strut: MarginStrut,

    /// Baseline offset (for alignment)
    pub baseline: Option<i32>,

    /// Whether layout needs to be redone with resolved BFC offset
    /// (margin collapsing changed things)
    pub needs_relayout: bool,
}

impl LayoutResult {
    /// Create a simple layout result.
    pub fn new(inline_size: f32, block_size: f32, bfc_offset: BfcOffset) -> Self {
        Self {
            inline_size,
            block_size,
            bfc_offset,
            exclusion_space: ExclusionSpace::new(),
            end_margin_strut: MarginStrut::default(),
            baseline: None,
            needs_relayout: false,
        }
    }
}
