//! Flexbox main layout algorithm.

use super::ConstraintLayoutTree;
use super::shared::{ChildStyleInfo, FlexPlacementParams, FlexResultParams, FlexboxLayoutParams};
use css_box::{BoxSides, compute_box_sides};
use css_display::normalize_children;
use std::collections::HashMap;

use css_flexbox::{
    AlignContent as FlexAlignContent, AlignItems as FlexAlignItems, CrossAndBaseline, CrossContext,
    CrossPlacement, CrossSize, FlexChild, FlexContainerInputs, FlexDirection as FlexboxDirection,
    FlexPlacement, ItemRef, JustifyContent as FlexJustifyContent, WritingMode,
    layout_multi_line_with_cross, layout_single_line_with_cross, sort_items_by_order_stable,
};
use css_orchestrator::style_model::{
    AlignContent as StyleAlignContent, AlignItems as StyleAlignItems, ComputedStyle,
    FlexDirection as StyleFlexDirection, FlexWrap as StyleFlexWrap,
    JustifyContent as StyleJustifyContent,
};
use js::NodeKey;

use super::super::constraint_space::{BfcOffset, ConstraintSpace, LayoutResult};

impl ConstraintLayoutTree {
    pub(super) fn handle_empty_flex_container(
        &mut self,
        abspos_children: &[NodeKey],
        bfc_offset: BfcOffset,
        sides: &BoxSides,
        result_params: &FlexResultParams,
    ) -> LayoutResult {
        if !abspos_children.is_empty() {
            self.layout_flex_abspos_children(abspos_children, bfc_offset, sides, result_params);
        }

        Self::create_empty_flex_result(
            result_params.container_inline_size,
            result_params.final_cross_size,
            bfc_offset,
            sides,
        )
    }

    /// Prepare flexbox container inputs.
    pub(super) fn prepare_flex_container_inputs(
        flex_direction: FlexboxDirection,
        container_inline_size: f32,
        container_cross_size: f32,
        is_row: bool,
        style: &ComputedStyle,
    ) -> FlexContainerInputs {
        // Resolve percentage gaps against container size
        let main_gap = if is_row {
            // Row flex: column-gap applies to main axis (inline)
            style
                .column_gap_percent
                .map_or(style.column_gap, |gap_percent| {
                    container_inline_size * gap_percent
                })
        } else {
            // Column flex: row-gap applies to main axis (block)
            style.row_gap_percent.map_or(style.row_gap, |gap_percent| {
                container_cross_size * gap_percent
            })
        };

        FlexContainerInputs {
            direction: flex_direction,
            writing_mode: WritingMode::HorizontalTb,
            container_main_size: if is_row {
                container_inline_size
            } else {
                container_cross_size
            },
            main_gap,
        }
    }

    /// Convert style align items to flex align items.
    /// Also used to resolve align-self: auto to the container's align-items value.
    fn convert_align_items(align_items: StyleAlignItems) -> FlexAlignItems {
        match align_items {
            StyleAlignItems::Normal | StyleAlignItems::Stretch => FlexAlignItems::Stretch,
            StyleAlignItems::FlexStart => FlexAlignItems::FlexStart,
            StyleAlignItems::Center => FlexAlignItems::Center,
            StyleAlignItems::FlexEnd => FlexAlignItems::FlexEnd,
        }
    }

    /// Convert style justify content to flex justify content.
    fn convert_justify_content(justify_content: StyleJustifyContent) -> FlexJustifyContent {
        match justify_content {
            StyleJustifyContent::FlexStart => FlexJustifyContent::Start,
            StyleJustifyContent::Center => FlexJustifyContent::Center,
            StyleJustifyContent::FlexEnd => FlexJustifyContent::End,
            StyleJustifyContent::SpaceBetween => FlexJustifyContent::SpaceBetween,
            StyleJustifyContent::SpaceAround => FlexJustifyContent::SpaceAround,
            StyleJustifyContent::SpaceEvenly => FlexJustifyContent::SpaceEvenly,
        }
    }

    /// Convert style align content to flex align content.
    fn convert_align_content(align_content: StyleAlignContent) -> FlexAlignContent {
        match align_content {
            StyleAlignContent::Stretch => FlexAlignContent::Stretch,
            StyleAlignContent::FlexStart => FlexAlignContent::Start,
            StyleAlignContent::Center => FlexAlignContent::Center,
            StyleAlignContent::FlexEnd => FlexAlignContent::End,
            StyleAlignContent::SpaceBetween => FlexAlignContent::SpaceBetween,
            StyleAlignContent::SpaceAround => FlexAlignContent::SpaceAround,
            StyleAlignContent::SpaceEvenly => FlexAlignContent::SpaceEvenly,
        }
    }

    /// Build cross inputs for flex children.
    fn build_cross_inputs(
        child_styles: &[ChildStyleInfo],
        is_row: bool,
        container_has_definite_cross_size: bool,
    ) -> Vec<(CrossSize, f32, f32)> {
        child_styles
            .iter()
            .map(|(_child, child_style, result)| {
                let has_explicit_cross_size = if is_row {
                    child_style.height.is_some()
                } else {
                    child_style.width.is_some()
                };
                // Get border-box size from result, then convert to content-box for flex algorithm
                let border_box_cross_size = if is_row {
                    result.block_size
                } else {
                    result.inline_size
                };
                let child_sides = compute_box_sides(child_style);
                let padding_border_cross = if is_row {
                    (child_sides.padding_top
                        + child_sides.padding_bottom
                        + child_sides.border_top
                        + child_sides.border_bottom)
                        .to_px()
                } else {
                    (child_sides.padding_left
                        + child_sides.padding_right
                        + child_sides.border_left
                        + child_sides.border_right)
                        .to_px()
                };
                let intrinsic_cross = (border_box_cross_size - padding_border_cross).max(0.0);

                // Use CrossSize enum to distinguish explicit sizes from stretchable items.
                // Per CSS Flexbox spec, items can only stretch if:
                // 1. The item itself doesn't have an explicit cross-axis size, AND
                // 2. The container has a definite cross-axis size
                let can_stretch = !has_explicit_cross_size && container_has_definite_cross_size;
                let cross_size = if can_stretch {
                    CrossSize::Stretch(intrinsic_cross)
                } else {
                    CrossSize::Explicit(intrinsic_cross)
                };

                (cross_size, 0.0, 1e9)
            })
            .collect()
    }

    /// Run flexbox layout algorithm and get placements.
    pub(super) fn run_flexbox_layout(
        flex_items: &[FlexChild],
        child_styles: &[ChildStyleInfo],
        params: &FlexboxLayoutParams,
        style: &ComputedStyle,
    ) -> Vec<(FlexPlacement, CrossPlacement)> {
        type BaselineInput = Option<(f32, f32)>;
        let align_items = Self::convert_align_items(style.align_items);
        let justify_content = Self::convert_justify_content(style.justify_content);
        let align_content = Self::convert_align_content(style.align_content);

        // Check if container has definite cross-axis size
        let container_has_definite_cross_size = if params.is_row {
            style.height.is_some()
        } else {
            style.width.is_some()
        };

        let cross_inputs = Self::build_cross_inputs(
            child_styles,
            params.is_row,
            container_has_definite_cross_size,
        );
        let baseline_inputs: Vec<BaselineInput> = vec![None; flex_items.len()];

        // Resolve percentage gaps for cross axis
        let cross_gap = if params.is_row {
            // Row flex: row-gap applies to cross axis (block)
            style.row_gap_percent.map_or(style.row_gap, |gap_percent| {
                params.container_cross_size * gap_percent
            })
        } else {
            // Column flex: column-gap applies to cross axis (inline)
            style
                .column_gap_percent
                .map_or(style.column_gap, |gap_percent| {
                    params.container_inline_size * gap_percent
                })
        };

        let cross_ctx = CrossContext {
            align_items,
            align_content,
            container_cross_size: if params.is_row {
                params.container_cross_size
            } else {
                params.container_inline_size
            },
            cross_gap,
        };
        if matches!(style.flex_wrap, StyleFlexWrap::NoWrap) {
            layout_single_line_with_cross(
                params.container_inputs,
                justify_content,
                cross_ctx,
                flex_items,
                CrossAndBaseline {
                    cross_inputs: &cross_inputs,
                    baseline_inputs: &baseline_inputs,
                },
            )
        } else {
            layout_multi_line_with_cross(
                params.container_inputs,
                justify_content,
                cross_ctx,
                flex_items,
                CrossAndBaseline {
                    cross_inputs: &cross_inputs,
                    baseline_inputs: &baseline_inputs,
                },
            )
        }
    }

    /// Sort flex items by their order property, preserving DOM order for ties.
    fn sort_flex_items_by_order(
        flex_items: &mut Vec<FlexChild>,
        child_styles: &mut Vec<ChildStyleInfo>,
    ) {
        // Build (ItemRef, order) pairs
        let order_pairs: Vec<(ItemRef, i32)> = child_styles
            .iter()
            .enumerate()
            .map(|(idx, (_, style, _))| (flex_items[idx].handle, style.order))
            .collect();

        // Get sorted indices
        let sorted_handles = sort_items_by_order_stable(&order_pairs);

        // Create a mapping from handle to original index
        let handle_to_idx: HashMap<u64, usize> = flex_items
            .iter()
            .enumerate()
            .map(|(idx, item)| (item.handle.0, idx))
            .collect();

        // Create new sorted vectors
        let sorted_flex_items: Vec<FlexChild> = sorted_handles
            .iter()
            .map(|handle| flex_items[handle_to_idx[&handle.0]])
            .collect();

        let sorted_child_styles: Vec<ChildStyleInfo> = sorted_handles
            .iter()
            .map(|handle| child_styles[handle_to_idx[&handle.0]].clone())
            .collect();

        // Replace original vectors with sorted ones
        *flex_items = sorted_flex_items;
        *child_styles = sorted_child_styles;
    }

    /// Prepare flexbox layout parameters.
    fn prepare_flexbox_params(
        flex_direction: FlexboxDirection,
        container_inline_size: f32,
        container_cross_size: f32,
        is_row: bool,
        style: &ComputedStyle,
    ) -> FlexboxLayoutParams {
        let container_inputs = Self::prepare_flex_container_inputs(
            flex_direction,
            container_inline_size,
            container_cross_size,
            is_row,
            style,
        );
        FlexboxLayoutParams {
            container_inputs,
            is_row,
            container_inline_size,
            container_cross_size,
        }
    }

    /// Layout a flex container using the proper flexbox algorithm.
    pub(super) fn layout_flex(
        &mut self,
        node: NodeKey,
        constraint_space: &ConstraintSpace,
        style: &ComputedStyle,
        sides: &BoxSides,
    ) -> LayoutResult {
        let container_inline_size = self.compute_inline_size(node, constraint_space, style, sides);
        let container_cross_size =
            Self::compute_flex_container_cross_size(style, sides, constraint_space);
        let (bfc_offset, _can_collapse_with_children) =
            Self::resolve_bfc_offset(constraint_space, style, sides, true);
        let flex_direction = match style.flex_direction {
            StyleFlexDirection::Row => FlexboxDirection::Row,
            StyleFlexDirection::Column => FlexboxDirection::Column,
        };
        let is_row = matches!(flex_direction, FlexboxDirection::Row);
        let children = normalize_children(&self.children, &self.styles, node);
        log::error!(
            "layout_flex: node={:?}, children.len()={}",
            node,
            children.len()
        );
        let (mut flex_items, mut child_styles, abspos_children) =
            self.build_flex_items(node, &children, container_inline_size, is_row);
        log::error!(
            "layout_flex: node={:?}, flex_items.len()={}",
            node,
            flex_items.len()
        );
        if flex_items.is_empty() {
            let result_params = FlexResultParams {
                container_inline_size,
                final_cross_size: container_cross_size,
                is_row,
                container_style: style.clone(),
            };
            return self.handle_empty_flex_container(
                &abspos_children,
                bfc_offset,
                sides,
                &result_params,
            );
        }

        // Sort items by order property (CSS Flexbox §7 - order property)
        Self::sort_flex_items_by_order(&mut flex_items, &mut child_styles);

        let main_size = if is_row {
            container_inline_size
        } else {
            container_cross_size
        };
        Self::update_flex_item_basis(&mut flex_items, &child_styles, is_row, main_size);
        let flexbox_params = Self::prepare_flexbox_params(
            flex_direction,
            container_inline_size,
            container_cross_size,
            is_row,
            style,
        );
        let placements =
            Self::run_flexbox_layout(&flex_items, &child_styles, &flexbox_params, style);
        let placement_params = FlexPlacementParams {
            content_base_inline: bfc_offset.inline_offset + sides.padding_left + sides.border_left,
            content_base_block: bfc_offset
                .block_offset
                .map(|y| y + sides.padding_top + sides.border_top),
            is_row,
        };
        let actual_cross_size =
            self.apply_flex_placements(&child_styles, &placements, &placement_params);
        let final_cross_size = if style.height.is_some() {
            container_cross_size
        } else {
            actual_cross_size
        };

        let result_params = FlexResultParams {
            container_inline_size,
            final_cross_size,
            is_row,
            container_style: style.clone(),
        };
        self.layout_flex_abspos_children(&abspos_children, bfc_offset, sides, &result_params);
        Self::create_flex_result(&result_params, bfc_offset, sides)
    }
}
