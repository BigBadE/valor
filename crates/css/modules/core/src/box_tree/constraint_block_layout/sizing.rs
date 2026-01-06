//! Size computation and constraint application for block layout.

use super::ConstraintLayoutTree;
use css_box::{BoxSides, compute_box_sides};
use css_orchestrator::style_model::{BoxSizing, ComputedStyle};
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
        _style: &ComputedStyle,
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
                // Text inputs don't have intrinsic height - they size based on their content
                None
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
        // Collect children to avoid borrow checker issues when we recursively call compute_max_content_width
        let children: Vec<NodeKey> = self.children.get(&node).map_or_else(Vec::new, Vec::clone);
        let mut max_width = 0.0f32;

        for child in &children {
            if self.is_text_node(*child) {
                // Measure text without wrapping (max-content means no line breaks)
                let text_measurement =
                    self.measure_text(*child, Some(node), AvailableSize::MaxContent);
                max_width = max_width.max(text_measurement.width);
            } else {
                // For block children, recursively measure their intrinsic width
                let Some(child_style) = self.styles.get(child).cloned() else {
                    continue;
                };

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
                let child_border_box = content_width
                    + child_sides.padding_left.to_px()
                    + child_sides.padding_right.to_px()
                    + child_sides.border_left.to_px()
                    + child_sides.border_right.to_px();

                max_width = max_width.max(child_border_box);
            }
        }

        max_width
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
        // Compute content-box width first
        let content_width = style.width.map_or_else(
            || {
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
            },
            |width| {
                // Check box-sizing property
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
            },
        );

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
