//! Flexbox basis and item computation.

use super::ConstraintLayoutTree;
use super::shared::{ChildStyleInfo, FlexLayoutResult};
use css_box::{BoxSides, LayoutUnit, compute_box_sides};
use css_flexbox::{FlexChild, ItemRef};
use css_orchestrator::style_model::{BoxSizing, ComputedStyle, Display, Position};
use js::NodeKey;

use super::super::constraint_space::{AvailableSize, BfcOffset, ConstraintSpace, LayoutResult};
use super::super::exclusion_space::ExclusionSpace;
use super::super::margin_strut::MarginStrut;

/// Convert explicit width to content-box, accounting for box-sizing.
fn width_to_content_box(width: f32, box_sizing: BoxSizing, padding_border: f32) -> f32 {
    match box_sizing {
        BoxSizing::BorderBox => (width - padding_border).max(0.0),
        BoxSizing::ContentBox => width,
    }
}

/// Compute padding+border for horizontal edges.
fn horizontal_padding_border(sides: &BoxSides) -> f32 {
    (sides.padding_left + sides.padding_right + sides.border_left + sides.border_right).to_px()
}

/// Compute available inline size for flex item measurement.
fn compute_available_inline_for_row(
    child_style: &ComputedStyle,
    container_inline_size: f32,
) -> AvailableSize {
    if let Some(width) = child_style.width {
        return AvailableSize::Definite(LayoutUnit::from_px(width));
    }
    if let Some(pct) = child_style.width_percent {
        return AvailableSize::Definite(LayoutUnit::from_px(container_inline_size * pct));
    }
    if let Some(basis) = child_style.flex_basis {
        return AvailableSize::Definite(LayoutUnit::from_px(basis));
    }
    if let Some(basis_pct) = child_style.flex_basis_percent {
        return AvailableSize::Definite(LayoutUnit::from_px(container_inline_size * basis_pct));
    }
    AvailableSize::MaxContent
}

/// Compute available inline size for column flex items.
fn compute_available_inline_for_column(
    child_style: &ComputedStyle,
    container_inline_size: f32,
) -> AvailableSize {
    if let Some(width) = child_style.width {
        return AvailableSize::Definite(LayoutUnit::from_px(width));
    }
    if let Some(pct) = child_style.width_percent {
        return AvailableSize::Definite(LayoutUnit::from_px(container_inline_size * pct));
    }
    AvailableSize::Definite(LayoutUnit::from_px(container_inline_size))
}

impl ConstraintLayoutTree {
    pub(super) fn compute_flex_basis(
        child_style: &ComputedStyle,
        child_sides: &BoxSides,
        child_result: &LayoutResult,
        is_row: bool,
        container_main_size: f32,
    ) -> f32 {
        // Calculate padding+border for the main axis
        let main_padding_border = if is_row {
            (child_sides.padding_left
                + child_sides.padding_right
                + child_sides.border_left
                + child_sides.border_right)
                .to_px()
        } else {
            (child_sides.padding_top
                + child_sides.padding_bottom
                + child_sides.border_top
                + child_sides.border_bottom)
                .to_px()
        };

        // Handle explicit flex-basis values (pixel or percentage)
        // Per CSS spec, flex-basis respects box-sizing
        if let Some(basis) = child_style.flex_basis {
            return match child_style.box_sizing {
                BoxSizing::BorderBox => (basis - main_padding_border).max(0.0),
                BoxSizing::ContentBox => basis,
            };
        }

        if let Some(basis_percent) = child_style.flex_basis_percent {
            // Resolve percentage flex-basis against container's main size
            let resolved_basis = container_main_size * basis_percent;
            return match child_style.box_sizing {
                BoxSizing::BorderBox => (resolved_basis - main_padding_border).max(0.0),
                BoxSizing::ContentBox => resolved_basis,
            };
        }

        if is_row {
            // Row: flex basis is width (if specified), else intrinsic size
            Self::compute_flex_basis_from_width(
                child_style,
                child_sides,
                child_result,
                container_main_size,
            )
        } else {
            // Column: flex basis is height (if specified), else intrinsic size
            Self::compute_flex_basis_from_height(child_style, child_sides, child_result)
        }
    }

    /// Compute flex basis from width. Returns content-box flex basis.
    pub(super) fn compute_flex_basis_from_width(
        child_style: &ComputedStyle,
        child_sides: &BoxSides,
        child_result: &LayoutResult,
        container_inline_size: f32,
    ) -> f32 {
        let padding_border = horizontal_padding_border(child_sides);

        // Priority: explicit width > percentage width > auto (intrinsic)
        // Use match to avoid option_if_let_else lint
        match (child_style.width, child_style.width_percent) {
            (Some(width), _) => {
                // Explicit pixel width
                width_to_content_box(width, child_style.box_sizing, padding_border)
            }
            (None, Some(width_percent)) => {
                // Percentage width - resolve against container's inline size
                let resolved_width = container_inline_size * width_percent;
                width_to_content_box(resolved_width, child_style.box_sizing, padding_border)
            }
            (None, None) => {
                // No explicit width: use intrinsic size from measurement.
                // child_result.inline_size is border-box, subtract padding+border
                (child_result.inline_size - padding_border).max(0.0)
            }
        }
    }

    /// Compute flex basis from height (helper).
    ///
    /// Returns the **content-box** flex basis. The flex algorithm will add
    /// `main_padding_border` to get the outer (border-box) size.
    pub(super) fn compute_flex_basis_from_height(
        child_style: &ComputedStyle,
        child_sides: &BoxSides,
        child_result: &LayoutResult,
    ) -> f32 {
        child_style.height.map_or_else(
            || {
                // No explicit height: block_size is border-box, so we need to
                // subtract padding+border to get content-box for the flex algorithm.
                let padding_border = (child_sides.padding_top
                    + child_sides.padding_bottom
                    + child_sides.border_top
                    + child_sides.border_bottom)
                    .to_px();
                (child_result.block_size - padding_border).max(0.0)
            },
            |height| {
                // Explicit height: need to account for box-sizing
                match child_style.box_sizing {
                    BoxSizing::BorderBox => {
                        let padding_border = (child_sides.padding_top
                            + child_sides.padding_bottom
                            + child_sides.border_top
                            + child_sides.border_bottom)
                            .to_px();
                        (height - padding_border).max(0.0)
                    }
                    BoxSizing::ContentBox => height,
                }
            },
        )
    }

    /// Compute flex container's content-box cross size.
    ///
    /// Returns the definite cross size if set, or 0 to indicate indefinite.
    /// This affects whether items stretch to fill the container (stretch only
    /// applies when the container has a definite cross size).
    pub(super) fn compute_flex_container_cross_size(
        style: &ComputedStyle,
        sides: &BoxSides,
        constraint_space: &ConstraintSpace,
    ) -> f32 {
        style.height.map_or_else(
            || {
                // No explicit height - use available block size from constraint space
                match constraint_space.available_block_size {
                    AvailableSize::Definite(size) => {
                        // Subtract padding and border to get content-box size
                        let padding_border = (sides.padding_top
                            + sides.padding_bottom
                            + sides.border_top
                            + sides.border_bottom)
                            .to_px();
                        (size.to_px() - padding_border).max(0.0)
                    }
                    AvailableSize::Indefinite
                    | AvailableSize::MinContent
                    | AvailableSize::MaxContent => {
                        // No available size - container will size to content
                        // Return 0 for now, will be adjusted after measuring children
                        0.0
                    }
                }
            },
            |height| match style.box_sizing {
                BoxSizing::BorderBox => {
                    let padding_border = (sides.padding_top
                        + sides.padding_bottom
                        + sides.border_top
                        + sides.border_bottom)
                        .to_px();
                    (height - padding_border).max(0.0)
                }
                BoxSizing::ContentBox => height,
            },
        )
    }

    /// Process text node as flex item.
    pub(super) fn process_text_flex_item(
        &self,
        child: NodeKey,
        parent_node: NodeKey,
        child_space: &ConstraintSpace,
        _is_row: bool,
    ) -> (FlexChild, ChildStyleInfo) {
        // Text nodes in flexbox create anonymous flex items
        // Measure the text to get its intrinsic size
        let text_measurement =
            self.measure_text(child, Some(parent_node), child_space.available_inline_size);

        // Create a layout result for the text node with measured dimensions
        // Use single_line_height (CSS line-height) for flex item cross-sizing to match Chrome.
        // For wrapped text, use text_rect_height (total height across all lines).
        let is_wrapped = text_measurement.text_rect_height > text_measurement.single_line_height;
        let block_size = if is_wrapped {
            text_measurement.text_rect_height
        } else {
            text_measurement.single_line_height
        };
        let child_result = LayoutResult {
            inline_size: text_measurement.width,
            block_size,
            bfc_offset: BfcOffset::root(),
            exclusion_space: ExclusionSpace::new(),
            end_margin_strut: MarginStrut::default(),
            baseline: None,
            needs_relayout: false,
        };

        // Create a dummy style for text nodes (they don't have their own style)
        let child_style = ComputedStyle::default();

        let flex_child = FlexChild {
            handle: ItemRef(child.0),
            flex_basis: 0.0,  // Will be filled later
            flex_grow: 0.0,   // Text doesn't grow
            flex_shrink: 1.0, // Text can shrink
            min_main: 0.0,
            max_main: 1e9,
            margin_left: 0.0,
            margin_right: 0.0,
            margin_top: 0.0,
            margin_bottom: 0.0,
            margin_left_auto: false,
            margin_right_auto: false,
            main_padding_border: 0.0, // Text nodes have no padding/border
        };

        (flex_child, (child, child_style, child_result))
    }

    /// Process element node as flex item. Maps CSS margins to flex main/cross axes.
    pub(super) fn process_element_flex_item(
        &mut self,
        child: NodeKey,
        child_space: &ConstraintSpace,
        is_row: bool,
    ) -> (FlexChild, ChildStyleInfo) {
        let child_style = self.style(child);
        let child_result = self.layout_block(child, child_space);
        let child_sides = compute_box_sides(&child_style);

        // For row flex: main axis is horizontal, so use CSS left/right for main margins
        // For column flex: main axis is vertical, so use CSS top/bottom for main margins
        let (main_margin_start, main_margin_end, cross_margin_start, cross_margin_end) = if is_row {
            (
                child_sides.margin_left.to_px(),
                child_sides.margin_right.to_px(),
                child_sides.margin_top.to_px(),
                child_sides.margin_bottom.to_px(),
            )
        } else {
            (
                child_sides.margin_top.to_px(),
                child_sides.margin_bottom.to_px(),
                child_sides.margin_left.to_px(),
                child_sides.margin_right.to_px(),
            )
        };

        // Extract min/max constraints from CSS
        let (min_main, max_main) = if is_row {
            // Row flex: min/max-width apply to main axis
            let min = child_style.min_width.unwrap_or(0.0);
            let max = child_style.max_width.unwrap_or(1e9);
            (min, max)
        } else {
            // Column flex: min/max-height apply to main axis
            let min = child_style.min_height.unwrap_or(0.0);
            let max = child_style.max_height.unwrap_or(1e9);
            (min, max)
        };

        let flex_child = FlexChild {
            handle: ItemRef(child.0),
            flex_basis: 0.0, // Will be filled later
            flex_grow: child_style.flex_grow,
            flex_shrink: child_style.flex_shrink,
            min_main,
            max_main,
            margin_left: main_margin_start,
            margin_right: main_margin_end,
            margin_top: cross_margin_start,
            margin_bottom: cross_margin_end,
            margin_left_auto: false,
            margin_right_auto: false,
            main_padding_border: 0.0, // Will be filled later based on flex direction
        };

        (flex_child, (child, child_style, child_result))
    }

    /// Create constraint space for measuring a flex item.
    ///
    /// For items with explicit width (in row flex) or height (in column flex),
    /// use that value so text wrapping is computed correctly. For auto-sized items,
    /// use `MaxContent` to measure intrinsic size.
    fn create_flex_item_measurement_space(
        child_style: &ComputedStyle,
        container_inline_size: f32,
        is_row: bool,
    ) -> ConstraintSpace {
        let child_sides = compute_box_sides(child_style);

        // For row flex: check if item has explicit width or flex-basis
        // For column flex: check if item has explicit height (for main axis)
        let available_inline = if is_row {
            compute_available_inline_for_row(child_style, container_inline_size)
        } else {
            compute_available_inline_for_column(child_style, container_inline_size)
        };

        // Determine if inline size is forced
        let is_inline_size_forced = is_row
            && (child_style.width.is_some()
                || child_style.width_percent.is_some()
                || child_style.flex_basis.is_some()
                || child_style.flex_basis_percent.is_some());

        // Account for box-sizing when forcing size
        let adjusted_inline = if is_inline_size_forced {
            if let AvailableSize::Definite(size) = available_inline {
                match child_style.box_sizing {
                    BoxSizing::BorderBox => {
                        // Size includes padding+border, pass as-is
                        AvailableSize::Definite(size)
                    }
                    BoxSizing::ContentBox => {
                        // Size is content-box, add padding+border for available space
                        let extra = child_sides.padding_left
                            + child_sides.padding_right
                            + child_sides.border_left
                            + child_sides.border_right;
                        AvailableSize::Definite(size + extra)
                    }
                }
            } else {
                available_inline
            }
        } else {
            available_inline
        };

        ConstraintSpace {
            available_inline_size: adjusted_inline,
            available_block_size: AvailableSize::Indefinite,
            bfc_offset: BfcOffset::root(),
            exclusion_space: ExclusionSpace::new(),
            margin_strut: MarginStrut::default(),
            is_new_formatting_context: true,
            percentage_resolution_block_size: None,
            fragmentainer_block_size: None,
            fragmentainer_offset: LayoutUnit::zero(),
            is_for_measurement_only: true,
            margins_already_applied: false,
            is_block_size_forced: false,
            is_inline_size_forced,
        }
    }

    /// Build flex items from children, filtering out abspos and display:none.
    pub(super) fn build_flex_items(
        &mut self,
        node: NodeKey,
        children: &[NodeKey],
        container_inline_size: f32,
        is_row: bool,
    ) -> FlexLayoutResult {
        let mut flex_items = Vec::new();
        let mut child_styles = Vec::new();
        let mut abspos_children = Vec::new();

        // Default space for text nodes (they don't have explicit sizes)
        let text_child_space = ConstraintSpace {
            available_inline_size: AvailableSize::MaxContent,
            available_block_size: AvailableSize::Indefinite,
            bfc_offset: BfcOffset::root(),
            exclusion_space: ExclusionSpace::new(),
            margin_strut: MarginStrut::default(),
            is_new_formatting_context: true,
            percentage_resolution_block_size: None,
            fragmentainer_block_size: None,
            fragmentainer_offset: LayoutUnit::zero(),
            is_for_measurement_only: true,
            margins_already_applied: false,
            is_block_size_forced: false,
            is_inline_size_forced: false,
        };

        for child in children {
            // Skip whitespace-only text nodes (CSS Flexbox spec §4.1)
            if let Some(text) = self.text_nodes.get(child)
                && text.trim().is_empty()
            {
                continue;
            }

            // Handle text nodes specially - they need to be measured, not laid out as blocks
            if self.is_text_node(*child) {
                let (flex_child, child_info) =
                    self.process_text_flex_item(*child, node, &text_child_space, is_row);
                flex_items.push(flex_child);
                child_styles.push(child_info);
                continue;
            }

            let child_style = self.style(*child);

            if matches!(child_style.display, Display::None) {
                continue;
            }

            if matches!(child_style.position, Position::Absolute | Position::Fixed) {
                abspos_children.push(*child);
                continue;
            }

            // Create per-child constraint space based on whether they have explicit sizes
            let child_space = Self::create_flex_item_measurement_space(
                &child_style,
                container_inline_size,
                is_row,
            );

            let (flex_child, child_info) =
                self.process_element_flex_item(*child, &child_space, is_row);

            flex_items.push(flex_child);
            child_styles.push(child_info);
        }

        (flex_items, child_styles, abspos_children)
    }

    /// Update flex item basis values based on child styles and results.
    pub(super) fn update_flex_item_basis(
        flex_items: &mut [FlexChild],
        child_styles: &[ChildStyleInfo],
        is_row: bool,
        container_main_size: f32,
    ) {
        for (idx, (_, child_style, child_result)) in child_styles.iter().enumerate() {
            if let Some(item) = flex_items.get_mut(idx) {
                let child_sides = compute_box_sides(child_style);
                item.flex_basis = Self::compute_flex_basis(
                    child_style,
                    &child_sides,
                    child_result,
                    is_row,
                    container_main_size,
                );

                // Set main-axis padding + border for outer size calculation
                // This is needed for correct free space calculation in flex shrink/grow
                item.main_padding_border = if is_row {
                    // Row flex: main axis is horizontal
                    (child_sides.padding_left
                        + child_sides.padding_right
                        + child_sides.border_left
                        + child_sides.border_right)
                        .to_px()
                } else {
                    // Column flex: main axis is vertical
                    (child_sides.padding_top
                        + child_sides.padding_bottom
                        + child_sides.border_top
                        + child_sides.border_bottom)
                        .to_px()
                };
            }
        }
    }

    /// Check if child has explicit inline offset based on direction.
    pub(super) fn has_explicit_inline_offset(child_style: &ComputedStyle, is_row: bool) -> bool {
        if is_row {
            child_style.left.is_some() || child_style.left_percent.is_some()
        } else {
            child_style.top.is_some() || child_style.top_percent.is_some()
        }
    }

    /// Check if child has explicit block offset based on direction.
    pub(super) fn has_explicit_block_offset(child_style: &ComputedStyle, is_row: bool) -> bool {
        if is_row {
            child_style.top.is_some() || child_style.top_percent.is_some()
        } else {
            child_style.left.is_some() || child_style.left_percent.is_some()
        }
    }
}
