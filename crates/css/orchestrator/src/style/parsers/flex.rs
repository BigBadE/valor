//! Flexbox property parsers.

use std::collections::HashMap;

use crate::style_model;

use super::super::parse_px;

/// Parse flex scalars: grow, shrink, basis.
pub fn apply_flex_scalars(
    computed: &mut style_model::ComputedStyle,
    decls: &HashMap<String, String>,
) {
    if let Some(value) = decls.get("flex-grow")
        && let Ok(number) = value.trim().parse::<f32>()
    {
        computed.flex_grow = number;
    }
    if let Some(value) = decls.get("flex-shrink") {
        if let Ok(number) = value.trim().parse::<f32>() {
            computed.flex_shrink = number;
        }
    } else {
        // Chromium default for flex-shrink is 1
        computed.flex_shrink = 1.0;
    }
    if let Some(value) = decls.get("flex-basis") {
        computed.flex_basis = parse_px(value);
    }
}

/// Parse flex alignment properties.
pub fn apply_flex_alignment(
    computed: &mut style_model::ComputedStyle,
    decls: &HashMap<String, String>,
) {
    parse_flex_direction_prop(computed, decls);
    parse_flex_wrap_prop(computed, decls);
    parse_align_items_prop(computed, decls);
    parse_justify_content_prop(computed, decls);
    parse_align_content_prop(computed, decls);
}

/// Parse the `flex-direction` property into the computed style.
fn parse_flex_direction_prop(
    computed: &mut style_model::ComputedStyle,
    decls: &HashMap<String, String>,
) {
    if let Some(value) = decls.get("flex-direction") {
        computed.flex_direction = if value.eq_ignore_ascii_case("column") {
            style_model::FlexDirection::Column
        } else {
            style_model::FlexDirection::Row
        };
    }
}

/// Parse the `flex-wrap` property into the computed style.
fn parse_flex_wrap_prop(
    computed: &mut style_model::ComputedStyle,
    decls: &HashMap<String, String>,
) {
    if let Some(value) = decls.get("flex-wrap") {
        computed.flex_wrap = if value.eq_ignore_ascii_case("wrap") {
            style_model::FlexWrap::Wrap
        } else {
            style_model::FlexWrap::NoWrap
        };
    }
}

/// Parse the `align-items` property into the computed style.
fn parse_align_items_prop(
    computed: &mut style_model::ComputedStyle,
    decls: &HashMap<String, String>,
) {
    if let Some(value) = decls.get("align-items") {
        computed.align_items = if value.eq_ignore_ascii_case("flex-start") {
            style_model::AlignItems::FlexStart
        } else if value.eq_ignore_ascii_case("center") {
            style_model::AlignItems::Center
        } else if value.eq_ignore_ascii_case("flex-end") {
            style_model::AlignItems::FlexEnd
        } else {
            style_model::AlignItems::Stretch
        };
    }
}

/// Parse the `justify-content` property into the computed style.
fn parse_justify_content_prop(
    computed: &mut style_model::ComputedStyle,
    decls: &HashMap<String, String>,
) {
    if let Some(value) = decls.get("justify-content") {
        computed.justify_content = if value.eq_ignore_ascii_case("center") {
            style_model::JustifyContent::Center
        } else if value.eq_ignore_ascii_case("flex-end") {
            style_model::JustifyContent::FlexEnd
        } else if value.eq_ignore_ascii_case("space-between") {
            style_model::JustifyContent::SpaceBetween
        } else if value.eq_ignore_ascii_case("space-around") {
            style_model::JustifyContent::SpaceAround
        } else if value.eq_ignore_ascii_case("space-evenly") {
            style_model::JustifyContent::SpaceEvenly
        } else {
            style_model::JustifyContent::FlexStart
        };
    }
}

/// Parse the `align-content` property into the computed style.
fn parse_align_content_prop(
    computed: &mut style_model::ComputedStyle,
    decls: &HashMap<String, String>,
) {
    if let Some(value) = decls.get("align-content") {
        computed.align_content = if value.eq_ignore_ascii_case("center") {
            style_model::AlignContent::Center
        } else if value.eq_ignore_ascii_case("flex-end") {
            style_model::AlignContent::FlexEnd
        } else if value.eq_ignore_ascii_case("space-between") {
            style_model::AlignContent::SpaceBetween
        } else if value.eq_ignore_ascii_case("space-around") {
            style_model::AlignContent::SpaceAround
        } else if value.eq_ignore_ascii_case("space-evenly") {
            style_model::AlignContent::SpaceEvenly
        } else if value.eq_ignore_ascii_case("stretch") {
            style_model::AlignContent::Stretch
        } else {
            style_model::AlignContent::FlexStart
        };
    }
}
