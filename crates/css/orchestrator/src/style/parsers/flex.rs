//! Flexbox property parsers.

use std::collections::HashMap;

use crate::style_model;

use super::super::parse_px;

/// Parse flex scalars: grow, shrink, basis.
pub fn apply_flex_scalars(
    computed: &mut style_model::ComputedStyle,
    decls: &HashMap<String, String>,
) {
    // Check if flex shorthand is present
    let has_flex_shorthand = decls.contains_key("flex");

    // Parse flex shorthand first (it sets all three properties)
    if has_flex_shorthand && let Some(value) = decls.get("flex") {
        parse_flex_shorthand(computed, value);
    }

    // Individual properties override shorthand
    if let Some(value) = decls.get("flex-grow")
        && let Ok(number) = value.trim().parse::<f32>()
    {
        computed.flex_grow = number;
    }
    if let Some(value) = decls.get("flex-shrink") {
        if let Ok(number) = value.trim().parse::<f32>() {
            computed.flex_shrink = number;
        }
    } else if !has_flex_shorthand {
        // Only apply default flex-shrink if no flex shorthand was used
        // Chromium default for flex-shrink is 1
        computed.flex_shrink = 1.0;
    }
    if let Some(value) = decls.get("flex-basis") {
        computed.flex_basis = parse_px(value);
    }
}

/// Parse the flex shorthand property.
///
/// CSS spec: <https://www.w3.org/TR/css-flexbox-1/#flex-property>
/// - `flex: none` → 0 0 auto
/// - `flex: auto` → 1 1 auto
/// - `flex: <number>` → <number> 1 0%
/// - `flex: <width>` → 1 1 <width>
fn parse_flex_shorthand(computed: &mut style_model::ComputedStyle, value: &str) {
    let value = value.trim();

    // Handle keyword values
    if value.eq_ignore_ascii_case("none") {
        computed.flex_grow = 0.0;
        computed.flex_shrink = 0.0;
        computed.flex_basis = None; // auto
        return;
    }

    if value.eq_ignore_ascii_case("auto") {
        computed.flex_grow = 1.0;
        computed.flex_shrink = 1.0;
        computed.flex_basis = None; // auto
        return;
    }

    // Try to parse as a single number (most common case: `flex: 1`)
    if let Ok(number) = value.parse::<f32>() {
        // `flex: <number>` → <number> 1 0%
        computed.flex_grow = number;
        computed.flex_shrink = 1.0;
        computed.flex_basis = Some(0.0); // 0%
        return;
    }

    // Handle multi-value syntax (e.g., "1 1 100px")
    let parts: Vec<&str> = value.split_whitespace().collect();

    if parts.len() == 1 {
        // Single non-numeric value is treated as flex-basis
        // `flex: <width>` → 1 1 <width>
        computed.flex_grow = 1.0;
        computed.flex_shrink = 1.0;
        computed.flex_basis = parse_px(value);
    } else if parts.len() == 2 {
        // Two values: grow shrink OR grow basis
        if let Ok(grow) = parts[0].parse::<f32>() {
            computed.flex_grow = grow;

            // Second value could be shrink (number) or basis (length)
            if let Ok(shrink) = parts[1].parse::<f32>() {
                computed.flex_shrink = shrink;
                computed.flex_basis = Some(0.0); // default to 0%
            } else {
                computed.flex_shrink = 1.0;
                computed.flex_basis = parse_px(parts[1]);
            }
        }
    } else if parts.len() == 3 {
        // Three values: grow shrink basis
        if let Ok(grow) = parts[0].parse::<f32>() {
            computed.flex_grow = grow;
        }
        if let Ok(shrink) = parts[1].parse::<f32>() {
            computed.flex_shrink = shrink;
        }
        computed.flex_basis = parse_px(parts[2]);
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
        computed.align_items = if value.eq_ignore_ascii_case("normal") {
            style_model::AlignItems::Normal
        } else if value.eq_ignore_ascii_case("stretch") {
            style_model::AlignItems::Stretch
        } else if value.eq_ignore_ascii_case("flex-start") {
            style_model::AlignItems::FlexStart
        } else if value.eq_ignore_ascii_case("center") {
            style_model::AlignItems::Center
        } else if value.eq_ignore_ascii_case("flex-end") {
            style_model::AlignItems::FlexEnd
        } else {
            style_model::AlignItems::Normal
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
