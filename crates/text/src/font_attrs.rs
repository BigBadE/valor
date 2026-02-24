//! Convert CSS properties to cosmic-text `Attrs` for font matching and shaping.

use cosmic_text::{Attrs, Family, Style, Weight};
use lightningcss::printer::PrinterOptions;
use lightningcss::properties::Property;
use lightningcss::properties::font::{
    AbsoluteFontWeight, FontFamily, FontStyle as CssFontStyle, FontWeight as CssFontWeight,
};
use lightningcss::traits::ToCss as _;

use crate::font_system::{default_font_family, map_font_family};

/// Default user-agent font size in pixels (CSS spec default).
pub const DEFAULT_FONT_SIZE_PX: f32 = 16.0;

/// Build cosmic-text `Attrs` from individual CSS property values.
///
/// Callers extract font properties from the Database and pass them here.
/// This keeps the text crate independent of the Database query mechanism.
pub fn build_attrs<'prop>(
    font_family: Option<&'prop Property<'static>>,
    font_weight: Option<&Property<'static>>,
    font_style: Option<&Property<'static>>,
) -> Attrs<'prop> {
    let mut attrs = Attrs::new();

    attrs = attrs.family(resolve_family(font_family));
    attrs = attrs.weight(resolve_weight(font_weight));
    attrs = attrs.style(resolve_style(font_style));

    attrs
}

/// Resolve CSS font-family to a cosmic-text `Family`.
///
/// Picks the first family in the CSS list. Falls back to the
/// platform default serif font.
fn resolve_family(prop: Option<&Property<'static>>) -> Family<'static> {
    let families: &[FontFamily<'_>] = match prop {
        Some(Property::FontFamily(list)) => list,
        _ => return default_font_family(),
    };

    if families.is_empty() {
        return default_font_family();
    }

    match &families[0] {
        FontFamily::Generic(generic) => {
            use lightningcss::properties::font::GenericFontFamily;
            match generic {
                GenericFontFamily::SansSerif | GenericFontFamily::SystemUI => {
                    map_font_family("sans-serif")
                }
                GenericFontFamily::Serif => map_font_family("serif"),
                GenericFontFamily::Monospace => map_font_family("monospace"),
                GenericFontFamily::Cursive => map_font_family("cursive"),
                GenericFontFamily::Fantasy => map_font_family("fantasy"),
                _ => default_font_family(),
            }
        }
        FontFamily::FamilyName(name) => {
            // FamilyName's inner field is private, so use ToCss to get the string.
            let css_str = name
                .to_css_string(PrinterOptions::default())
                .unwrap_or_default();
            // Strip surrounding quotes that ToCss may add.
            let trimmed = css_str
                .trim_start_matches(['\'', '"'])
                .trim_end_matches(['\'', '"']);
            // Leak the string so it lives for `'static`.  This is acceptable
            // because font family names are a small, bounded set per page.
            let leaked: &'static str = String::from(trimmed).leak();
            Family::Name(leaked)
        }
    }
}

/// Resolve CSS font-weight to a cosmic-text `Weight`.
fn resolve_weight(prop: Option<&Property<'static>>) -> Weight {
    match prop {
        Some(Property::FontWeight(weight)) => {
            let numeric = match weight {
                CssFontWeight::Absolute(abs) => match abs {
                    AbsoluteFontWeight::Normal => 400.0,
                    AbsoluteFontWeight::Bold => 700.0,
                    AbsoluteFontWeight::Weight(val) => *val,
                },
                // Bolder/Lighter are relative — treat as bold/light since we
                // don't have the inherited weight context here.
                CssFontWeight::Bolder => 700.0,
                CssFontWeight::Lighter => 300.0,
            };
            Weight(numeric as u16)
        }
        _ => Weight::NORMAL,
    }
}

/// Resolve CSS font-style to a cosmic-text `Style`.
fn resolve_style(prop: Option<&Property<'static>>) -> Style {
    match prop {
        Some(Property::FontStyle(css_style)) => match css_style {
            CssFontStyle::Normal => Style::Normal,
            CssFontStyle::Italic => Style::Italic,
            CssFontStyle::Oblique(_) => Style::Oblique,
        },
        _ => Style::Normal,
    }
}
