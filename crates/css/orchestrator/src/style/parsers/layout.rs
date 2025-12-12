//! Layout property parsers (display, position, float, overflow, offsets).

use std::collections::HashMap;

use crate::style_model;

use super::super::{parse_int, parse_px};

/// Parse layout-related keywords (display, position, z-index, overflow).
pub fn apply_layout_keywords(
    computed: &mut style_model::ComputedStyle,
    decls: &HashMap<String, String>,
) {
    if let Some(value) = decls.get("display") {
        computed.display = if value.eq_ignore_ascii_case("block") {
            style_model::Display::Block
        } else if value.eq_ignore_ascii_case("flex") {
            style_model::Display::Flex
        } else if value.eq_ignore_ascii_case("inline-flex") {
            style_model::Display::InlineFlex
        } else if value.eq_ignore_ascii_case("grid") {
            style_model::Display::Grid
        } else if value.eq_ignore_ascii_case("inline-grid") {
            style_model::Display::InlineGrid
        } else if value.eq_ignore_ascii_case("none") {
            style_model::Display::None
        } else if value.eq_ignore_ascii_case("contents") {
            style_model::Display::Contents
        } else if value.eq_ignore_ascii_case("inline-block") {
            style_model::Display::InlineBlock
        } else {
            // Default to inline for unknown values or "inline"
            style_model::Display::Inline
        };
    }
    if let Some(value) = decls.get("position") {
        computed.position = if value.eq_ignore_ascii_case("relative") {
            style_model::Position::Relative
        } else if value.eq_ignore_ascii_case("absolute") {
            style_model::Position::Absolute
        } else if value.eq_ignore_ascii_case("fixed") {
            style_model::Position::Fixed
        } else {
            style_model::Position::Static
        };
    }
    if let Some(value) = decls.get("z-index") {
        computed.z_index = parse_int(value);
    }
    if let Some(value) = decls.get("overflow") {
        computed.overflow = if value.eq_ignore_ascii_case("hidden") {
            style_model::Overflow::Hidden
        } else if value.eq_ignore_ascii_case("auto") {
            style_model::Overflow::Auto
        } else if value.eq_ignore_ascii_case("scroll") {
            style_model::Overflow::Scroll
        } else if value.eq_ignore_ascii_case("clip") {
            style_model::Overflow::Clip
        } else {
            style_model::Overflow::Visible
        };
    }
    // CSS 2.2 §9.5 Floats — parse 'float' and 'clear'
    apply_float_and_clear(computed, decls);
}

/// Parse CSS 2.2 `float` and `clear` longhands from declarations into the computed style.
fn apply_float_and_clear(
    computed: &mut style_model::ComputedStyle,
    decls: &HashMap<String, String>,
) {
    if let Some(value) = decls.get("float") {
        computed.float = if value.eq_ignore_ascii_case("left") {
            style_model::Float::Left
        } else if value.eq_ignore_ascii_case("right") {
            style_model::Float::Right
        } else {
            style_model::Float::None
        };
    }
    if let Some(value) = decls.get("clear") {
        computed.clear = if value.eq_ignore_ascii_case("left") {
            style_model::Clear::Left
        } else if value.eq_ignore_ascii_case("right") {
            style_model::Clear::Right
        } else if value.eq_ignore_ascii_case("both") {
            style_model::Clear::Both
        } else {
            style_model::Clear::None
        };
    }
}

/// Parse positional offsets (top/left/right/bottom) as pixels or percentages.
pub fn apply_offsets(computed: &mut style_model::ComputedStyle, decls: &HashMap<String, String>) {
    // Helper for one side: prefer percentage if provided, otherwise pixels.
    #[inline]
    fn parse_offset(raw_opt: Option<&String>) -> (Option<f32>, Option<f32>) {
        if let Some(raw) = raw_opt {
            let trimmed = raw.trim();
            if let Some(percent_str) = trimmed.strip_suffix('%')
                && let Ok(percent_value) = percent_str.trim().parse::<f32>()
            {
                return (None, Some((percent_value / 100.0).max(0.0)));
            }
            if let Some(pixel_value) = parse_px(trimmed) {
                return (Some(pixel_value.max(0.0)), None);
            }
        }
        (None, None)
    }

    let (top_px, top_pct) = parse_offset(decls.get("top"));
    if top_pct.is_some() {
        computed.top_percent = top_pct;
        computed.top = None;
    } else if top_px.is_some() {
        computed.top = top_px;
        computed.top_percent = None;
    }

    let (left_px, left_pct) = parse_offset(decls.get("left"));
    if left_pct.is_some() {
        computed.left_percent = left_pct;
        computed.left = None;
    } else if left_px.is_some() {
        computed.left = left_px;
        computed.left_percent = None;
    }

    let (right_px, right_pct) = parse_offset(decls.get("right"));
    if right_pct.is_some() {
        computed.right_percent = right_pct;
        computed.right = None;
    } else if right_px.is_some() {
        computed.right = right_px;
        computed.right_percent = None;
    }

    let (bottom_px, bottom_pct) = parse_offset(decls.get("bottom"));
    if bottom_pct.is_some() {
        computed.bottom_percent = bottom_pct;
        computed.bottom = None;
    } else if bottom_px.is_some() {
        computed.bottom = bottom_px;
        computed.bottom_percent = None;
    }
}
