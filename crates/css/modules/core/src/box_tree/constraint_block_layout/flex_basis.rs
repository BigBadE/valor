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

impl ConstraintLayoutTree {
    pub(super) fn compute_flex_basis(
        child_style: &ComputedStyle,
        child_sides: &BoxSides,
        child_result: &LayoutResult,
        is_row: bool,
    ) -> f32 {
        if let Some(basis) = child_style.flex_basis {
            return basis;
        }

        if is_row {
            // Row: flex basis is width (if specified), else intrinsic size
            Self::compute_flex_basis_from_width(child_style, child_sides, child_result)
        } else {
            // Column: flex basis is height (if specified), else intrinsic size
            Self::compute_flex_basis_from_height(child_style, child_sides, child_result)
        }
    }

    /// Compute flex basis from width (helper).
    pub(super) fn compute_flex_basis_from_width(
        child_style: &ComputedStyle,
        child_sides: &BoxSides,
        child_result: &LayoutResult,
    ) -> f32 {
        child_style.width.map_or_else(
            || {
                // No explicit width: use intrinsic content-box size
                child_result.inline_size
                    - (child_sides.padding_left
                        + child_sides.padding_right
                        + child_sides.border_left
                        + child_sides.border_right)
                        .to_px()
            },
            |width| {
                // Explicit width: need to account for box-sizing
                match child_style.box_sizing {
                    BoxSizing::BorderBox => {
                        let padding_border = (child_sides.padding_left
                            + child_sides.padding_right
                            + child_sides.border_left
                            + child_sides.border_right)
                            .to_px();
                        (width - padding_border).max(0.0)
                    }
                    BoxSizing::ContentBox => width,
                }
            },
        )
    }

    /// Compute flex basis from height (helper).
    pub(super) fn compute_flex_basis_from_height(
        child_style: &ComputedStyle,
        child_sides: &BoxSides,
        child_result: &LayoutResult,
    ) -> f32 {
        child_style.height.map_or_else(
            || {
                // No explicit height: use intrinsic content-box size
                child_result.block_size
                    - (child_sides.padding_top
                        + child_sides.padding_bottom
                        + child_sides.border_top
                        + child_sides.border_bottom)
                        .to_px()
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
                    AvailableSize::Indefinite | AvailableSize::MinContent | AvailableSize::MaxContent => {
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
    ) -> (FlexChild, ChildStyleInfo) {
        // Text nodes in flexbox create anonymous flex items
        // Measure the text to get its intrinsic size
        let (text_width, _total_height, _glyph_height, _ascent, _single_line_height, text_rect_height) =
            self.measure_text(child, Some(parent_node), child_space.available_inline_size);

        // Create a layout result for the text node with measured dimensions
        // Use text_rect_height (glyph_height for single-line, glyph_height * lines for multi-line)
        let child_result = LayoutResult {
            inline_size: text_width,
            block_size: text_rect_height,
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
        };

        (flex_child, (child, child_style, child_result))
    }

    /// Process element node as flex item.
    pub(super) fn process_element_flex_item(
        &mut self,
        child: NodeKey,
        child_space: &ConstraintSpace,
    ) -> (FlexChild, ChildStyleInfo) {
        let child_style = self.style(child);
        let child_result = self.layout_block(child, child_space);
        let child_sides = compute_box_sides(&child_style);

        let flex_child = FlexChild {
            handle: ItemRef(child.0),
            flex_basis: 0.0, // Will be filled later
            flex_grow: child_style.flex_grow,
            flex_shrink: child_style.flex_shrink,
            min_main: 0.0,
            max_main: 1e9,
            margin_left: child_sides.margin_left.to_px(),
            margin_right: child_sides.margin_right.to_px(),
            margin_top: child_sides.margin_top.to_px(),
            margin_bottom: child_sides.margin_bottom.to_px(),
            margin_left_auto: false,
            margin_right_auto: false,
        };

        (flex_child, (child, child_style, child_result))
    }

    /// Build flex items from children, filtering out abspos and display:none.
    pub(super) fn build_flex_items(
        &mut self,
        node: NodeKey,
        children: &[NodeKey],
    ) -> FlexLayoutResult {
        let mut flex_items = Vec::new();
        let mut child_styles = Vec::new();
        let mut abspos_children = Vec::new();

        let child_space = ConstraintSpace {
            available_inline_size: AvailableSize::Indefinite,
            available_block_size: AvailableSize::Indefinite,
            bfc_offset: BfcOffset::root(),
            exclusion_space: ExclusionSpace::new(),
            margin_strut: MarginStrut::default(),
            is_new_formatting_context: true,
            percentage_resolution_block_size: None,
            fragmentainer_block_size: None,
            fragmentainer_offset: LayoutUnit::zero(),
            is_for_measurement_only: true, // Flexbox basis calculation is measurement
        };

        for child in children {
            // Skip whitespace-only text nodes (CSS Flexbox spec §4.1)
            // Text nodes in flex containers only generate anonymous flex items
            // if they contain non-whitespace characters
            if let Some(text) = self.text_nodes.get(child)
                && text.trim().is_empty()
            {
                continue;
            }

            // Handle text nodes specially - they need to be measured, not laid out as blocks
            if self.is_text_node(*child) {
                let (flex_child, child_info) =
                    self.process_text_flex_item(*child, node, &child_space);
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

            let (flex_child, child_info) = self.process_element_flex_item(*child, &child_space);
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
    ) {
        for (idx, (_, child_style, child_result)) in child_styles.iter().enumerate() {
            if let Some(item) = flex_items.get_mut(idx) {
                let child_sides = compute_box_sides(child_style);
                item.flex_basis =
                    Self::compute_flex_basis(child_style, &child_sides, child_result, is_row);
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
