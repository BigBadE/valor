//! Size computation and constraint application for block layout.

use super::ConstraintLayoutTree;
use css_box::{BoxSides, compute_box_sides};
use css_orchestrator::style_model::{BoxSizing, ComputedStyle, Display, FlexDirection};
use js::NodeKey;

use super::super::constraint_space::{AvailableSize, ConstraintSpace};

impl ConstraintLayoutTree {
    pub(super) fn apply_width_constraints(
        border_box_width: f32,
        style: &ComputedStyle,
        _sides: &BoxSides,
    ) -> f32 {
        let mut result = border_box_width;

        // Apply min-width constraint (border-box)
        if let Some(min_width) = style.min_width {
            result = result.max(min_width);
        }

        // Apply max-width constraint (border-box)
        if let Some(max_width) = style.max_width {
            result = result.min(max_width);
        }

        result
    }

    /// Apply min/max height constraints to a border-box height.
    ///
    /// Per CSS Sizing spec, min/max constraints are applied AFTER height computation
    /// and ALWAYS in border-box space.
    pub(super) fn apply_height_constraints(
        border_box_height: f32,
        style: &ComputedStyle,
        _sides: &BoxSides,
    ) -> f32 {
        let mut result = border_box_height;

        // Apply min-height constraint (border-box)
        if let Some(min_height) = style.min_height {
            result = result.max(min_height);
        }

        // Apply max-height constraint (border-box)
        if let Some(max_height) = style.max_height {
            result = result.min(max_height);
        }

        result
    }

    /// Compute intrinsic width for form controls.
    ///
    /// Returns content-box width in pixels, or None if element doesn't have intrinsic width.
    pub(super) fn compute_form_control_intrinsic_width(&self, node: NodeKey) -> Option<f32> {
        let tag = self.tags.get(&node)?;
        let tag_lower = tag.to_lowercase();

        match tag_lower.as_str() {
            "input" => {
                // Checkboxes and radios have intrinsic 13x13 size
                if let Some(attrs) = self.attrs.get(&node)
                    && let Some(input_type) = attrs.get("type")
                {
                    let type_lower = input_type.to_lowercase();
                    if type_lower == "checkbox" || type_lower == "radio" {
                        return Some(13.0);
                    }
                }
                // Other inputs don't have intrinsic width (they respect CSS width)
                None
            }
            "button" => {
                // Buttons shrink-wrap their content, no intrinsic width
                // This allows them to size based on their text content
                None
            }
            _ => None,
        }
    }

    /// Compute intrinsic height for form controls.
    ///
    /// Returns content-box height in pixels, or None if element doesn't have intrinsic height.
    pub(super) fn compute_form_control_intrinsic_height(
        &self,
        node: NodeKey,
        style: &ComputedStyle,
    ) -> Option<f32> {
        let tag = self.tags.get(&node)?;
        let tag_lower = tag.to_lowercase();

        match tag_lower.as_str() {
            "input" => {
                if let Some(attrs) = self.attrs.get(&node)
                    && let Some(input_type) = attrs.get("type")
                {
                    let type_lower = input_type.to_lowercase();
                    if type_lower == "checkbox" || type_lower == "radio" {
                        // Checkboxes and radios have intrinsic 13x13 size
                        return Some(13.0);
                    }
                }
                // Text inputs have intrinsic height equal to one line of text.
                // Chrome uses the computed line-height for the content-box height.
                // For line-height: normal, typical fonts use approximately font-size * 1.2-1.4
                // System UI fonts typically use ~1.357 (e.g., 19px for 14px font)
                let font_size = style.font_size;
                let line_height = style
                    .line_height
                    .unwrap_or_else(|| (font_size * 1.357).round());
                Some(line_height)
            }
            "button" => {
                // Buttons don't have intrinsic height - they size based on their content
                None
            }
            "textarea" => {
                // Textareas don't have intrinsic height, they respect CSS height
                None
            }
            _ => None,
        }
    }

    /// Compute max-content width for a block by measuring its children.
    ///
    /// This is used for intrinsic sizing (e.g., flex-basis: auto with width: auto).
    /// Returns the content-box width in pixels.
    fn compute_max_content_width(&mut self, node: NodeKey) -> f32 {
        // Check if this is a flex container with row direction
        // Row flex containers sum children horizontally, not take max
        let node_style = self.styles.get(&node).cloned();
        let is_row_flex = node_style.as_ref().is_some_and(|style| {
            matches!(style.display, Display::Flex | Display::InlineFlex)
                && matches!(style.flex_direction, FlexDirection::Row)
        });

        // Get gap for flex containers
        let gap = node_style.as_ref().map_or(0.0, |style| style.column_gap);

        // Collect children to avoid borrow checker issues when we recursively call compute_max_content_width
        let children: Vec<NodeKey> = self.children.get(&node).map_or_else(Vec::new, Vec::clone);

        // For row flex: sum widths; for block: take max
        let mut max_width = 0.0f32;
        let mut sum_width = 0.0f32;
        let mut item_count = 0usize;

        for child in &children {
            // Skip whitespace-only text nodes in flex containers
            if is_row_flex
                && let Some(text) = self.text_nodes.get(child)
                && text.trim().is_empty()
            {
                continue;
            }

            let child_width = if self.is_text_node(*child) {
                // Measure text without wrapping (max-content means no line breaks)
                let text_measurement =
                    self.measure_text(*child, Some(node), AvailableSize::MaxContent);
                text_measurement.width
            } else {
                // For block children, recursively measure their intrinsic width
                let Some(child_style) = self.styles.get(child).cloned() else {
                    continue;
                };

                // Skip display:none children
                if matches!(child_style.display, Display::None) {
                    continue;
                }

                let child_sides = compute_box_sides(&child_style);

                // Determine the content-box width of the child
                let content_width = match child_style.width {
                    Some(explicit_width)
                        if matches!(child_style.box_sizing, BoxSizing::BorderBox) =>
                    {
                        // Border-box: subtract padding and border to get content-box
                        let padding_border = child_sides.padding_left.to_px()
                            + child_sides.padding_right.to_px()
                            + child_sides.border_left.to_px()
                            + child_sides.border_right.to_px();
                        (explicit_width - padding_border).max(0.0)
                    }
                    Some(explicit_width) => explicit_width, // Content-box
                    None => {
                        // Child has auto width - recursively measure its content
                        self.compute_max_content_width(*child)
                    }
                };

                // For max-content measurement, we need the width of the child's border-box
                // (content + padding + border). Margins are NOT included in max-content
                // as they collapse in normal flow.
                content_width
                    + child_sides.padding_left.to_px()
                    + child_sides.padding_right.to_px()
                    + child_sides.border_left.to_px()
                    + child_sides.border_right.to_px()
            };

            max_width = max_width.max(child_width);
            sum_width += child_width;
            item_count += 1;
        }

        if is_row_flex {
            // Row flex: sum of children + gaps between them
            let total_gap = if item_count > 1 {
                gap * (item_count - 1) as f32
            } else {
                0.0
            };
            sum_width + total_gap
        } else {
            // Block layout: max of children (stacked vertically)
            max_width
        }
    }

    /// Compute inline size for auto width.
    ///
    /// Returns the content-box width for elements with width: auto.
    fn compute_inline_size_auto(
        &mut self,
        node: NodeKey,
        constraint_space: &ConstraintSpace,
        _style: &ComputedStyle,
        sides: &BoxSides,
    ) -> f32 {
        // Check for form control intrinsic width first
        if let Some(intrinsic_width) = self.compute_form_control_intrinsic_width(node) {
            return intrinsic_width;
        }

        // Auto width: behavior depends on available size
        match constraint_space.available_inline_size {
            AvailableSize::MaxContent | AvailableSize::MinContent => {
                // Intrinsic sizing: measure content width
                // For flex basis calculation with auto, this gives us the content-based size
                self.compute_max_content_width(node)
            }
            AvailableSize::Definite(size) => {
                // Fill available space minus horizontal edges
                let horizontal_edges = (sides.margin_left
                    + sides.padding_left
                    + sides.border_left
                    + sides.border_right
                    + sides.padding_right
                    + sides.margin_right)
                    .to_px();
                let result = (size.to_px() - horizontal_edges).max(0.0);
                Self::debug_log_grid_item(size.to_px(), horizontal_edges, result, sides);
                result
            }
            AvailableSize::Indefinite => {
                // Use ICB width as available space
                let horizontal_edges = (sides.margin_left
                    + sides.padding_left
                    + sides.border_left
                    + sides.border_right
                    + sides.padding_right
                    + sides.margin_right)
                    .to_px();
                (self.icb_width.to_px() - horizontal_edges).max(0.0)
            }
        }
    }

    /// Compute inline size (width) for a block.
    ///
    /// Returns the content-box width (not including padding/border).
    pub(super) fn compute_inline_size(
        &mut self,
        node: NodeKey,
        constraint_space: &ConstraintSpace,
        style: &ComputedStyle,
        sides: &BoxSides,
    ) -> f32 {
        // If inline size is forced (e.g., flex item after grow/shrink), use the available size
        // directly as the border-box size
        if constraint_space.is_inline_size_forced
            && let AvailableSize::Definite(size) = constraint_space.available_inline_size
        {
            let padding_border =
                (sides.padding_left + sides.padding_right + sides.border_left + sides.border_right)
                    .to_px();
            return (size.to_px() - padding_border).max(0.0);
        }

        // Compute content-box width first
        // Priority: explicit width > percentage width > auto
        let content_width = if let Some(width) = style.width {
            // Explicit pixel width
            match style.box_sizing {
                BoxSizing::BorderBox => {
                    // width includes padding and border, subtract them to get content-box
                    let padding_border = (sides.padding_left
                        + sides.padding_right
                        + sides.border_left
                        + sides.border_right)
                        .to_px();
                    (width - padding_border).max(0.0)
                }
                BoxSizing::ContentBox => {
                    // width is already content-box
                    width
                }
            }
        } else if let Some(width_percent) = style.width_percent {
            // Percentage width - resolve against containing block (available inline size)
            let containing_block_width = match constraint_space.available_inline_size {
                AvailableSize::Definite(size) => size.to_px(),
                AvailableSize::MaxContent | AvailableSize::MinContent => {
                    // During intrinsic sizing, percentages resolve to auto
                    // Fall through to auto behavior
                    return self.compute_inline_size_auto(node, constraint_space, style, sides);
                }
                AvailableSize::Indefinite => self.icb_width.to_px(),
            };
            let resolved_width = containing_block_width * width_percent;
            match style.box_sizing {
                BoxSizing::BorderBox => {
                    let padding_border = (sides.padding_left
                        + sides.padding_right
                        + sides.border_left
                        + sides.border_right)
                        .to_px();
                    (resolved_width - padding_border).max(0.0)
                }
                BoxSizing::ContentBox => resolved_width,
            }
        } else {
            // Auto width
            self.compute_inline_size_auto(node, constraint_space, style, sides)
        };

        // Convert to border-box for constraint application
        let padding_border =
            (sides.padding_left + sides.padding_right + sides.border_left + sides.border_right)
                .to_px();
        let border_box_width = content_width + padding_border;

        // Apply min/max constraints in border-box space
        let constrained_border_box = Self::apply_width_constraints(border_box_width, style, sides);

        // Convert back to content-box
        (constrained_border_box - padding_border).max(0.0)
    }

    /// Debug logging for grid items with specific width.
    fn debug_log_grid_item(available: f32, horizontal_edges: f32, result: f32, sides: &BoxSides) {
        if (available - 272.0).abs() < 0.1 {
            use std::fs::OpenOptions;
            use std::io::Write as _;
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open("text_wrap_debug.log")
            {
                drop(writeln!(
                    file,
                    "compute_inline_size [272px GRID ITEM]: available={available:.6}, horizontal_edges={horizontal_edges:.6}, result={result:.6}"
                ));
                drop(writeln!(
                    file,
                    "  margin_left={:.6}, padding_left={:.6}, border_left={:.6}",
                    sides.margin_left.to_px(),
                    sides.padding_left.to_px(),
                    sides.border_left.to_px()
                ));
                drop(writeln!(
                    file,
                    "  border_right={:.6}, padding_right={:.6}, margin_right={:.6}",
                    sides.border_right.to_px(),
                    sides.padding_right.to_px(),
                    sides.margin_right.to_px()
                ));
            }
        }
    }
}
