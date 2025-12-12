//! Grid layout integration for block layout.

use super::super::grid_template_parser;
use super::ConstraintLayoutTree;
use css_box::{BoxSides, LayoutUnit};
use css_display::normalize_children;
use css_grid::{
    GridAlignment, GridAutoFlow, GridAxisTracks, GridContainerInputs, GridItem, GridLayoutResult,
    GridPlacedItem, GridTrack, GridTrackSize, TrackBreadth, TrackListType, layout_grid,
};
use css_orchestrator::style_model::{ComputedStyle, GridAutoFlow as StyleGridAutoFlow};
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

        tracing::debug!(
            "Grid layout pass 1 (columns): items_count={}, is_final={}",
            grid_items.len(),
            is_final_layout
        );

        let column_result = layout_grid(&grid_items, grid_inputs)?;

        // PASS 2: Size rows (only for final layout)
        // During measurement, we skip this to avoid exponential blowup from nested grids
        if is_final_layout {
            tracing::info!("Grid layout PASS 2: Starting row sizing (is_final_layout=true)");

            // Re-measure items at their actual column widths to get correct heights
            self.remeasure_items_for_rows(&mut grid_items, &column_result.items);

            tracing::info!("Grid layout pass 2 (rows): re-measured items, calling layout_grid again");

            // Run layout again with correct row heights
            let final_result = layout_grid(&grid_items, grid_inputs)?;

            tracing::info!(
                "Grid layout PASS 2 complete: total_height={:.1}px, row_count={}",
                final_result.total_height,
                final_result.row_sizes.base_sizes.len()
            );
            for (idx, &row_height) in final_result.row_sizes.base_sizes.iter().enumerate() {
                tracing::info!("  Row {}: height={:.1}px", idx, row_height);
            }

            Ok(final_result)
        } else {
            tracing::info!("Grid layout: Skipping PASS 2 (is_final_layout=false, measurement mode)");
            // For measurement/nested grids, use column_result directly
            // Heights will be approximate but avoid infinite recursion
            Ok(column_result)
        }
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

                // Measure at indefinite size for natural dimensions
                // We use measure_item directly to get both width and height
                let size =
                    self.measure_item(child, AvailableSize::Indefinite, AvailableSize::Indefinite);

                // For column sizing, we use natural width as both min and max content
                // This is a simplification - proper implementation would measure at
                // different wrapping constraints
                item.min_content_width = size.inline;
                item.max_content_width = size.inline;

                // Heights here are from indefinite measurement and will be wrong for
                // wrapping content at constrained widths. We'll re-measure in pass 2.
                item.min_content_height = size.block;
                item.max_content_height = size.block;

                tracing::debug!(
                    "Grid item {:?} (column sizing): w={}, h={}",
                    child,
                    item.max_content_width,
                    item.max_content_height
                );

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
        tracing::info!(
            "=== REMEASURE_ITEMS_FOR_ROWS: {} items ===",
            items.len()
        );

        for (item, placement) in items.iter_mut().zip(placements.iter()) {
            let old_height = item.max_content_height;

            // Measure height at the actual column width from pass 1
            let height = self.measure_block_at_inline(item.node_id, placement.width);

            // Update both min and max height with the same value
            // (we're measuring at a definite width, so there's only one result)
            item.min_content_height = height;
            item.max_content_height = height;

            tracing::info!(
                "Grid item {:?} (row sizing): width={:.1}px, old_height={:.1}px, NEW_HEIGHT={:.1}px, delta={:.1}px",
                item.node_id,
                placement.width,
                old_height,
                height,
                height - old_height
            );
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

    /// Layout a grid container.
    pub(super) fn layout_grid_container(
        &mut self,
        node: NodeKey,
        constraint_space: &ConstraintSpace,
        style: &ComputedStyle,
        sides: &BoxSides,
    ) -> LayoutResult {
        tracing::debug!("layout_grid_container for node {:?}", node);

        // Compute container size
        let container_inline_size = self.compute_inline_size(node, constraint_space, style, sides);

        // Resolve BFC offset
        let bfc_offset = if constraint_space.bfc_offset.is_resolved() {
            constraint_space.bfc_offset
        } else {
            BfcOffset::new(
                constraint_space.bfc_offset.inline_offset,
                Some(LayoutUnit::zero()),
            )
        };

        // Determine if this is measurement or final layout using the explicit flag
        // This avoids ambiguity between root layout and nested measurement
        let is_measurement = constraint_space.is_for_measurement_only;

        // Parse grid properties
        let (col_tracks, row_tracks) = Self::parse_grid_tracks(style);

        // Run grid layout
        // Use explicit container height if specified, otherwise use available space
        let container_block_size = style.height.unwrap_or_else(|| {
            constraint_space
                .available_block_size
                .resolve(LayoutUnit::from_px(10000.0))
                .to_px()
        });

        // Convert GridAutoFlow from style model to grid module type
        let grid_auto_flow = match style.grid_auto_flow {
            StyleGridAutoFlow::Row => GridAutoFlow::Row,
            StyleGridAutoFlow::Column => GridAutoFlow::Column,
            StyleGridAutoFlow::RowDense => GridAutoFlow::RowDense,
            StyleGridAutoFlow::ColumnDense => GridAutoFlow::ColumnDense,
        };

        let _padding_inline_sum = sides.padding_left.to_px() + sides.padding_right.to_px();
        let _border_inline_sum = sides.border_left.to_px() + sides.border_right.to_px();
        let padding_block_sum = sides.padding_top.to_px() + sides.padding_bottom.to_px();
        let border_block_sum = sides.border_top.to_px() + sides.border_bottom.to_px();

        // Run spec-compliant grid layout (column sizing, then row sizing)
        let grid_inputs = GridContainerInputs {
            row_tracks,
            col_tracks,
            auto_flow: grid_auto_flow,
            available_width: container_inline_size,
            available_height: container_block_size,
            align_items: GridAlignment::default(),
            justify_items: GridAlignment::default(),
            has_explicit_height: style.height.is_some(),
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
        let final_inline_size = container_inline_size;
        let final_block_size = content_block + padding_block_sum + border_block_sum;

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
