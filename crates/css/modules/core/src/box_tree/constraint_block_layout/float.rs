//! Float layout for block layout.

use super::ConstraintLayoutTree;
use css_box::{BoxSides, LayoutUnit};
use css_display::normalize_children;
use css_orchestrator::style_model::{BoxSizing, ComputedStyle, Float};
use js::NodeKey;

use super::super::constraint_space::{AvailableSize, BfcOffset, ConstraintSpace, LayoutResult};
use super::super::exclusion_space::{ExclusionSpace, FloatSize};
use super::super::margin_strut::MarginStrut;

impl ConstraintLayoutTree {
    pub(super) fn compute_float_inline_offset(
        constraint_space: &ConstraintSpace,
        style: &ComputedStyle,
        sides: &BoxSides,
        border_box_inline: f32,
        container_inline_size: LayoutUnit,
    ) -> LayoutUnit {
        let base_inline_offset = constraint_space.bfc_offset.inline_offset + sides.margin_left;

        match style.float {
            Float::Right => {
                constraint_space.bfc_offset.inline_offset + container_inline_size
                    - LayoutUnit::from_px(border_box_inline.round())
                    - sides.margin_right
            }
            Float::Left | Float::None => base_inline_offset,
        }
    }

    /// Layout float's children and compute content height.
    pub(super) fn layout_float_children(
        &mut self,
        node: NodeKey,
        inline_size: f32,
        constraint_space: &ConstraintSpace,
        sides: &BoxSides,
    ) -> f32 {
        let child_space = ConstraintSpace {
            available_inline_size: AvailableSize::Definite(LayoutUnit::from_px(
                inline_size.max(0.0),
            )),
            available_block_size: AvailableSize::Indefinite,
            bfc_offset: BfcOffset::root(),
            exclusion_space: ExclusionSpace::new(),
            margin_strut: MarginStrut::default(),
            is_new_formatting_context: true,
            percentage_resolution_block_size: constraint_space.percentage_resolution_block_size,
            fragmentainer_block_size: constraint_space.fragmentainer_block_size,
            fragmentainer_offset: constraint_space.fragmentainer_offset,
            is_for_measurement_only: constraint_space.is_for_measurement_only, // Propagate measurement flag
        };

        // Normalize children to handle display:none and display:contents
        let children = normalize_children(&self.children, &self.styles, node);
        let mut content_height = 0.0f32;

        for child in children {
            let child_result = self.layout_block(child, &child_space);
            content_height = content_height.max(
                child_result
                    .bfc_offset
                    .block_offset
                    .unwrap_or(LayoutUnit::zero())
                    .to_px()
                    + child_result.block_size,
            );
            self.layout_results.insert(child, child_result);
        }

        content_height
            + sides.padding_top.to_px()
            + sides.padding_bottom.to_px()
            + sides.border_top.to_px()
            + sides.border_bottom.to_px()
    }

    /// Layout a float element.
    pub(super) fn layout_float(
        &mut self,
        node: NodeKey,
        constraint_space: &ConstraintSpace,
        style: &ComputedStyle,
        sides: &BoxSides,
    ) -> LayoutResult {
        let inline_size = style.width.unwrap_or_else(|| {
            (constraint_space
                .available_inline_size
                .resolve(LayoutUnit::from_px(400.0))
                .to_px()
                / 2.0)
                .max(100.0)
        });

        let container_inline_size = constraint_space
            .available_inline_size
            .resolve(self.icb_width);
        let border_box_inline = inline_size
            + sides.padding_left.to_px()
            + sides.padding_right.to_px()
            + sides.border_left.to_px()
            + sides.border_right.to_px();

        let inline_offset = Self::compute_float_inline_offset(
            constraint_space,
            style,
            sides,
            border_box_inline,
            container_inline_size,
        );

        let float_bfc_offset =
            BfcOffset::new(inline_offset, constraint_space.bfc_offset.block_offset);

        // Compute border-box height with box-sizing and constraints
        let block_size = style.height.map_or_else(
            || {
                // Auto height: layout children (already returns border-box)
                let border_box_height =
                    self.layout_float_children(node, inline_size, constraint_space, sides);
                Self::apply_height_constraints(border_box_height, style, sides)
            },
            |height| {
                let border_box_height = match style.box_sizing {
                    BoxSizing::ContentBox => {
                        let padding_border = sides.padding_top.to_px()
                            + sides.padding_bottom.to_px()
                            + sides.border_top.to_px()
                            + sides.border_bottom.to_px();
                        height + padding_border
                    }
                    BoxSizing::BorderBox => height,
                };
                Self::apply_height_constraints(border_box_height, style, sides)
            },
        );

        let mut updated_exclusion = constraint_space.exclusion_space.clone();
        updated_exclusion.add_float(
            node,
            float_bfc_offset,
            FloatSize {
                inline_size: LayoutUnit::from_px(
                    (border_box_inline + sides.margin_left.to_px() + sides.margin_right.to_px())
                        .round(),
                ),
                block_size: LayoutUnit::from_px(
                    (block_size + sides.margin_top.to_px() + sides.margin_bottom.to_px()).round(),
                ),
                float_type: style.float,
            },
        );

        LayoutResult {
            inline_size: border_box_inline,
            block_size,
            bfc_offset: float_bfc_offset,
            exclusion_space: updated_exclusion,
            end_margin_strut: MarginStrut::default(),
            baseline: None,
            needs_relayout: false,
        }
    }
}
