//! Computed value resolution.

use crate::{CssKeyword, CssValue, LengthValue};
use rewrite_core::NodeId;

/// Convert a CSS value to subpixels (i32, 1/64th pixel).
pub fn css_value_to_subpixels(
    value: &CssValue,
    node: NodeId,
    db: &rewrite_core::Database,
    containing_block_size: Option<i32>,
) -> i32 {
    match value {
        CssValue::Length(length) => length_to_subpixels(length, node, db, containing_block_size),
        CssValue::Percentage(pct) => {
            if let Some(cb_size) = containing_block_size {
                (cb_size as f32 * pct) as i32
            } else {
                0
            }
        }
        CssValue::Number(num) => (*num * 64.0) as i32,
        CssValue::Integer(int) => int * 64,
        CssValue::Keyword(CssKeyword::Auto) => 0,
        CssValue::Keyword(_) => 0,
        _ => 0,
    }
}

/// Convert a length value to subpixels.
fn length_to_subpixels(
    length: &LengthValue,
    node: NodeId,
    db: &rewrite_core::Database,
    containing_block_size: Option<i32>,
) -> i32 {
    match length {
        LengthValue::Px(px) => (px * 64.0) as i32,
        LengthValue::Em(em) => {
            let font_size = get_font_size(node, db);
            (em * font_size * 64.0) as i32
        }
        LengthValue::Rem(rem) => {
            let root_font_size = 16.0;
            (rem * root_font_size * 64.0) as i32
        }
        LengthValue::Vw(vw) => {
            let viewport_width = 1920.0;
            (vw / 100.0 * viewport_width * 64.0) as i32
        }
        LengthValue::Vh(vh) => {
            let viewport_height = 1080.0;
            (vh / 100.0 * viewport_height * 64.0) as i32
        }
        LengthValue::Percent(pct) => {
            if let Some(cb_size) = containing_block_size {
                (cb_size as f32 * pct) as i32
            } else {
                0
            }
        }
        LengthValue::Vmin(vmin) => {
            let min = 1080.0f32.min(1920.0);
            (vmin / 100.0 * min * 64.0) as i32
        }
        LengthValue::Vmax(vmax) => {
            let max = 1080.0f32.max(1920.0);
            (vmax / 100.0 * max * 64.0) as i32
        }
        _ => 0,
    }
}

/// Get the font-size for a node in pixels.
fn get_font_size(node: NodeId, db: &rewrite_core::Database) -> f32 {
    use super::properties::FONT_SIZE;

    // Query font-size through cascade
    let mut ctx = rewrite_core::DependencyContext::new();
    let value =
        db.query::<super::cascade::CascadedPropertyQuery>((node, FONT_SIZE.to_string()), &mut ctx);
    match value {
        CssValue::Length(LengthValue::Px(px)) => px,
        _ => 16.0,
    }
}

/// Compute a dimensional value from Database inputs.
pub fn compute_dimensional_value(
    node: NodeId,
    property: &str,
    db: &rewrite_core::Database,
    containing_block_size: Option<i32>,
) -> i32 {
    let mut ctx = rewrite_core::DependencyContext::new();
    let value =
        db.query::<super::cascade::CascadedPropertyQuery>((node, property.to_string()), &mut ctx);

    css_value_to_subpixels(&value, node, db, containing_block_size)
}

/// Expand a shorthand property and set inputs in Database.
pub fn expand_shorthand(property: &str, value: &str, db: &rewrite_core::Database, node: NodeId) {
    use super::parser::{parse_edges, parse_gap};
    use super::properties::*;

    match property {
        "padding" => {
            if let Some([top, right, bottom, left]) = parse_edges(value) {
                db.set_input::<super::CssPropertyInput>((node, PADDING_TOP.to_string()), top);
                db.set_input::<super::CssPropertyInput>((node, PADDING_RIGHT.to_string()), right);
                db.set_input::<super::CssPropertyInput>((node, PADDING_BOTTOM.to_string()), bottom);
                db.set_input::<super::CssPropertyInput>((node, PADDING_LEFT.to_string()), left);
            }
        }
        "margin" => {
            if let Some([top, right, bottom, left]) = parse_edges(value) {
                db.set_input::<super::CssPropertyInput>((node, MARGIN_TOP.to_string()), top);
                db.set_input::<super::CssPropertyInput>((node, MARGIN_RIGHT.to_string()), right);
                db.set_input::<super::CssPropertyInput>((node, MARGIN_BOTTOM.to_string()), bottom);
                db.set_input::<super::CssPropertyInput>((node, MARGIN_LEFT.to_string()), left);
            }
        }
        "border-width" => {
            if let Some([top, right, bottom, left]) = parse_edges(value) {
                db.set_input::<super::CssPropertyInput>((node, BORDER_TOP_WIDTH.to_string()), top);
                db.set_input::<super::CssPropertyInput>(
                    (node, BORDER_RIGHT_WIDTH.to_string()),
                    right,
                );
                db.set_input::<super::CssPropertyInput>(
                    (node, BORDER_BOTTOM_WIDTH.to_string()),
                    bottom,
                );
                db.set_input::<super::CssPropertyInput>(
                    (node, BORDER_LEFT_WIDTH.to_string()),
                    left,
                );
            }
        }
        "gap" => {
            if let Some([row, column]) = parse_gap(value) {
                db.set_input::<super::CssPropertyInput>((node, ROW_GAP.to_string()), row);
                db.set_input::<super::CssPropertyInput>((node, COLUMN_GAP.to_string()), column);
            }
        }
        _ => {}
    }
}
