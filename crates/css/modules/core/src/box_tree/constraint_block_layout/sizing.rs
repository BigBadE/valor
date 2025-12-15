//! Size computation and constraint application for block layout.

use super::ConstraintLayoutTree;
use css_box::BoxSides;
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

    /// Compute inline size (width) for a block.
    ///
    /// Returns the content-box width (not including padding/border).
    pub(super) fn compute_inline_size(
        &self,
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

                // Auto width: fill available space
                let available = match constraint_space.available_inline_size {
                    AvailableSize::Definite(size) => size.to_px(),
                    _ => self.icb_width.to_px(),
                };

                // For block boxes, width is available minus horizontal margins/padding/border
                let horizontal_edges = (sides.margin_left
                    + sides.padding_left
                    + sides.border_left
                    + sides.border_right
                    + sides.padding_right
                    + sides.margin_right)
                    .to_px();

                (available - horizontal_edges).max(0.0)
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
}
