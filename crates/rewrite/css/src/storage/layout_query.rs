//! Queries for computed layout dimensions.

use rewrite_core::{Database, DependencyContext, NodeId, Query, Relationship};

use super::{CssValueQuery, ViewportInput};

/// Query that computes the content box width of an element in subpixels.
/// This is used for containing block size calculations.
pub struct LayoutWidthQuery;

impl Query for LayoutWidthQuery {
    type Key = NodeId;
    type Value = i32; // subpixels

    fn execute(db: &Database, node: Self::Key, ctx: &mut DependencyContext) -> Self::Value {
        // Get the element's computed width
        let width_value = db.query::<CssValueQuery>((node, "width".to_string()), ctx);

        // If width is explicit (not auto), use it
        if !width_value.is_auto() {
            let width_subpixels = width_value.subpixels_or_zero();

            // Width is the content box, so we're done
            return width_subpixels;
        }

        // Width is auto - need to compute based on parent's width
        let parents = db.resolve_relationship(node, Relationship::Parent);
        if let Some(&parent) = parents.first() {
            // Recursively get parent's content box width
            let parent_width = db.query::<LayoutWidthQuery>(parent, ctx);

            // Subtract this element's horizontal margins, borders, and padding
            // Note: Auto margins don't affect content box width, they're computed separately
            let margin_left_val = db.query::<CssValueQuery>((node, "margin-left".to_string()), ctx);
            let margin_left = if margin_left_val.is_auto() {
                0
            } else {
                margin_left_val.subpixels_or_zero()
            };

            let margin_right_val =
                db.query::<CssValueQuery>((node, "margin-right".to_string()), ctx);
            let margin_right = if margin_right_val.is_auto() {
                0
            } else {
                margin_right_val.subpixels_or_zero()
            };

            let padding_left = db
                .query::<CssValueQuery>((node, "padding-left".to_string()), ctx)
                .subpixels_or_zero();
            let padding_right = db
                .query::<CssValueQuery>((node, "padding-right".to_string()), ctx)
                .subpixels_or_zero();
            let border_left = db
                .query::<CssValueQuery>((node, "border-left-width".to_string()), ctx)
                .subpixels_or_zero();
            let border_right = db
                .query::<CssValueQuery>((node, "border-right-width".to_string()), ctx)
                .subpixels_or_zero();

            let available_width = parent_width
                - margin_left
                - margin_right
                - padding_left
                - padding_right
                - border_left
                - border_right;

            return available_width.max(0);
        }

        // No parent - this is likely the root element or body
        // Use viewport width
        let viewport = db.get_input::<ViewportInput>(&()).unwrap_or_default();
        (viewport.width * 64.0) as i32
    }
}

/// Query that computes the content box height of an element in subpixels.
pub struct LayoutHeightQuery;

impl Query for LayoutHeightQuery {
    type Key = NodeId;
    type Value = i32; // subpixels

    fn execute(db: &Database, node: Self::Key, ctx: &mut DependencyContext) -> Self::Value {
        // Get the element's computed height
        let height_value = db.query::<CssValueQuery>((node, "height".to_string()), ctx);

        // If height is explicit (not auto), use it
        if !height_value.is_auto() {
            return height_value.subpixels_or_zero();
        }

        // Height is auto - for now, return 0 (proper implementation would compute based on content)
        // This is more complex as it depends on children and content
        0
    }
}

/// Query that computes the resolved (computed) margin for an element.
/// This handles auto margin computation for centering.
pub struct ResolvedMarginQuery;

impl Query for ResolvedMarginQuery {
    type Key = (NodeId, String); // (node, "margin-left" or "margin-right" etc.)
    type Value = i32; // subpixels

    fn execute(db: &Database, key: Self::Key, ctx: &mut DependencyContext) -> Self::Value {
        let (node, property) = key;

        // Get the CSS value for this margin
        let margin_value = db.query::<CssValueQuery>((node, property.clone()), ctx);

        // If it's not auto, just return the value
        if !margin_value.is_auto() {
            return margin_value.subpixels_or_zero();
        }

        // It's auto - compute based on containing block and element width
        // Only horizontal auto margins have special behavior (centering)
        if property != "margin-left" && property != "margin-right" {
            // Vertical auto margins are 0
            return 0;
        }

        // Get containing block width
        let parents = db.resolve_relationship(node, Relationship::Parent);
        let Some(&parent) = parents.first() else {
            return 0;
        };

        let parent_width = db.query::<LayoutWidthQuery>(parent, ctx);

        // Get element's width (content box)
        let element_width = db.query::<LayoutWidthQuery>(node, ctx);

        // Get the other margin, padding, and border
        let other_margin_prop = if property == "margin-left" {
            "margin-right"
        } else {
            "margin-left"
        };

        let other_margin_value =
            db.query::<CssValueQuery>((node, other_margin_prop.to_string()), ctx);
        let other_margin = if other_margin_value.is_auto() {
            0
        } else {
            other_margin_value.subpixels_or_zero()
        };

        let padding_left = db
            .query::<CssValueQuery>((node, "padding-left".to_string()), ctx)
            .subpixels_or_zero();
        let padding_right = db
            .query::<CssValueQuery>((node, "padding-right".to_string()), ctx)
            .subpixels_or_zero();
        let border_left = db
            .query::<CssValueQuery>((node, "border-left-width".to_string()), ctx)
            .subpixels_or_zero();
        let border_right = db
            .query::<CssValueQuery>((node, "border-right-width".to_string()), ctx)
            .subpixels_or_zero();

        // Available space for margins
        let used_space = element_width
            + padding_left
            + padding_right
            + border_left
            + border_right
            + other_margin;
        let available_space = parent_width - used_space;

        // If both margins are auto, split equally
        if other_margin_value.is_auto() {
            return (available_space / 2).max(0);
        }

        // Only this margin is auto, it gets all available space
        available_space.max(0)
    }
}
