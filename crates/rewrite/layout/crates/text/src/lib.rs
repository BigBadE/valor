//! Text layout crate - handles text measurement and inline layout.
//!
//! This crate provides text metrics computation for determining the height
//! and width of text content using actual font metrics from cosmic-text.

use glyphon::{Attrs, Family, FontSystem, Weight};
use rewrite_core::{NodeId, Relationship, ScopedDb};
use rewrite_css::{
    CssKeyword, CssValue, DisplayQuery, FontSizeQuery, FontWeightQuery, LengthValue,
    LineHeightQuery, Subpixels,
};
use rewrite_html::TextContentQuery;
use std::sync::{Arc, Mutex, PoisonError};

// Global font system (lazy-initialized)
type FontSystemOption = Option<Arc<Mutex<FontSystem>>>;
static FONT_SYSTEM: Mutex<FontSystemOption> = Mutex::new(None);

/// Get or initialize the global font system.
fn get_font_system() -> Arc<Mutex<FontSystem>> {
    let mut guard = FONT_SYSTEM.lock().unwrap_or_else(PoisonError::into_inner);

    if let Some(ref font_sys) = *guard {
        Arc::clone(font_sys)
    } else {
        let mut font_system = FontSystem::new();
        font_system.db_mut().load_system_fonts();

        // Set platform-specific defaults to match Chrome
        #[cfg(target_os = "windows")]
        {
            font_system.db_mut().set_sans_serif_family("Arial");
            font_system.db_mut().set_serif_family("Times New Roman");
            font_system.db_mut().set_monospace_family("Consolas");
        }

        #[cfg(all(unix, not(target_os = "macos")))]
        {
            font_system.db_mut().set_serif_family("Liberation Serif");
        }

        let arc = Arc::new(Mutex::new(font_system));
        *guard = Some(Arc::clone(&arc));
        arc
    }
}

/// Font metrics data from actual font file.
#[derive(Debug, Clone, Copy)]
pub struct FontMetricsData {
    /// Ascent in normalized units (0-1 range, multiply by font_size to get pixels).
    pub ascent: f32,
    /// Descent in normalized units (positive value).
    pub descent: f32,
    /// Leading (line-gap) in normalized units.
    pub leading: f32,
}

/// Get font metrics from actual font file.
fn get_actual_font_metrics(font_size: f32, font_weight: u16) -> Option<FontMetricsData> {
    let font_sys_arc = get_font_system();
    let mut font_sys = font_sys_arc.lock().unwrap_or_else(PoisonError::into_inner);

    // Create font attributes (use sans-serif as default for now)
    let attrs = Attrs::new()
        .family(Family::SansSerif)
        .weight(Weight(font_weight));

    // Get font matches
    let font_matches = font_sys.get_font_matches(&attrs);
    let first_match = font_matches.first()?;

    // Convert weight for font lookup
    use glyphon::fontdb::Weight as FontdbWeight;
    let weight = FontdbWeight(first_match.font_weight);

    // Get the actual font
    let font = font_sys.get_font(first_match.id, weight)?;

    // Get font metrics
    let metrics = font.as_ref().metrics();
    let units_per_em = f32::from(metrics.units_per_em);

    // Platform-specific metric extraction (matching Chrome's behavior)
    #[cfg(target_os = "windows")]
    let (ascent, descent, leading) = {
        // Chrome on Windows uses OS/2 win metrics (no line gap) by default
        if let Some((win_ascent, win_descent)) = font.os2_metrics() {
            (win_ascent, win_descent, 0.0)
        } else {
            // Fallback to hhea metrics
            let ascent = metrics.ascent / units_per_em;
            let descent = -metrics.descent / units_per_em;
            let leading = metrics.leading / units_per_em;
            (ascent, descent, leading)
        }
    };

    #[cfg(not(target_os = "windows"))]
    let (ascent, descent, leading) = {
        // Linux/macOS use hhea metrics with leading
        let ascent = metrics.ascent / units_per_em;
        let descent = -metrics.descent / units_per_em;
        let leading = metrics.leading / units_per_em;
        (ascent, descent, leading)
    };

    Some(FontMetricsData {
        ascent,
        descent,
        leading,
    })
}

/// Font metrics for text measurement.
#[derive(Debug, Clone, Copy)]
pub struct FontMetrics {
    /// Font size in subpixels.
    pub font_size: Subpixels,
    /// Ascent (height above baseline) in subpixels.
    pub ascent: Subpixels,
    /// Descent (depth below baseline) in subpixels.
    pub descent: Subpixels,
    /// Line height in subpixels.
    pub line_height: Subpixels,
}

impl FontMetrics {
    /// Calculate font metrics from CSS properties using actual font data.
    pub fn from_element(scoped: &mut ScopedDb) -> Self {
        let font_size = get_font_size(scoped);
        let font_size_px = font_size as f32 / 64.0; // Convert subpixels to pixels

        // Get font weight
        let font_weight = scoped.query::<FontWeightQuery>();
        let weight = match font_weight {
            CssValue::Number(w) => w as u16,
            _ => 400, // Normal weight
        };

        // Try to get actual font metrics
        if let Some(metrics_data) = get_actual_font_metrics(font_size_px, weight) {
            let ascent_px = metrics_data.ascent * font_size_px;
            let descent_px = metrics_data.descent * font_size_px;
            let leading_px = metrics_data.leading * font_size_px;

            // Round each component separately (Chrome's behavior)
            let ascent_rounded = ascent_px.round();
            let descent_rounded = descent_px.round();
            let leading_rounded = leading_px.round();

            // Normal line-height = ascent + descent + leading (all rounded separately)
            let normal_line_height_px = ascent_rounded + descent_rounded + leading_rounded;

            // Check for explicit CSS line-height
            let line_height_px = get_line_height(scoped, font_size);
            let final_line_height = if line_height_px > 0 {
                line_height_px
            } else {
                (normal_line_height_px * 64.0) as Subpixels
            };

            Self {
                font_size,
                ascent: (ascent_rounded * 64.0) as Subpixels,
                descent: (descent_rounded * 64.0) as Subpixels,
                line_height: final_line_height,
            }
        } else {
            // Fallback to estimates if font metrics unavailable
            let line_height = get_line_height(scoped, font_size);
            let ascent = (font_size as f32 * 0.75) as Subpixels;
            let descent = (font_size as f32 * 0.25) as Subpixels;

            Self {
                font_size,
                ascent,
                descent,
                line_height: if line_height > 0 {
                    line_height
                } else {
                    (font_size as f32 * 1.2) as Subpixels
                },
            }
        }
    }

    /// Get the total height of the font (ascent + descent).
    pub fn total_height(&self) -> Subpixels {
        self.ascent + self.descent
    }

    /// Get the baseline offset from the top of a line box.
    pub fn baseline_from_top(&self) -> Subpixels {
        let half_leading = (self.line_height - self.total_height()) / 2;
        half_leading + self.ascent
    }
}

/// Compute the height of text content for a block element.
///
/// This function walks through immediate text node children ONLY and computes
/// the total height needed to display those text lines. It does NOT recurse
/// into element children, as those will have their own size computed separately.
pub fn compute_text_content_height(scoped: &mut ScopedDb) -> Subpixels {
    let node = scoped.node();
    let db = scoped.db();

    // Get all children (text nodes and elements)
    let children = db.resolve_relationship(node, Relationship::Children);

    let mut total_height = 0;

    for &child in &children {
        // Only count direct text node children, not element children
        if let Some(text_content) = scoped.node_query::<TextContentQuery>(child) {
            // It's a text node - compute its height based on parent's font metrics
            let metrics = FontMetrics::from_element(scoped);

            // Count lines (simple: just use line_height for now)
            // TODO: Implement proper line breaking and wrapping
            let text = text_content.as_str();
            if text.trim().is_empty() {
                continue;
            }

            // For now, assume single line of text
            total_height += metrics.line_height;
        }
        // Element children are NOT counted here - they will be sized by the
        // block layout size computation which sums children's block sizes
    }

    total_height
}

/// Get the font size for an element in subpixels.
fn get_font_size(scoped: &mut ScopedDb) -> Subpixels {
    let font_size_value = scoped.query::<FontSizeQuery>();

    match font_size_value {
        CssValue::Length(len) => {
            match len {
                LengthValue::Px(px) => (px * 64.0) as Subpixels,
                LengthValue::Em(em) => {
                    // Em is relative to parent font size
                    // For now, use default 16px
                    (em * 16.0 * 64.0) as Subpixels
                }
                LengthValue::Rem(rem) => (rem * 16.0 * 64.0) as Subpixels,
                LengthValue::Vh(vh) => {
                    // Viewport height percentage
                    use rewrite_css::{ViewportInput, ViewportSize};
                    let viewport = scoped
                        .db()
                        .get_input::<ViewportInput>(&())
                        .unwrap_or_else(ViewportSize::default);
                    (vh / 100.0 * viewport.height * 64.0) as Subpixels
                }
                LengthValue::Vw(vw) => {
                    // Viewport width percentage
                    use rewrite_css::{ViewportInput, ViewportSize};
                    let viewport = scoped
                        .db()
                        .get_input::<ViewportInput>(&())
                        .unwrap_or_else(ViewportSize::default);
                    (vw / 100.0 * viewport.width * 64.0) as Subpixels
                }
                _ => 16 * 64, // Default 16px
            }
        }
        CssValue::Keyword(CssKeyword::Inherit) => {
            // Get parent's font size
            if let Some(parent) = scoped.parent_id() {
                let parent_font_size = scoped.node_query::<FontSizeQuery>(parent);
                match parent_font_size {
                    CssValue::Length(LengthValue::Px(px)) => (px * 64.0) as Subpixels,
                    _ => 16 * 64,
                }
            } else {
                16 * 64
            }
        }
        _ => 16 * 64, // Default 16px = 1024 subpixels
    }
}

/// Get the line height for an element in subpixels.
fn get_line_height(scoped: &mut ScopedDb, font_size: Subpixels) -> Subpixels {
    let line_height_value = scoped.query::<LineHeightQuery>();

    match line_height_value {
        CssValue::Number(factor) => {
            // Unitless number is multiplied by font size
            (font_size as f32 * factor) as Subpixels
        }
        CssValue::Length(len) => {
            match len {
                LengthValue::Px(px) => (px * 64.0) as Subpixels,
                LengthValue::Em(em) => (em * font_size as f32) as Subpixels,
                _ => 0, // Return 0 to indicate "use default"
            }
        }
        CssValue::Keyword(CssKeyword::Normal) | _ => {
            // Return 0 to indicate "use default from font metrics"
            0
        }
    }
}
