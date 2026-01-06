//! Core block layout algorithm implementation.

use super::ConstraintLayoutTree;
use super::shared::{
    BlockLayoutParams, BlockSizeParams, ChildrenLayoutState, EndMarginStrutParams,
};
use css_box::{BoxSides, LayoutUnit, compute_box_sides};
use css_display::normalize_children;
use css_orchestrator::style_model::{BoxSizing, Clear, ComputedStyle, Float, Position};
use js::NodeKey;

use super::super::constraint_space::{AvailableSize, BfcOffset, ConstraintSpace, LayoutResult};
use super::super::exclusion_space::ExclusionSpace;
use super::super::margin_strut::MarginStrut;

impl ConstraintLayoutTree {
    /// Compute the end position of a child, including BFC margins if applicable.
    fn compute_child_end_with_bfc_margins(
        child_border_box_end: LayoutUnit,
        end_margin_strut: MarginStrut,
        child_style: &ComputedStyle,
    ) -> LayoutUnit {
        // For BFC-establishing elements, their margins don't collapse with siblings
        // If the end_margin_strut is empty, it means the element establishes a BFC
        // and we need to add its bottom margin as actual spacing
        if end_margin_strut.is_empty() {
            let child_sides = compute_box_sides(child_style);
            child_border_box_end + child_sides.margin_bottom
        } else {
            child_border_box_end
        }
    }

    pub(super) fn layout_block_first_pass(
        &mut self,
        node: NodeKey,
        params: &BlockLayoutParams,
    ) -> LayoutResult {
        // Estimate BFC offset (will be corrected in second pass)
        let estimated_offset = params
            .constraint_space
            .bfc_offset
            .block_offset
            .unwrap_or(LayoutUnit::zero())
            + params.sides.margin_top;

        let estimated_bfc_offset = BfcOffset::new(
            params.constraint_space.bfc_offset.inline_offset,
            Some(estimated_offset),
        );

        // Layout children with estimated offset
        let estimated_params = BlockLayoutParams {
            bfc_offset: estimated_bfc_offset,
            ..*params
        };
        let result = self.layout_block_children(node, &estimated_params);

        // Mark that we need relayout once we know actual BFC offset
        LayoutResult {
            needs_relayout: true,
            ..result
        }
    }
    pub(super) fn layout_block_child_and_update_state(
        &mut self,
        child: NodeKey,
        child_space: &mut ConstraintSpace,
        state: &mut ChildrenLayoutState,
        can_collapse_with_children: bool,
    ) {
        let child_result = self.layout_block(child, child_space);
        child_space.exclusion_space = child_result.exclusion_space.clone();

        let child_style = self.style(child);
        let is_float = !matches!(child_style.float, Float::None);
        let is_out_of_flow =
            is_float || matches!(child_style.position, Position::Absolute | Position::Fixed);

        // Check if child has clearance - if so, margins don't collapse with parent
        let has_clear = !matches!(child_style.clear, Clear::None);
        let has_floats_to_clear = child_space.exclusion_space.all_floats().next().is_some();
        let has_clearance = has_clear && has_floats_to_clear;

        // Only update layout state for in-flow children (not floats or absolutely positioned)
        if !is_out_of_flow {
            if let Some(child_start) = child_result.bfc_offset.block_offset {
                // Only resolve parent offset if child doesn't have clearance
                // (clearance prevents margin collapse)
                if !has_clearance {
                    Self::resolve_parent_offset_if_needed(
                        &mut state.resolved_bfc_offset,
                        &child_result,
                        state.first_inflow_child_seen,
                        can_collapse_with_children,
                    );
                }

                state.first_inflow_child_seen = true;
                let child_border_box_end =
                    child_start + LayoutUnit::from_px(child_result.block_size.round());

                // BUG FIX: For self-collapsing elements, margins collapse THROUGH them
                // The next sibling should start at the parent's base offset, not after the self-collapsing element
                // We detect self-collapsing by: block_size==0 and end_margin_strut contains incoming margins
                let is_self_collapsing = child_result.block_size.abs() < 0.01
                    && child_result.end_margin_strut.positive_margin > LayoutUnit::zero();

                // Self-collapsing element: next sibling starts where this element's incoming position was
                // This allows the next sibling's margin to collapse with all the accumulated margins
                // NOTE: child_space.bfc_offset.block_offset stays unchanged (not advanced)
                let should_advance_offset = !(is_self_collapsing && can_collapse_with_children);

                if should_advance_offset {
                    // Normal element: set next child's starting position to bottom of current child
                    let child_end = Self::compute_child_end_with_bfc_margins(
                        child_border_box_end,
                        child_result.end_margin_strut,
                        &child_style,
                    );
                    child_space.bfc_offset.block_offset = Some(child_end);
                }
            }

            // Carry forward the child's end margin strut for potential sibling collapse
            // Important: This allows the next sibling to collapse margins properly
            child_space.margin_strut = child_result.end_margin_strut;
            state.last_child_end_margin_strut = child_result.end_margin_strut;
            tracing::debug!(
                "layout_block_child_and_update_state: Setting next child margin_strut={:?}",
                child_space.margin_strut
            );
        }

        state.max_block_size = state.max_block_size.max(child_result.block_size);
        self.layout_results.insert(child, child_result);
    }
    pub(super) fn create_block_child_space(
        constraint_space: &ConstraintSpace,
        inline_size: f32,
        child_base_bfc_offset: BfcOffset,
        initial_margin_strut: MarginStrut,
        establishes_bfc: bool,
    ) -> ConstraintSpace {
        ConstraintSpace {
            available_inline_size: AvailableSize::Definite(LayoutUnit::from_px(inline_size)),
            available_block_size: constraint_space.available_block_size,
            bfc_offset: child_base_bfc_offset,
            exclusion_space: if establishes_bfc {
                ExclusionSpace::new()
            } else {
                constraint_space.exclusion_space.clone()
            },
            margin_strut: initial_margin_strut,
            is_new_formatting_context: establishes_bfc,
            percentage_resolution_block_size: constraint_space.percentage_resolution_block_size,
            fragmentainer_block_size: constraint_space.fragmentainer_block_size,
            fragmentainer_offset: constraint_space.fragmentainer_offset,
            is_for_measurement_only: constraint_space.is_for_measurement_only, // Propagate measurement flag
            margins_already_applied: false,
        }
    }
    pub(super) fn compute_block_size_from_children(
        &self,
        node: NodeKey,
        params: &BlockSizeParams<'_>,
        sides: &BoxSides,
        style: &ComputedStyle,
    ) -> f32 {
        // Compute border-box height
        let border_box_height = params.style_height.map_or_else(
            || {
                // Check for form control intrinsic height first
                if let Some(intrinsic_height) =
                    self.compute_form_control_intrinsic_height(node, style)
                {
                    // Form control has intrinsic height - use it as content-box height
                    let padding_border = sides.padding_top.to_px()
                        + sides.padding_bottom.to_px()
                        + sides.border_top.to_px()
                        + sides.border_bottom.to_px();
                    return intrinsic_height + padding_border;
                }

                // Auto height: compute from children
                // For BFC roots, children are laid out in the new BFC starting at 0
                // For non-BFC elements, use resolved/bfc offset depending on margin collapse
                let start_offset = if params.establishes_bfc {
                    LayoutUnit::zero()
                } else if params.can_collapse_with_children {
                    params
                        .resolved_bfc_offset
                        .block_offset
                        .unwrap_or(LayoutUnit::zero())
                } else {
                    params.bfc_offset.block_offset.unwrap_or(LayoutUnit::zero())
                        + sides.padding_top
                        + sides.border_top
                };

                // Consider both normal flow children and floats for the end offset
                let normal_flow_end = params.child_space_bfc_offset.unwrap_or(start_offset);
                let float_end = params.exclusion_space.last_float_bottom();
                let end_offset = normal_flow_end.max(float_end);

                let mut content_height =
                    (end_offset - start_offset).max(LayoutUnit::zero()).to_px();

                if content_height == 0.0 && params.has_text_content {
                    content_height = 18.0;
                }

                // BUG FIX: When bottom margin doesn't collapse (padding/border present),
                // the last child's margin must be included in the height calculation
                let can_collapse_bottom = sides.padding_bottom == LayoutUnit::zero()
                    && sides.border_bottom == LayoutUnit::zero();
                let non_collapsing_bottom_margin = if can_collapse_bottom {
                    0.0
                } else {
                    params.last_child_end_margin_strut.collapse().to_px()
                };

                // For both can_collapse_with_children cases, we use content_height as base:
                // - When true: padding/border is added below
                // - When false: start_offset already includes padding_top + border_top,
                //   but we still add all edges below to get correct border-box height
                content_height
                    + non_collapsing_bottom_margin
                    + sides.padding_top.to_px()
                    + sides.padding_bottom.to_px()
                    + sides.border_top.to_px()
                    + sides.border_bottom.to_px()
            },
            |height| {
                // Apply box-sizing transformation to specified height
                match style.box_sizing {
                    BoxSizing::ContentBox => {
                        // Height is content-box, add padding and border
                        let padding_border = sides.padding_top.to_px()
                            + sides.padding_bottom.to_px()
                            + sides.border_top.to_px()
                            + sides.border_bottom.to_px();
                        height + padding_border
                    }
                    BoxSizing::BorderBox => {
                        // Height already includes padding and border
                        height
                    }
                }
            },
        );

        // Apply min/max constraints in border-box space
        Self::apply_height_constraints(border_box_height.max(0.0), style, sides)
    }
    pub(super) fn compute_border_box_inline(inline_size: f32, sides: &BoxSides) -> f32 {
        inline_size
            + sides.padding_left.to_px()
            + sides.padding_right.to_px()
            + sides.border_left.to_px()
            + sides.border_right.to_px()
    }

    pub(super) fn resolve_final_bfc_offset(
        block_size: f32,
        can_collapse_with_children: bool,
        state: &ChildrenLayoutState,
        params: &BlockLayoutParams,
        _initial_margin_strut: MarginStrut,
    ) -> BfcOffset {
        // - Self-collapsing box: resolve based on parent offset + collapsed margins
        // - Box that collapses with first child: use the resolved offset (matches first child)
        // - Box with border/padding (can't collapse): use params offset
        if block_size.abs() < 0.01 && !state.first_inflow_child_seen && can_collapse_with_children {
            // Self-collapsing box: resolve position based on parent offset + all collapsed margins
            // BUG FIX: Use the incoming margin strut from params, not just the element's own margins
            // The incoming strut includes parent/sibling margins that should collapse with this element
            let parent_offset = params
                .constraint_space
                .bfc_offset
                .block_offset
                .unwrap_or(LayoutUnit::zero());
            let mut margin_strut = params.constraint_space.margin_strut;
            margin_strut.append(params.sides.margin_top);
            margin_strut.append(params.sides.margin_bottom);
            let margin_collapse = margin_strut.collapse();
            let resolved_offset = parent_offset + margin_collapse;
            BfcOffset::new(params.bfc_offset.inline_offset, Some(resolved_offset))
        } else if can_collapse_with_children && state.first_inflow_child_seen {
            // Non-self-collapsing box that can collapse with children: use resolved offset
            // (this is where the first child ended up after margin collapse)
            state.resolved_bfc_offset
        } else {
            // Box with border/padding or no children: use the box's own offset
            params.bfc_offset
        }
    }

    /// Process all children in the layout loop.
    pub(super) fn process_children_layout(
        &mut self,
        node: NodeKey,
        child_space: &mut ConstraintSpace,
        state: &mut ChildrenLayoutState,
        can_collapse_with_children: bool,
    ) {
        // Normalize children to handle display:none and display:contents
        let children = normalize_children(&self.children, &self.styles, node);

        for child in children {
            if self.is_text_node(child) {
                self.layout_text_child(
                    (node, child),
                    child_space,
                    state,
                    can_collapse_with_children,
                );
            } else {
                self.layout_block_child_and_update_state(
                    child,
                    child_space,
                    state,
                    can_collapse_with_children,
                );
            }
        }
    }

    /// Layout block's children and compute final size.
    pub(super) fn layout_block_children(
        &mut self,
        node: NodeKey,
        params: &BlockLayoutParams,
    ) -> LayoutResult {
        // Margins can collapse with children only if:
        // 1. No padding/border at top
        // 2. Parent doesn't establish a new BFC (BFC boundary prevents collapse)
        let can_collapse_with_children = params.sides.padding_top == LayoutUnit::zero()
            && params.sides.border_top == LayoutUnit::zero()
            && !params.establishes_bfc;

        let initial_margin_strut = Self::compute_initial_margin_strut(
            params.constraint_space,
            params.sides,
            params.establishes_bfc,
            can_collapse_with_children,
        );

        let child_base_bfc_offset = Self::compute_child_base_bfc_offset(
            params.bfc_offset,
            params.sides,
            params.establishes_bfc,
            can_collapse_with_children,
        );

        let mut child_space = Self::create_block_child_space(
            params.constraint_space,
            params.inline_size,
            child_base_bfc_offset,
            initial_margin_strut,
            params.establishes_bfc,
        );

        let mut state = ChildrenLayoutState::new(params.bfc_offset);

        // Process all children
        self.process_children_layout(
            node,
            &mut child_space,
            &mut state,
            can_collapse_with_children,
        );

        let block_size_params = BlockSizeParams {
            style_height: params.style.height,
            can_collapse_with_children,
            establishes_bfc: params.establishes_bfc,
            resolved_bfc_offset: state.resolved_bfc_offset,
            bfc_offset: params.bfc_offset,
            child_space_bfc_offset: child_space.bfc_offset.block_offset,
            has_text_content: state.has_text_content,
            exclusion_space: &child_space.exclusion_space,
            last_child_end_margin_strut: state.last_child_end_margin_strut,
        };
        let block_size = self.compute_block_size_from_children(
            node,
            &block_size_params,
            params.sides,
            params.style,
        );
        let border_box_inline = Self::compute_border_box_inline(params.inline_size, params.sides);

        let end_margin_strut_params = EndMarginStrutParams {
            sides: params.sides,
            state: &state,
            incoming_space: params.constraint_space,
            can_collapse_with_children,
            block_size,
        };
        let end_margin_strut = Self::compute_end_margin_strut(&end_margin_strut_params);

        let final_bfc_offset = Self::resolve_final_bfc_offset(
            block_size,
            can_collapse_with_children,
            &state,
            params,
            initial_margin_strut,
        );

        LayoutResult {
            inline_size: border_box_inline.max(0.0),
            block_size: block_size.max(0.0),
            bfc_offset: final_bfc_offset,
            exclusion_space: child_space.exclusion_space,
            end_margin_strut,
            baseline: None,
            needs_relayout: false,
        }
    }
}
