//! Grid layout integration for block layout.

use super::super::grid_template_parser;
use super::ConstraintLayoutTree;
use css_box::{BoxSides, LayoutUnit, compute_box_sides};
use css_display::normalize_children;
use css_grid::{
    GridAlignment, GridAutoFlow, GridAxisTracks, GridContainerInputs, GridItem, GridLayoutResult,
    GridPlacedItem, GridTrack, GridTrackSize, TrackBreadth, TrackListType, layout_grid,
};
use css_orchestrator::style_model::{ComputedStyle, GridAutoFlow as StyleGridAutoFlow};
use css_text::measure_text;
use js::NodeKey;

use super::super::constraint_space::{AvailableSize, BfcOffset, ConstraintSpace, LayoutResult};
use super::super::exclusion_space::ExclusionSpace;
use super::super::margin_strut::MarginStrut;

impl ConstraintLayoutTree {
    pub(super) fn parse_grid_template(template: &str, gap: f32) -> GridAxisTracks {
        grid_template_parser::parse_grid_template(template, gap)
    }

    /// Parse grid track templates from style.
    pub(super) fn parse_grid_tracks(style: &ComputedStyle) -> (GridAxisTracks, GridAxisTracks) {
        let col_tracks = style.grid_template_columns.as_ref().map_or_else(
            || {
                GridAxisTracks::new(
                    vec![GridTrack {
                        size: GridTrackSize::Breadth(TrackBreadth::Flex(1.0)),
                        track_type: TrackListType::Explicit,
                    }],
                    style.column_gap,
                )
            },
            |template| Self::parse_grid_template(template, style.column_gap),
        );

        let row_tracks = style.grid_template_rows.as_ref().map_or_else(
            || {
                GridAxisTracks::new(
                    vec![GridTrack {
                        size: GridTrackSize::Breadth(TrackBreadth::Auto),
                        track_type: TrackListType::Explicit,
                    }],
                    style.row_gap,
                )
            },
            |template| Self::parse_grid_template(template, style.row_gap),
        );

        (col_tracks, row_tracks)
    }

    /// Run spec-compliant grid layout algorithm.
    ///
    /// Implements the CSS Grid Level 2 two-pass sizing algorithm:
    /// 1. Size column tracks (items measured at indefinite block size)
    /// 2. Size row tracks (items re-measured at definite column widths)
    ///
    /// This correctly handles text wrapping: items are measured at their actual
    /// column widths for row sizing, so text wraps properly and row heights are correct.
    ///
    /// [Spec: CSS Grid Layout Module Level 2 §12 Grid Sizing Algorithm]
    /// <https://www.w3.org/TR/css-grid-2/#layout-algorithm>
    ///
    /// # Parameters
    ///
    /// - `node`: The grid container node
    /// - `grid_inputs`: Grid container configuration
    /// - `is_final_layout`: If true, perform two-pass layout. If false, use single-pass approximation.
    ///
    /// # Errors
    /// Returns an error if grid layout computation fails.
    fn run_spec_compliant_grid_layout(
        &mut self,
        node: NodeKey,
        grid_inputs: &GridContainerInputs,
        is_final_layout: bool,
    ) -> Result<GridLayoutResult<NodeKey>, String> {
        // PASS 1: Size columns
        // Items measured with indefinite block size to get column widths
        let mut grid_items = self.prepare_grid_items_for_columns(node);

        let column_result = layout_grid(&grid_items, grid_inputs)?;

        // PASS 2: Size rows (only for final layout)
        // During measurement, we skip this to avoid exponential blowup from nested grids
        if is_final_layout {
            // Re-measure items at their actual column widths to get correct heights
            self.remeasure_items_for_rows(&mut grid_items, &column_result.items);

            // Run layout again with correct row heights
            let final_result = layout_grid(&grid_items, grid_inputs)?;

            Ok(final_result)
        } else {
            // For measurement/nested grids, use column_result directly
            // Heights will be approximate but avoid infinite recursion
            Ok(column_result)
        }
    }

    /// Measure the max-content width of a grid item.
    ///
    /// For block containers, this recursively measures the widest child content.
    /// This gives us the natural width of the content for grid auto-sizing.
    fn measure_grid_item_content_width(&mut self, node: NodeKey) -> f32 {
        // Get children of this node (clone to avoid borrow conflicts)
        let children = self.children.get(&node).cloned();
        let Some(children) = children else {
            return 0.0;
        };

        let mut max_width = 0.0f32;

        for child_key in children {
            // If it's a text node, measure the text width
            if let Some(text) = self.text_nodes.get(&child_key).cloned() {
                if let Some(style) = self.styles.get(&node) {
                    // Measure text at its natural (unwrapped) width
                    let metrics = measure_text(&text, style);
                    max_width = max_width.max(metrics.width);
                }
            } else {
                // Recursively measure child elements
                let child_width = self.measure_grid_item_content_width(child_key);
                max_width = max_width.max(child_width);
            }
        }

        // Add padding and borders from this node's style
        if let Some(style) = self.styles.get(&node) {
            let sides = compute_box_sides(style);
            max_width += sides.padding_left.to_px() + sides.padding_right.to_px();
            max_width += sides.border_left.to_px() + sides.border_right.to_px();
        }

        max_width
    }

    /// Prepare grid items for column sizing (pass 1).
    ///
    /// Items are measured with indefinite inline size and indefinite block size
    /// to determine their natural widths for column sizing.
    ///
    /// Heights from this pass are NOT used - they will be wrong for wrapping text.
    ///
    /// [Spec: CSS Grid Layout Module Level 2 §12 Grid Sizing Algorithm]
    /// <https://www.w3.org/TR/css-grid-2/#layout-algorithm>
    fn prepare_grid_items_for_columns(&mut self, node: NodeKey) -> Vec<GridItem<NodeKey>> {
        let normalized_children = normalize_children(&self.children, &self.styles, node);

        // Filter out whitespace-only text nodes (CSS Grid spec §5.1)
        let normalized_children: Vec<NodeKey> = normalized_children
            .into_iter()
            .filter(|child| {
                self.text_nodes
                    .get(child)
                    .is_none_or(|text| !text.trim().is_empty())
            })
            .collect();

        // Measure items for column sizing
        normalized_children
            .iter()
            .map(|&child| {
                let mut item = GridItem::new(child);

                // Measure content width for grid auto-sizing
                // For block containers, we need to measure the max-content width of children
                // rather than laying out at indefinite size (which gives container width)
                let content_width = self.measure_grid_item_content_width(child);

                // Measure height at indefinite size for initial estimate
                let size =
                    self.measure_item(child, AvailableSize::Indefinite, AvailableSize::Indefinite);

                // For column sizing, use measured content width
                item.min_content_width = content_width;
                item.max_content_width = content_width;

                // Heights here are from indefinite measurement and will be wrong for
                // wrapping content at constrained widths. We'll re-measure in pass 2.
                item.min_content_height = size.block;
                item.max_content_height = size.block;

                item
            })
            .collect()
    }

    /// Re-measure grid items for row sizing (pass 2).
    ///
    /// After column widths are determined, measure each item's height at its
    /// actual column width. This gives correct heights for wrapping text.
    ///
    /// [Spec: CSS Grid Layout Module Level 2 §12 Grid Sizing Algorithm]
    /// <https://www.w3.org/TR/css-grid-2/#layout-algorithm>
    fn remeasure_items_for_rows(
        &mut self,
        items: &mut [GridItem<NodeKey>],
        placements: &[GridPlacedItem<NodeKey>],
    ) {
        for (item, placement) in items.iter_mut().zip(placements.iter()) {
            // Measure height at the actual column width from pass 1
            let height = self.measure_block_at_inline(item.node_id, placement.width);

            // Update both min and max height with the same value
            // (we're measuring at a definite width, so there's only one result)
            item.min_content_height = height;
            item.max_content_height = height;
        }
    }

    /// Layout grid items and store their results.
    pub(super) fn layout_grid_items(
        &mut self,
        grid_result: &GridLayoutResult<NodeKey>,
        bfc_offset: BfcOffset,
        sides: &BoxSides,
        constraint_space: &ConstraintSpace,
    ) {
        for placed_item in &grid_result.items {
            let node_key = placed_item.node_id;

            // Create constraint space for this grid item
            // Grid area provides available space, but item sizes to content unless stretched
            let item_inline_size = LayoutUnit::from_px(placed_item.width);
            let item_block_size = LayoutUnit::from_px(placed_item.height);

            let item_bfc_inline = bfc_offset.inline_offset
                + LayoutUnit::from_px(
                    sides.padding_left.to_px() + sides.border_left.to_px() + placed_item.x,
                );

            let item_bfc_block = bfc_offset.block_offset.unwrap_or(LayoutUnit::zero())
                + LayoutUnit::from_px(
                    sides.padding_top.to_px() + sides.border_top.to_px() + placed_item.y,
                );

            let item_constraint = ConstraintSpace {
                available_inline_size: AvailableSize::Definite(item_inline_size),
                available_block_size: AvailableSize::Definite(item_block_size),
                percentage_resolution_block_size: constraint_space.percentage_resolution_block_size,
                bfc_offset: BfcOffset::new(item_bfc_inline, Some(item_bfc_block)),
                exclusion_space: ExclusionSpace::new(),
                margin_strut: MarginStrut::default(),
                is_new_formatting_context: true,
                fragmentainer_block_size: None,
                fragmentainer_offset: LayoutUnit::zero(),
                is_for_measurement_only: false, // Grid item final layout is not measurement
                margins_already_applied: true, // Grid layout algorithm already positioned items with margins
            };

            // Layout the grid item to compute its internal layout
            let item_result = self.layout_block(node_key, &item_constraint);

            // Determine final size based on item's explicit sizing
            // If item has no explicit height, it should stretch to grid area (default align-self: stretch)
            // If item has explicit height, use its natural size
            let style = &self.styles[&node_key];
            let has_explicit_height = style.height.is_some();
            let has_explicit_width = style.width.is_some();

            let final_inline_size = if has_explicit_width {
                item_result.inline_size
            } else {
                // Stretch to fill grid area width (default justify-self: stretch)
                placed_item.width
            };

            let final_block_size = if has_explicit_height {
                item_result.block_size
            } else {
                // Stretch to fill grid area height (default align-self: stretch)
                placed_item.height
            };

            let grid_item_result = LayoutResult {
                inline_size: final_inline_size,
                block_size: final_block_size,
                bfc_offset: BfcOffset::new(item_bfc_inline, Some(item_bfc_block)),
                exclusion_space: item_result.exclusion_space,
                end_margin_strut: MarginStrut::default(),
                baseline: item_result.baseline,
                needs_relayout: false,
            };

            self.layout_results.insert(node_key, grid_item_result);
        }
    }

    /// Compute grid container height from style, considering min-height for track sizing.
    ///
    /// Returns (content-box height, whether min-height constrains sizing).
    fn compute_grid_container_height(
        style: &ComputedStyle,
        constraint_space: &ConstraintSpace,
        block_pbsum: f32,
    ) -> (f32, bool) {
        style.height.map_or_else(
            || {
                style.min_height.map_or_else(
                    || {
                        // No height or min-height, use available space
                        (
                            constraint_space
                                .available_block_size
                                .resolve(LayoutUnit::from_px(10000.0))
                                .to_px(),
                            false,
                        )
                    },
                    |min_height| {
                        // No explicit height, but min-height acts as definite size for grid sizing
                        // Per CSS Grid spec, min-height establishes definite size for track sizing
                        // Convert from border-box to content-box
                        (min_height - block_pbsum, true)
                    },
                )
            },
            |height| {
                // Explicit height is set - convert from border-box to content-box
                (height - block_pbsum, false)
            },
        )
    }

    /// Layout a grid container.
    pub(super) fn layout_grid_container(
        &mut self,
        node: NodeKey,
        constraint_space: &ConstraintSpace,
        style: &ComputedStyle,
        sides: &BoxSides,
    ) -> LayoutResult {
        // Compute container size
        let container_inline_size = self.compute_inline_size(node, constraint_space, style, sides);

        // Resolve BFC offset properly to handle incoming margins
        // Grid containers establish a new BFC, so we need to collapse incoming margin strut
        let (bfc_offset, _can_collapse_with_children) =
            Self::resolve_bfc_offset(constraint_space, style, sides, true);

        // Determine if this is measurement or final layout using the explicit flag
        // This avoids ambiguity between root layout and nested measurement
        let is_measurement = constraint_space.is_for_measurement_only;

        // Parse grid properties
        let (col_tracks, row_tracks) = Self::parse_grid_tracks(style);

        // Compute padding/border before grid sizing (grid expects content-box dimensions)
        let padding_block_sum = sides.padding_top.to_px() + sides.padding_bottom.to_px();
        let border_block_sum = sides.border_top.to_px() + sides.border_bottom.to_px();
        let block_pbsum = padding_block_sum + border_block_sum;

        // Run grid layout
        // Determine container height and whether it's explicit for grid track sizing
        let (container_block_size, has_min_height_constraint) =
            Self::compute_grid_container_height(style, constraint_space, block_pbsum);

        // Convert GridAutoFlow from style model to grid module type
        let grid_auto_flow = match style.grid_auto_flow {
            StyleGridAutoFlow::Row => GridAutoFlow::Row,
            StyleGridAutoFlow::Column => GridAutoFlow::Column,
            StyleGridAutoFlow::RowDense => GridAutoFlow::RowDense,
            StyleGridAutoFlow::ColumnDense => GridAutoFlow::ColumnDense,
        };

        let _padding_inline_sum = sides.padding_left.to_px() + sides.padding_right.to_px();
        let _border_inline_sum = sides.border_left.to_px() + sides.border_right.to_px();
        // padding_block_sum and border_block_sum already computed above for grid sizing

        // Run spec-compliant grid layout (column sizing, then row sizing)
        let grid_inputs = GridContainerInputs {
            row_tracks,
            col_tracks,
            auto_flow: grid_auto_flow,
            available_width: container_inline_size,
            available_height: container_block_size,
            align_items: GridAlignment::default(),
            justify_items: GridAlignment::default(),
            // Height is explicit if set directly or if min-height constrains the grid
            has_explicit_height: style.height.is_some() || has_min_height_constraint,
        };

        // Use two-pass layout only for final layout, not during measurement
        // This avoids exponential blowup from nested grids
        let use_two_pass = !is_measurement;

        let grid_result =
            match self.run_spec_compliant_grid_layout(node, &grid_inputs, use_two_pass) {
                Ok(result) => result,
                Err(error) => {
                    tracing::error!("Grid layout failed: {}", error);
                    return LayoutResult {
                        inline_size: container_inline_size,
                        block_size: 0.0,
                        bfc_offset,
                        exclusion_space: constraint_space.exclusion_space.clone(),
                        end_margin_strut: MarginStrut::default(),
                        baseline: None,
                        needs_relayout: false,
                    };
                }
            };

        self.layout_grid_items(&grid_result, bfc_offset, sides, constraint_space);

        // Compute final container size
        let content_block = grid_result.total_height;

        // Convert content-box width to border-box (matching block layout behavior)
        let padding_inline_sum = sides.padding_left.to_px() + sides.padding_right.to_px();
        let border_inline_sum = sides.border_left.to_px() + sides.border_right.to_px();
        let final_inline_size = container_inline_size + padding_inline_sum + border_inline_sum;
        let mut final_block_size = content_block + padding_block_sum + border_block_sum;

        // Apply min-height and max-height constraints
        if let Some(min_height) = style.min_height {
            final_block_size = final_block_size.max(min_height);
        }
        if let Some(max_height) = style.max_height {
            final_block_size = final_block_size.min(max_height);
        }

        LayoutResult {
            inline_size: final_inline_size,
            block_size: final_block_size,
            bfc_offset,
            exclusion_space: constraint_space.exclusion_space.clone(),
            end_margin_strut: MarginStrut::default(),
            baseline: None,
            needs_relayout: false,
        }
    }
}
