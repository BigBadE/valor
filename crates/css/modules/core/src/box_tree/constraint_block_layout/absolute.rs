//! Absolute positioning for block layout.

use super::ConstraintLayoutTree;
use css_box::{BoxSides, LayoutUnit};
use css_display::normalize_children;
use css_orchestrator::style_model::{BoxSizing, ComputedStyle, Position};
use js::NodeKey;

use super::super::constraint_space::{AvailableSize, BfcOffset, ConstraintSpace, LayoutResult};
use super::super::exclusion_space::ExclusionSpace;
use super::super::margin_strut::MarginStrut;

impl ConstraintLayoutTree {
    pub(super) fn layout_absolute_children(
        &mut self,
        node: NodeKey,
        inline_size: f32,
        constraint_space: &ConstraintSpace,
    ) -> f32 {
        let child_space = ConstraintSpace {
            available_inline_size: AvailableSize::Definite(LayoutUnit::from_px(inline_size)),
            available_block_size: AvailableSize::Indefinite,
            bfc_offset: BfcOffset::root(),
            exclusion_space: ExclusionSpace::new(),
            margin_strut: MarginStrut::default(),
            is_new_formatting_context: true,
            percentage_resolution_block_size: constraint_space.percentage_resolution_block_size,
            fragmentainer_block_size: constraint_space.fragmentainer_block_size,
            fragmentainer_offset: constraint_space.fragmentainer_offset,
            is_for_measurement_only: constraint_space.is_for_measurement_only, // Propagate measurement flag
            margins_already_applied: false,
            is_block_size_forced: false,
            is_inline_size_forced: false,
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
    }

    /// Compute absolute positioning offset based on containing block and style.
    pub(super) fn compute_abspos_offset(
        &self,
        constraint_space: &ConstraintSpace,
        style: &ComputedStyle,
        sides: &BoxSides,
    ) -> BfcOffset {
        // Determine containing block based on position type
        let (containing_block_inline, containing_block_block) =
            if matches!(style.position, Position::Fixed) {
                (LayoutUnit::zero(), LayoutUnit::zero())
            } else {
                (
                    constraint_space.bfc_offset.inline_offset,
                    constraint_space
                        .bfc_offset
                        .block_offset
                        .unwrap_or(LayoutUnit::zero()),
                )
            };

        // Apply positioning offsets (left, top, right, bottom)
        let mut inline_offset = containing_block_inline;
        let mut block_offset = containing_block_block;

        // Apply left offset if specified
        if let Some(left) = style.left {
            inline_offset += LayoutUnit::from_px(left);
        } else if let Some(left_percent) = style.left_percent {
            let cb_width = match constraint_space.available_inline_size {
                AvailableSize::Definite(width) => width,
                _ => self.icb_width,
            };
            inline_offset += cb_width * left_percent;
        }

        // Apply top offset if specified
        if let Some(top) = style.top {
            block_offset += LayoutUnit::from_px(top);
        } else if let Some(top_percent) = style.top_percent {
            let cb_height = constraint_space
                .percentage_resolution_block_size
                .unwrap_or(self.icb_height);
            block_offset += cb_height * top_percent;
        }

        // Add margins to the final position
        inline_offset += sides.margin_left;
        block_offset += sides.margin_top;

        BfcOffset::new(inline_offset, Some(block_offset))
    }

    /// Layout an absolutely positioned element.
    pub(super) fn layout_absolute(
        &mut self,
        node: NodeKey,
        constraint_space: &ConstraintSpace,
        style: &ComputedStyle,
        sides: &BoxSides,
    ) -> LayoutResult {
        // Absolutely positioned elements establish BFC

        // Compute border-box width with box-sizing (match height logic below)
        let border_box_inline = style.width.map_or_else(
            || {
                // Auto width for abspos: shrink to fit (simplified to 200.0 content + padding/border)
                200.0
                    + sides.padding_left.to_px()
                    + sides.padding_right.to_px()
                    + sides.border_left.to_px()
                    + sides.border_right.to_px()
            },
            |width| match style.box_sizing {
                BoxSizing::ContentBox => {
                    let padding_border = sides.padding_left.to_px()
                        + sides.padding_right.to_px()
                        + sides.border_left.to_px()
                        + sides.border_right.to_px();
                    width + padding_border
                }
                BoxSizing::BorderBox => width,
            },
        );

        // Compute content-box width for laying out children
        let content_inline_size = match style.box_sizing {
            BoxSizing::ContentBox => style.width.unwrap_or(200.0),
            BoxSizing::BorderBox => {
                let padding_border = sides.padding_left.to_px()
                    + sides.padding_right.to_px()
                    + sides.border_left.to_px()
                    + sides.border_right.to_px();
                style.width.unwrap_or(200.0 + padding_border) - padding_border
            }
        };

        // Compute border-box height with box-sizing and constraints
        let block_size = style.height.map_or_else(
            || {
                // Auto height: layout children
                let content_height =
                    self.layout_absolute_children(node, content_inline_size, constraint_space);
                let border_box_height = content_height
                    + sides.padding_top.to_px()
                    + sides.padding_bottom.to_px()
                    + sides.border_top.to_px()
                    + sides.border_bottom.to_px();
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

        let abspos_bfc_offset = self.compute_abspos_offset(constraint_space, style, sides);

        // Abspos doesn't affect normal flow, so don't add to exclusion space
        LayoutResult {
            inline_size: border_box_inline,
            block_size,
            bfc_offset: abspos_bfc_offset,
            exclusion_space: constraint_space.exclusion_space.clone(),
            end_margin_strut: MarginStrut::default(),
            baseline: None,
            needs_relayout: false,
        }
    }
}
