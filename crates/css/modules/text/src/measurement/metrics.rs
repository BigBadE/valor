//! Text measurement metrics and calculations.

use super::font_attrs::prepare_font_attrs;
use super::font_system::{get_font_metrics, get_font_system};
use css_orchestrator::style_model::ComputedStyle;
use glyphon::{Attrs, Buffer, FontSystem, Metrics, Shaping, Weight, cosmic_text::Wrap};
use std::sync::PoisonError;

/// Measured text dimensions.
#[derive(Debug, Clone, Copy)]
pub struct TextMetrics {
    /// Width of the text in pixels.
    pub width: f32,
    /// Height of the text LINE in pixels (CSS line-height) - ROUNDED for layout.
    /// This is ascent + descent + `line_gap` from actual font metrics,
    /// or explicit line-height from CSS if specified.
    /// This is what CSS layout uses for box sizing.
    pub height: f32,
    /// Unrounded line height in pixels - for rendering to match layout calculations.
    pub height_unrounded: f32,
    /// Actual rendered glyph height (ascent + descent from font metrics).
    /// This is the bounding box of the actual glyphs, used for text node rects.
    pub glyph_height: f32,
    /// Ascent from the baseline (positive, upward).
    pub ascent: f32,
    /// Descent from the baseline (positive, downward).
    pub descent: f32,
    /// Matched font weight after CSS font matching algorithm (e.g., requested 300 -> matched 400).
    pub matched_font_weight: u16,
}

/// Wrapped text measurement result.
#[derive(Debug, Clone, Copy)]
pub struct WrappedTextMetrics {
    /// Total height of all wrapped lines in pixels.
    pub total_height: f32,
    /// Number of wrapped lines.
    pub line_count: usize,
    /// Glyph height (ascent + descent) in pixels.
    pub glyph_height: f32,
    /// Ascent from baseline in pixels.
    pub ascent: f32,
    /// Descent from baseline in pixels.
    pub descent: f32,
    /// Height of a single line in pixels.
    pub single_line_height: f32,
    /// Actual maximum width of wrapped lines in pixels.
    pub actual_width: f32,
}

/// Computed line height metrics.
#[derive(Debug, Clone, Copy)]
struct LineHeightMetrics {
    /// Rounded glyph height (ascent + descent).
    glyph_height: f32,
    /// Rounded ascent.
    ascent: f32,
    /// Rounded descent.
    descent: f32,
    /// Rounded line height for layout.
    line_height: f32,
    /// Unrounded line height for cosmic-text.
    line_height_unrounded: f32,
}

/// Compute line height metrics from font metrics and style.
fn compute_line_height_metrics(
    font_metrics: Option<super::font_system::FontMetricsData>,
    font_size: f32,
    style: &ComputedStyle,
) -> LineHeightMetrics {
    font_metrics.map_or_else(
        || {
            // Fallback if font metrics unavailable
            let fallback = style.line_height.unwrap_or(font_size);
            LineHeightMetrics {
                glyph_height: font_size,
                ascent: font_size * 0.8,
                descent: font_size * 0.2,
                line_height: fallback,
                line_height_unrounded: fallback,
            }
        },
        |metrics| {
            let ascent_px = metrics.ascent * font_size;
            let descent_px = metrics.descent * font_size;
            let leading_px = metrics.leading * font_size;
            let glyph_h = ascent_px + descent_px;

            // CSS "normal" line-height = ascent + descent + leading (line-gap from font metrics)
            let normal_line_h = glyph_h + leading_px;

            // Unrounded line height for cosmic-text internal calculations
            let line_h_unrounded = style.line_height.unwrap_or(normal_line_h);

            // Platform-specific rounding strategy
            // Windows: round components individually (matches GDI behavior)
            // Non-Windows: floor total height (matches FreeType/Chrome on Linux)
            #[cfg(target_os = "windows")]
            let (glyph_h_rounded, ascent_rounded, descent_rounded) = {
                let asc = ascent_px.round();
                let desc = descent_px.round();
                (asc + desc, asc, desc)
            };

            #[cfg(not(target_os = "windows"))]
            let (glyph_h_rounded, ascent_rounded, descent_rounded) = {
                let glyph_h = glyph_h.floor();
                (glyph_h, ascent_px.round(), descent_px.round())
            };

            let leading_rounded = leading_px.round();
            let normal_line_h_rounded = glyph_h_rounded + leading_rounded;
            let line_h = style.line_height.unwrap_or(normal_line_h_rounded);

            LineHeightMetrics {
                glyph_height: glyph_h_rounded,
                ascent: ascent_rounded,
                descent: descent_rounded,
                line_height: line_h,
                line_height_unrounded: line_h_unrounded,
            }
        },
    )
}

/// Get matched font weight from font system.
fn get_matched_font_weight(
    font_sys: &mut FontSystem,
    attrs: &Attrs<'_>,
    style: &ComputedStyle,
) -> u16 {
    let font_matches = font_sys.get_font_matches(attrs);
    font_matches.first().map_or_else(
        || {
            if style.font_weight == 0 {
                400
            } else {
                style.font_weight
            }
        },
        |first_match| first_match.font_weight,
    )
}

/// Measure text using actual font metrics from glyphon.
///
/// This function uses glyphon's font shaping to get exact text dimensions.
/// Returns the LINE HEIGHT (the vertical space needed for the line), which matches
/// what CSS layout uses for box sizing.
///
/// # Arguments
/// * `text` - The text to measure (whitespace should be collapsed beforehand)
/// * `style` - The computed style containing `font-size` and other properties
///
/// # Returns
/// `TextMetrics` with actual width and LINE HEIGHT from font shaping.
/// The height includes the line-height spacing as used by CSS.
///
/// # Panics
/// Panics if `font_size` in the style is 0.0 or not set.
pub fn measure_text(text: &str, style: &ComputedStyle) -> TextMetrics {
    let font_system = get_font_system();
    let mut font_sys = font_system.lock().unwrap_or_else(PoisonError::into_inner);

    let font_size = style.font_size;
    assert!(
        font_size > 0.0,
        "font_size must be specified in ComputedStyle"
    );

    let attrs = prepare_font_attrs(style);
    let matched_font_weight = get_matched_font_weight(&mut font_sys, &attrs, style);
    let attrs = attrs.weight(Weight(matched_font_weight));

    let font_metrics = get_font_metrics(&mut font_sys, &attrs);


    let metrics = compute_line_height_metrics(font_metrics, font_size, style);

    if text.is_empty() {
        return TextMetrics {
            width: 0.0,
            height: metrics.line_height,
            height_unrounded: metrics.line_height_unrounded,
            glyph_height: metrics.glyph_height,
            ascent: metrics.ascent,
            descent: metrics.descent,
            matched_font_weight,
        };
    }

    let width = measure_text_width_internal(&mut font_sys, text, &attrs, font_size, &metrics);

    TextMetrics {
        width,
        height: metrics.line_height,
        height_unrounded: metrics.line_height_unrounded,
        glyph_height: metrics.glyph_height,
        ascent: metrics.ascent,
        descent: metrics.descent,
        matched_font_weight,
    }
}

/// Internal function to measure text width using shaped buffer.
fn measure_text_width_internal(
    font_sys: &mut FontSystem,
    text: &str,
    attrs: &Attrs<'_>,
    font_size: f32,
    metrics: &LineHeightMetrics,
) -> f32 {
    let buffer_metrics = Metrics::new(font_size, metrics.line_height_unrounded);
    let mut buffer = Buffer::new(font_sys, buffer_metrics);

    buffer.set_text(font_sys, text, attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(font_sys, false);

    let mut max_width = 0.0f32;
    for line_idx in 0..buffer.lines.len() {
        if let Some(layout_lines) = buffer.line_layout(font_sys, line_idx) {
            for layout_line in layout_lines {
                let width = layout_line.w;
                max_width = max_width.max(width);
            }
        }
    }

    max_width
}

/// Measure text with wrapping at a specific width.
///
/// Returns the total height needed for the text when wrapped to fit within
/// the given width, along with the number of lines and glyph metrics.
///
/// # Arguments
/// * `text` - The text to measure (whitespace should be collapsed beforehand)
/// * `style` - The computed style containing font-size
/// * `max_width` - Maximum width in pixels before wrapping
///
/// # Returns
/// `WrappedTextMetrics` containing total height, line count, and glyph metrics.
///
/// # Panics
/// Panics if `font_size` in the style is 0.0 or not set.
pub fn measure_text_wrapped(
    text: &str,
    style: &ComputedStyle,
    max_width: f32,
) -> WrappedTextMetrics {
    let font_system = get_font_system();
    let mut font_sys = font_system.lock().unwrap_or_else(PoisonError::into_inner);

    let font_size = style.font_size;
    assert!(
        font_size > 0.0,
        "font_size must be specified in ComputedStyle"
    );

    let attrs = prepare_font_attrs(style);
    let matched_font_weight = get_matched_font_weight(&mut font_sys, &attrs, style);
    let attrs = attrs.weight(Weight(matched_font_weight));

    let font_metrics = get_font_metrics(&mut font_sys, &attrs);
    let metrics = compute_line_height_metrics(font_metrics, font_size, style);

    if text.is_empty() {
        return WrappedTextMetrics {
            total_height: metrics.line_height,
            line_count: 0,
            glyph_height: metrics.glyph_height,
            ascent: metrics.ascent,
            descent: metrics.descent,
            single_line_height: metrics.line_height,
            actual_width: 0.0,
        };
    }

    let layout_result = measure_wrapped_layout(
        &mut font_sys,
        &WrappedMeasureParams {
            text,
            attrs: &attrs,
            font_size,
            max_width,
            metrics: &metrics,
        },
    );

    let (final_glyph_height, final_ascent, final_descent) =
        if layout_result.max_ascent > 0.0 || layout_result.max_descent > 0.0 {
            let actual_glyph_h = layout_result.max_ascent + layout_result.max_descent;
            (
                actual_glyph_h,
                layout_result.max_ascent,
                layout_result.max_descent,
            )
        } else {
            (metrics.glyph_height, metrics.ascent, metrics.descent)
        };

    let total_height = layout_result.line_count as f32 * metrics.line_height;

    WrappedTextMetrics {
        total_height,
        line_count: layout_result.line_count,
        glyph_height: final_glyph_height,
        ascent: final_ascent,
        descent: final_descent,
        single_line_height: metrics.line_height,
        actual_width: layout_result.max_width,
    }
}

/// Wrapped layout result from buffer shaping.
struct WrappedLayoutResult {
    line_count: usize,
    max_ascent: f32,
    max_descent: f32,
    max_width: f32,
}

/// Parameters for wrapped text measurement.
#[derive(Clone, Copy)]
struct WrappedMeasureParams<'params> {
    text: &'params str,
    attrs: &'params Attrs<'params>,
    font_size: f32,
    max_width: f32,
    metrics: &'params LineHeightMetrics,
}

/// Measure wrapped text layout using a buffer with wrapping enabled.
fn measure_wrapped_layout(
    font_sys: &mut FontSystem,
    params: &WrappedMeasureParams<'_>,
) -> WrappedLayoutResult {
    let buffer_metrics = Metrics::new(params.font_size, params.metrics.line_height_unrounded);
    let mut buffer = Buffer::new(font_sys, buffer_metrics);

    buffer.set_wrap(font_sys, Wrap::WordOrGlyph);
    buffer.set_size(font_sys, Some(params.max_width), None);

    if params.text.contains("Quoted") {
        tracing::debug!(
            "measure_text_wrapped [QUOTED]: {:.3}px, text={:?}",
            params.max_width,
            params.text
        );
    }
    tracing::debug!(
        "measure_text_wrapped: {:.3}px, text_len={}, text_preview={:?}",
        params.max_width,
        params.text.len(),
        &params.text[..params.text.len().min(60)]
    );

    buffer.set_text(font_sys, params.text, params.attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(font_sys, false);

    let mut layout_line_count = 0;
    let mut actual_max_ascent = 0.0f32;
    let mut actual_max_descent = 0.0f32;
    let mut actual_max_width = 0.0f32;

    for line_idx in 0..buffer.lines.len() {
        if let Some(layout_lines) = buffer.line_layout(font_sys, line_idx) {
            layout_line_count += layout_lines.len();
            for layout_line in layout_lines {
                actual_max_ascent = actual_max_ascent.max(layout_line.max_ascent);
                actual_max_descent = actual_max_descent.max(layout_line.max_descent);
                actual_max_width = actual_max_width.max(layout_line.w);
            }
        }
    }

    WrappedLayoutResult {
        line_count: layout_line_count,
        max_ascent: actual_max_ascent,
        max_descent: actual_max_descent,
        max_width: actual_max_width,
    }
}

/// Measure text width using actual font metrics.
///
/// This is a convenience wrapper around `measure_text()` that returns only the width.
///
/// # Arguments
/// * `text` - The text to measure (whitespace should be collapsed beforehand)
/// * `style` - The computed style containing font-size and other properties
///
/// # Returns
/// The width of the text in pixels based on actual font metrics.
pub fn measure_text_width(text: &str, style: &ComputedStyle) -> f32 {
    measure_text(text, style).width
}
