//! Shared types and parameter structs for constraint block layout.

use super::super::constraint_space::{BfcOffset, ConstraintSpace, LayoutResult};
use super::super::exclusion_space::ExclusionSpace;
use super::super::margin_strut::MarginStrut;
use css_box::{BoxSides, LayoutUnit};
use css_flexbox::{FlexChild, FlexContainerInputs};
use css_orchestrator::style_model::ComputedStyle;
use js::NodeKey;

/// Parameters for block layout operations (to avoid too many arguments).
pub(super) struct BlockLayoutParams<'params> {
    pub(super) constraint_space: &'params ConstraintSpace,
    pub(super) style: &'params ComputedStyle,
    pub(super) sides: &'params BoxSides,
    pub(super) inline_size: f32,
    pub(super) bfc_offset: BfcOffset,
    pub(super) establishes_bfc: bool,
}

/// State tracked during children layout.
pub(super) struct ChildrenLayoutState {
    pub(super) max_block_size: f32,
    pub(super) has_text_content: bool,
    pub(super) last_child_end_margin_strut: MarginStrut,
    pub(super) first_inflow_child_seen: bool,
    pub(super) resolved_bfc_offset: BfcOffset,
}

impl ChildrenLayoutState {
    pub(super) fn new(bfc_offset: BfcOffset) -> Self {
        Self {
            max_block_size: 0.0,
            has_text_content: false,
            last_child_end_margin_strut: MarginStrut::default(),
            first_inflow_child_seen: false,
            resolved_bfc_offset: bfc_offset,
        }
    }
}

/// Type alias for child style information tuple.
pub(super) type ChildStyleInfo = (NodeKey, ComputedStyle, LayoutResult);

/// Type alias for flex layout result.
pub(super) type FlexLayoutResult = (Vec<FlexChild>, Vec<ChildStyleInfo>, Vec<NodeKey>);

/// Parameters for computing block size from children.
pub(super) struct BlockSizeParams<'exclusion> {
    pub(super) style_height: Option<f32>,
    pub(super) can_collapse_with_children: bool,
    pub(super) establishes_bfc: bool,
    pub(super) resolved_bfc_offset: BfcOffset,
    pub(super) bfc_offset: BfcOffset,
    pub(super) child_space_bfc_offset: Option<LayoutUnit>,
    pub(super) has_text_content: bool,
    pub(super) exclusion_space: &'exclusion ExclusionSpace,
    pub(super) last_child_end_margin_strut: MarginStrut,
}

/// Parameters for applying flex placements.
pub(super) struct FlexPlacementParams {
    pub(super) content_base_inline: LayoutUnit,
    pub(super) content_base_block: Option<LayoutUnit>,
    pub(super) is_row: bool,
}

/// Parameters for creating flex result.
pub(super) struct FlexResultParams {
    pub(super) container_inline_size: f32,
    pub(super) final_cross_size: f32,
    pub(super) is_row: bool,
    pub(super) container_style: ComputedStyle,
}

/// Parameters for running flexbox layout.
pub(super) struct FlexboxLayoutParams {
    pub(super) container_inputs: FlexContainerInputs,
    pub(super) is_row: bool,
    pub(super) container_inline_size: f32,
    pub(super) container_cross_size: f32,
}

/// Parameters for computing end margin strut.
pub(super) struct EndMarginStrutParams<'params> {
    pub(super) sides: &'params BoxSides,
    pub(super) state: &'params ChildrenLayoutState,
    pub(super) incoming_space: &'params ConstraintSpace,
    pub(super) can_collapse_with_children: bool,
    pub(super) block_size: f32,
}

/// Parameters for creating flex abspos constraint space.
pub(super) struct FlexAbsposConstraintParams<'params> {
    pub(super) bfc_offset: BfcOffset,
    pub(super) sides: &'params BoxSides,
    pub(super) container_style: &'params ComputedStyle,
    pub(super) child_style: &'params ComputedStyle,
    pub(super) child_sides: &'params BoxSides,
    pub(super) container_inline_size: f32,
    pub(super) final_cross_size: f32,
    pub(super) is_row: bool,
}
