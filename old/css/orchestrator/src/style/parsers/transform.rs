//! CSS transform property parser.

use std::collections::HashMap;

use crate::style_model::Transform;

/// Parse the transform property.
///
/// Supports translateX() and translateY() functions for MVP.
/// Example: "translateX(20px)" or "translateX(20px) translateY(10px)"
pub fn parse_transform(value: &str) -> Option<Transform> {
    let mut transform = Transform::default();

    // Simple parser for translateX and translateY
    let value = value.trim();

    // Split by whitespace to get individual transform functions
    let parts: Vec<&str> = value.split_whitespace().collect();

    for part in parts {
        if let Some(translate_x_value) = part.strip_prefix("translateX(") {
            if let Some(px_value) = translate_x_value.strip_suffix(')') {
                if let Some(px) = parse_px(px_value) {
                    transform.translate_x = px;
                }
            }
        } else if let Some(translate_y_value) = part.strip_prefix("translateY(") {
            if let Some(px_value) = translate_y_value.strip_suffix(')') {
                if let Some(px) = parse_px(px_value) {
                    transform.translate_y = px;
                }
            }
        }
    }

    Some(transform)
}

/// Parse a pixel value from a string (e.g., "20px" -> 20.0).
fn parse_px(value: &str) -> Option<f32> {
    let trimmed = value.trim();
    if let Some(px_str) = trimmed.strip_suffix("px") {
        px_str.trim().parse::<f32>().ok()
    } else {
        // Try parsing as a number without unit (treat as pixels)
        trimmed.parse::<f32>().ok()
    }
}

/// Apply transform property from declarations.
pub fn apply_transform(
    computed: &mut crate::style_model::ComputedStyle,
    decls: &HashMap<String, String>,
) {
    if let Some(value) = decls.get("transform") {
        log::info!("TRANSFORM DECL FOUND: {}", value);
        if let Some(transform) = parse_transform(value) {
            log::info!(
                "TRANSFORM PARSED: tx={}, ty={}",
                transform.translate_x,
                transform.translate_y
            );
            computed.transform = transform;
        } else {
            log::warn!("TRANSFORM PARSE FAILED: {}", value);
        }
    }
}
