//! Flexbox main layout algorithm.

use super::ConstraintLayoutTree;
use super::shared::{ChildStyleInfo, FlexPlacementParams, FlexResultParams, FlexboxLayoutParams};
use css_box::BoxSides;
use css_display::normalize_children;
use css_flexbox::{
    AlignContent as FlexAlignContent, AlignItems as FlexAlignItems, CrossAndBaseline, CrossContext,
    CrossPlacement, FlexChild, FlexContainerInputs, FlexDirection as FlexboxDirection,
    FlexPlacement, JustifyContent as FlexJustifyContent, WritingMode, layout_multi_line_with_cross,
    layout_single_line_with_cross,
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
        FlexContainerInputs {
            direction: flex_direction,
            writing_mode: WritingMode::HorizontalTb,
            container_main_size: if is_row {
                container_inline_size
            } else {
                container_cross_size
            },
            main_gap: if is_row {
                style.column_gap
            } else {
                style.row_gap
            },
        }
    }

    /// Run flexbox layout algorithm and get placements.
    pub(super) fn run_flexbox_layout(
        flex_items: &[FlexChild],
        child_styles: &[ChildStyleInfo],
        params: &FlexboxLayoutParams,
        style: &ComputedStyle,
    ) -> Vec<(FlexPlacement, CrossPlacement)> {
        type BaselineInput = Option<(f32, f32)>;

        let align_items = match style.align_items {
            StyleAlignItems::Normal | StyleAlignItems::Stretch => FlexAlignItems::Stretch,
            StyleAlignItems::FlexStart => FlexAlignItems::FlexStart,
            StyleAlignItems::Center => FlexAlignItems::Center,
            StyleAlignItems::FlexEnd => FlexAlignItems::FlexEnd,
        };

        let justify_content = match style.justify_content {
            StyleJustifyContent::FlexStart => FlexJustifyContent::Start,
            StyleJustifyContent::Center => FlexJustifyContent::Center,
            StyleJustifyContent::FlexEnd => FlexJustifyContent::End,
            StyleJustifyContent::SpaceBetween => FlexJustifyContent::SpaceBetween,
            StyleJustifyContent::SpaceAround => FlexJustifyContent::SpaceAround,
            StyleJustifyContent::SpaceEvenly => FlexJustifyContent::SpaceEvenly,
        };

        let align_content = match style.align_content {
            StyleAlignContent::Stretch => FlexAlignContent::Stretch,
            StyleAlignContent::FlexStart => FlexAlignContent::Start,
            StyleAlignContent::Center => FlexAlignContent::Center,
            StyleAlignContent::FlexEnd => FlexAlignContent::End,
            StyleAlignContent::SpaceBetween => FlexAlignContent::SpaceBetween,
            StyleAlignContent::SpaceAround => FlexAlignContent::SpaceAround,
            StyleAlignContent::SpaceEvenly => FlexAlignContent::SpaceEvenly,
        };

        let cross_inputs: Vec<(f32, f32, f32)> = child_styles
            .iter()
            .enumerate()
            .map(|(_idx, (_child, _, result))| {
                let cross_size = if params.is_row {
                    result.block_size
                } else {
                    result.inline_size
                };
                (cross_size, 0.0, 1e9)
            })
            .collect();

        let baseline_inputs: Vec<BaselineInput> = vec![None; flex_items.len()];

        let cross_ctx = CrossContext {
            align_items,
            align_content,
            container_cross_size: if params.is_row {
                params.container_cross_size
            } else {
                params.container_inline_size
            },
            cross_gap: if params.is_row {
                style.row_gap
            } else {
                style.column_gap
            },
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

    /// Layout a flex container using the proper flexbox algorithm.
    pub(super) fn layout_flex(
        &mut self,
        node: NodeKey,
        constraint_space: &ConstraintSpace,
        style: &ComputedStyle,
        sides: &BoxSides,
    ) -> LayoutResult {
        let container_inline_size = self.compute_inline_size(node, constraint_space, style, sides);
        let container_cross_size = Self::compute_flex_container_cross_size(style, sides, constraint_space);

        let bfc_offset = BfcOffset::new(
            constraint_space.bfc_offset.inline_offset + sides.margin_left,
            constraint_space.bfc_offset.block_offset,
        );

        let flex_direction = match style.flex_direction {
            StyleFlexDirection::Row => FlexboxDirection::Row,
            StyleFlexDirection::Column => FlexboxDirection::Column,
        };

        let is_row = matches!(flex_direction, FlexboxDirection::Row);
        let children = normalize_children(&self.children, &self.styles, node);

        let (mut flex_items, child_styles, abspos_children) =
            self.build_flex_items(node, &children);

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

        Self::update_flex_item_basis(&mut flex_items, &child_styles, is_row);

        let container_inputs = Self::prepare_flex_container_inputs(
            flex_direction,
            container_inline_size,
            container_cross_size,
            is_row,
            style,
        );

        let flexbox_params = FlexboxLayoutParams {
            container_inputs,
            is_row,
            container_inline_size,
            container_cross_size,
        };

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
