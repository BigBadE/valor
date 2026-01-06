//! Flexbox absolute positioning handling.

use super::ConstraintLayoutTree;
use super::shared::{
    ChildStyleInfo, FlexAbsposConstraintParams, FlexPlacementParams, FlexResultParams,
};
use css_box::{BoxSides, LayoutUnit, compute_box_sides};
use css_flexbox::{CrossPlacement, FlexPlacement};
use css_orchestrator::style_model::ComputedStyle;
use js::NodeKey;

use super::super::constraint_space::{AvailableSize, BfcOffset, ConstraintSpace, LayoutResult};
use super::super::exclusion_space::ExclusionSpace;
use super::super::margin_strut::MarginStrut;

impl ConstraintLayoutTree {
    pub(super) fn compute_static_main_offset(params: &FlexAbsposConstraintParams<'_>) -> f32 {
        if Self::has_explicit_inline_offset(params.child_style, params.is_row) {
            return 0.0;
        }

        if params.is_row {
            // Row: main axis is inline, justify-content applies
            Self::compute_flex_abspos_main_offset(
                params.container_style,
                params.child_style,
                params.child_sides,
                params.container_inline_size,
                params.is_row,
            )
        } else {
            // Column: main axis is block, justify-content still applies to block axis
            Self::compute_flex_abspos_main_offset(
                params.container_style,
                params.child_style,
                params.child_sides,
                params.final_cross_size,
                params.is_row,
            )
        }
    }

    /// Compute static cross offset for flex abspos child.
    pub(super) fn compute_static_cross_offset(params: &FlexAbsposConstraintParams<'_>) -> f32 {
        if Self::has_explicit_block_offset(params.child_style, params.is_row) {
            return 0.0;
        }

        if params.is_row {
            // Row: cross axis is block, align-items applies
            Self::compute_flex_abspos_cross_offset(
                params.container_style,
                params.child_style,
                params.child_sides,
                params.final_cross_size,
                params.is_row,
            )
        } else {
            // Column: cross axis is inline, align-items applies
            Self::compute_flex_abspos_cross_offset(
                params.container_style,
                params.child_style,
                params.child_sides,
                params.container_inline_size,
                params.is_row,
            )
        }
    }

    /// Create constraint space for abspos children in flex container.
    ///
    /// The containing block for absolutely positioned children of a flex container
    /// is the padding box of the flex container (content box + padding).
    pub(super) fn create_flex_abspos_constraint_space(
        params: &FlexAbsposConstraintParams<'_>,
    ) -> ConstraintSpace {
        // The containing block for abspos children starts at the content box
        // (i.e., inside padding and border)
        let content_inline_offset =
            params.bfc_offset.inline_offset + params.sides.padding_left + params.sides.border_left;
        let content_block_offset = params
            .bfc_offset
            .block_offset
            .map(|y| y + params.sides.padding_top + params.sides.border_top);

        // Compute static position offsets (only if child doesn't have explicit offsets)
        let static_main_offset = Self::compute_static_main_offset(params);
        let static_cross_offset = Self::compute_static_cross_offset(params);

        let content_bfc_offset = if params.is_row {
            BfcOffset::new(
                content_inline_offset + LayoutUnit::from_px(static_main_offset),
                content_block_offset.map(|y| y + LayoutUnit::from_px(static_cross_offset)),
            )
        } else {
            BfcOffset::new(
                content_inline_offset + LayoutUnit::from_px(static_cross_offset),
                content_block_offset.map(|y| y + LayoutUnit::from_px(static_main_offset)),
            )
        };

        ConstraintSpace {
            available_inline_size: AvailableSize::Definite(LayoutUnit::from_px(
                params.container_inline_size,
            )),
            available_block_size: AvailableSize::Definite(if params.is_row {
                LayoutUnit::from_px(params.final_cross_size)
            } else {
                LayoutUnit::from_px(params.container_inline_size)
            }),
            bfc_offset: content_bfc_offset,
            exclusion_space: ExclusionSpace::new(),
            margin_strut: MarginStrut::default(),
            is_new_formatting_context: true,
            percentage_resolution_block_size: Some(if params.is_row {
                LayoutUnit::from_px(params.final_cross_size)
            } else {
                LayoutUnit::from_px(params.container_inline_size)
            }),
            fragmentainer_block_size: None,
            fragmentainer_offset: LayoutUnit::zero(),
            is_for_measurement_only: false, // Abspos layout is final positioning
            margins_already_applied: false,
        }
    }

    /// Compute static position offset along the main axis for abspos child in flex container.
    pub(super) fn compute_flex_abspos_main_offset(
        container_style: &ComputedStyle,
        child_style: &ComputedStyle,
        child_sides: &BoxSides,
        container_main_size: f32,
        is_row: bool,
    ) -> f32 {
        use css_orchestrator::style_model::JustifyContent;

        // Get child's main size (border-box)
        let child_main_size = if is_row {
            child_style.width.map_or_else(
                || {
                    200.0
                        + child_sides.padding_left.to_px()
                        + child_sides.padding_right.to_px()
                        + child_sides.border_left.to_px()
                        + child_sides.border_right.to_px()
                },
                |width| {
                    width
                        + child_sides.padding_left.to_px()
                        + child_sides.padding_right.to_px()
                        + child_sides.border_left.to_px()
                        + child_sides.border_right.to_px()
                },
            )
        } else {
            child_style.height.unwrap_or(0.0)
                + child_sides.padding_top.to_px()
                + child_sides.padding_bottom.to_px()
                + child_sides.border_top.to_px()
                + child_sides.border_bottom.to_px()
        };

        // Apply justify-content to compute main axis offset
        match container_style.justify_content {
            JustifyContent::Center => (container_main_size - child_main_size) / 2.0,
            JustifyContent::FlexEnd => container_main_size - child_main_size,
            _ => 0.0, // FlexStart or other values default to start
        }
    }

    /// Compute static position offset along the cross axis for abspos child in flex container.
    pub(super) fn compute_flex_abspos_cross_offset(
        container_style: &ComputedStyle,
        child_style: &ComputedStyle,
        child_sides: &BoxSides,
        container_cross_size: f32,
        is_row: bool,
    ) -> f32 {
        use css_orchestrator::style_model::AlignItems;

        // Get child's cross size (border-box)
        let child_cross_size = if is_row {
            // Row: cross axis is block (height)
            child_style.height.unwrap_or(0.0)
                + child_sides.padding_top.to_px()
                + child_sides.padding_bottom.to_px()
                + child_sides.border_top.to_px()
                + child_sides.border_bottom.to_px()
        } else {
            // Column: cross axis is inline (width)
            child_style.width.map_or_else(
                || {
                    200.0
                        + child_sides.padding_left.to_px()
                        + child_sides.padding_right.to_px()
                        + child_sides.border_left.to_px()
                        + child_sides.border_right.to_px()
                },
                |width| {
                    width
                        + child_sides.padding_left.to_px()
                        + child_sides.padding_right.to_px()
                        + child_sides.border_left.to_px()
                        + child_sides.border_right.to_px()
                },
            )
        };

        // Apply align-items to compute cross axis offset
        match container_style.align_items {
            AlignItems::Center => (container_cross_size - child_cross_size) / 2.0,
            AlignItems::FlexEnd => container_cross_size - child_cross_size,
            _ => 0.0, // FlexStart or other values default to start
        }
    }

    /// Apply flex placements to children and compute actual cross size.
    pub(super) fn apply_flex_placements(
        &mut self,
        child_styles: &[ChildStyleInfo],
        placements: &[(FlexPlacement, CrossPlacement)],
        params: &FlexPlacementParams,
    ) -> f32 {
        let mut actual_cross_size = 0.0f32;

        for (idx, (child, child_style, _)) in child_styles.iter().enumerate() {
            if let Some((main_placement, cross_placement)) = placements.get(idx) {
                let child_sides = compute_box_sides(child_style);

                // Convert f32 coordinates to LayoutUnit to preserve sub-pixel precision
                // Margins are already in LayoutUnit from BoxSides
                let final_inline_offset = if params.is_row {
                    let offset = params.content_base_inline
                        + LayoutUnit::from_px(main_placement.main_offset);
                    log::debug!(
                        "[FLEX-PLACEMENT] Row: content_base={:.2} main_offset={:.2} final={:.2}",
                        params.content_base_inline.to_px(),
                        main_placement.main_offset,
                        offset.to_px()
                    );
                    offset
                } else {
                    params.content_base_inline
                        + LayoutUnit::from_px(cross_placement.cross_offset)
                        + child_sides.margin_left
                };

                let cross_with_margin =
                    LayoutUnit::from_px(cross_placement.cross_offset) + child_sides.margin_top;
                let final_block_offset = if params.is_row {
                    params.content_base_block.map(|y| y + cross_with_margin)
                } else {
                    params
                        .content_base_block
                        .map(|y| y + LayoutUnit::from_px(main_placement.main_offset))
                };

                let (final_inline_size, final_block_size) = if params.is_row {
                    (main_placement.main_size, cross_placement.cross_size)
                } else {
                    (cross_placement.cross_size, main_placement.main_size)
                };

                // Actually lay out the flex child with final sizes so its descendants
                // (like text nodes) get proper layout rects.
                let child_constraint_space = ConstraintSpace {
                    available_inline_size: AvailableSize::Definite(LayoutUnit::from_px(
                        final_inline_size,
                    )),
                    available_block_size: AvailableSize::Definite(LayoutUnit::from_px(
                        final_block_size,
                    )),
                    bfc_offset: BfcOffset::new(final_inline_offset, final_block_offset),
                    margin_strut: MarginStrut::default(),
                    exclusion_space: ExclusionSpace::new(),
                    is_new_formatting_context: false,
                    percentage_resolution_block_size: Some(LayoutUnit::from_px(final_block_size)),
                    fragmentainer_block_size: None,
                    fragmentainer_offset: LayoutUnit::zero(),
                    is_for_measurement_only: false, // Final layout, not measurement
                    margins_already_applied: true, // Flex algorithm already positioned items with margins
                };

                let final_child_result = self.layout_block(*child, &child_constraint_space);

                self.layout_results.insert(*child, final_child_result);
                actual_cross_size = actual_cross_size
                    .max(cross_placement.cross_offset + cross_placement.cross_size);
            }
        }

        actual_cross_size
    }

    /// Create empty flex container result (when no flex items).
    pub(super) fn create_empty_flex_result(
        container_inline_size: f32,
        container_cross_size: f32,
        bfc_offset: BfcOffset,
        sides: &BoxSides,
    ) -> LayoutResult {
        let border_box_inline = container_inline_size
            + sides.padding_left.to_px()
            + sides.padding_right.to_px()
            + sides.border_left.to_px()
            + sides.border_right.to_px();

        let border_box_block = container_cross_size
            + sides.padding_top.to_px()
            + sides.padding_bottom.to_px()
            + sides.border_top.to_px()
            + sides.border_bottom.to_px();

        LayoutResult {
            inline_size: border_box_inline,
            block_size: border_box_block,
            bfc_offset,
            exclusion_space: ExclusionSpace::new(),
            end_margin_strut: MarginStrut::default(),
            baseline: None,
            needs_relayout: false,
        }
    }

    /// Layout absolutely positioned children in flex container.
    pub(super) fn layout_flex_abspos_children(
        &mut self,
        abspos_children: &[NodeKey],
        bfc_offset: BfcOffset,
        container_sides: &BoxSides,
        params: &FlexResultParams,
    ) {
        let container_style = params.container_style.clone();
        for abspos_child in abspos_children {
            let abspos_child_style = self.style(*abspos_child);
            let abspos_child_sides = compute_box_sides(&abspos_child_style);

            let abspos_params = FlexAbsposConstraintParams {
                bfc_offset,
                sides: container_sides,
                container_style: &container_style,
                child_style: &abspos_child_style,
                child_sides: &abspos_child_sides,
                container_inline_size: params.container_inline_size,
                final_cross_size: params.final_cross_size,
                is_row: params.is_row,
            };
            let abspos_space = Self::create_flex_abspos_constraint_space(&abspos_params);

            let abspos_result = self.layout_absolute(
                *abspos_child,
                &abspos_space,
                &abspos_child_style,
                &abspos_child_sides,
            );

            self.layout_results.insert(*abspos_child, abspos_result);
        }
    }

    /// Create final flex container result.
    pub(super) fn create_flex_result(
        params: &FlexResultParams,
        bfc_offset: BfcOffset,
        sides: &BoxSides,
    ) -> LayoutResult {
        // Both row and column use the same calculation:
        // border_box_inline is always CSS width (container_inline_size + horizontal edges)
        // border_box_block is always CSS height (final_cross_size + vertical edges)
        let (border_box_inline, border_box_block) = (
            params.container_inline_size
                + sides.padding_left.to_px()
                + sides.padding_right.to_px()
                + sides.border_left.to_px()
                + sides.border_right.to_px(),
            params.final_cross_size
                + sides.padding_top.to_px()
                + sides.padding_bottom.to_px()
                + sides.border_top.to_px()
                + sides.border_bottom.to_px(),
        );

        LayoutResult {
            inline_size: border_box_inline,
            block_size: border_box_block,
            bfc_offset,
            exclusion_space: ExclusionSpace::new(),
            end_margin_strut: MarginStrut::default(),
            baseline: None,
            needs_relayout: false,
        }
    }
}
