//! Font attribute preparation for text measurement.

use super::font_system::map_font_family;
use css_orchestrator::style_model::ComputedStyle;
use glyphon::{Attrs, Family, Weight};

/// Prepare font attributes from computed style.
pub fn prepare_font_attrs(style: &ComputedStyle) -> Attrs<'_> {
    let font_weight = if style.font_weight == 0 {
        400 // Default to normal
    } else {
        style.font_weight
    };
    let weight = Weight(font_weight);
    let mut attrs = Attrs::new().weight(weight);

    // Set font family if specified in style
    if let Some(ref family) = style.font_family {
        // Parse the font family string to handle multiple fallback fonts
        // Format: "'Courier New', Courier, monospace" or "Courier New"
        let family_clean = family.trim();
        if !family_clean.is_empty() {
            // Parse the font family list and try to use the first available font
            let mut font_set = false;
            for font_spec in family_clean.split(',') {
                let font_name = font_spec.trim().trim_matches('\'').trim_matches('"').trim();

                if !font_name.is_empty() {
                    // Map generic families to match Chrome's default font choices
                    let family_enum = map_font_family(font_name);

                    attrs = attrs.family(family_enum);
                    font_set = true;
                    // Use the first font in the list
                    break;
                }
            }

            if !font_set {
                // Fallback to generic serif family (will use fontdb settings)
                attrs = attrs.family(Family::Serif);
            }
        }
    } else {
        // Use the shared default font family function (single source of truth)
        attrs = attrs.family(super::font_system::get_default_font_family());
    }

    attrs
}
