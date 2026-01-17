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
    /// Actual shaped glyph ascent from cosmic-text layout (may differ from font metrics).
    pub shaped_ascent: f32,
    /// Actual shaped glyph descent from cosmic-text layout (may differ from font metrics).
    pub shaped_descent: f32,
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

            // Chrome's actual "normal" line-height calculation:
            // Use the font's embedded metrics directly without artificial multipliers.
            // The font designer already specified the intended line-height via the OS/2 table.
            //
            // Platform-specific behavior (handled in font_system.rs):
            // - Windows: Uses OS/2 winAscent + winDescent (no line gap)
            // - Linux/macOS: Uses hhea ascent + descent + leading
            //
            // CRITICAL: Chrome rounds the TOTAL, not individual components.
            // For Liberation Serif 24px: ascent=21.387 + descent=5.191 = 26.578 → round to 27px
            // If we rounded individually: round(21.387)=21 + round(5.191)=5 = 26px ❌
            //
            // This applies to BOTH line-height AND glyph height calculations.
            // Glyph height must also round the sum to match Chrome's behavior.

            // Normal line-height = round(ascent + descent + leading)
            let normal_line_h = (ascent_px + descent_px + leading_px).round();

            // Glyph height for rendering = floor(ascent + descent), no leading
            // Chrome appears to use floor/truncate for glyph height, not round
            // For Liberation Serif 16px: 14.2578 + 3.4609 = 17.7188 → floor = 17px ✓
            let glyph_h_rounded = (ascent_px + descent_px).floor();

            // For glyph positioning, we need individual ascent/descent.
            // Chrome uses ascent/descent from the font metrics for vertical positioning.
            // We round these individually for pixel-aligned positioning.
            let ascent_rounded = ascent_px.round();
            let descent_rounded = descent_px.round();

            // Unrounded line height for cosmic-text internal calculations
            let line_h_unrounded = style.line_height.unwrap_or(normal_line_h);

            // Use explicit line-height from style, or computed normal line-height
            let line_h = style.line_height.unwrap_or(normal_line_h);

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
            shaped_ascent: metrics.ascent,
            shaped_descent: metrics.descent,
        };
    }

    let measure_result = measure_text_internal(&mut font_sys, text, &attrs, font_size, &metrics);

    // Use font-level typographic metrics for shaped bounds, not per-glyph ink bounds
    // Chrome appears to use font ascent/descent, not actual glyph ink bounds
    let shaped_ascent = metrics.ascent;
    let shaped_descent = metrics.descent;

    TextMetrics {
        width: measure_result.width,
        height: metrics.line_height,
        height_unrounded: metrics.line_height_unrounded,
        glyph_height: metrics.glyph_height,
        ascent: metrics.ascent,
        descent: metrics.descent,
        matched_font_weight,
        shaped_ascent,
        shaped_descent,
    }
}

/// Result of internal text measurement with actual glyph bounds.
struct TextMeasureResult {
    width: f32,
    #[allow(dead_code, reason = "Reserved for future glyph bounds calculation")]
    actual_ascent: f32,
    #[allow(dead_code, reason = "Reserved for future glyph bounds calculation")]
    actual_descent: f32,
}

/// Internal function to measure text width and actual glyph bounds using shaped buffer.
fn measure_text_internal(
    font_sys: &mut FontSystem,
    text: &str,
    attrs: &Attrs<'_>,
    font_size: f32,
    metrics: &LineHeightMetrics,
) -> TextMeasureResult {
    let buffer_metrics = Metrics::new(font_size, metrics.line_height_unrounded);
    let mut buffer = Buffer::new(font_sys, buffer_metrics);

    buffer.set_text(font_sys, text, attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(font_sys, false);

    let mut max_width = 0.0f32;

    for line_idx in 0..buffer.lines.len() {
        if let Some(layout_lines) = buffer.line_layout(font_sys, line_idx) {
            for layout_line in layout_lines {
                max_width = max_width.max(layout_line.w);
            }
        }
    }

    TextMeasureResult {
        width: max_width,
        actual_ascent: 0.0,  // Not used anymore
        actual_descent: 0.0, // Not used anymore
    }
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

    // Use font-level typographic metrics, not per-glyph ink bounds
    // Chrome appears to use font ascent/descent for text bounding rects
    let (final_glyph_height, final_ascent, final_descent) =
        (metrics.glyph_height, metrics.ascent, metrics.descent);

    // Chrome's multi-line text height calculation:
    // For single-line text: use line_height
    // For multi-line text: (line_count - 1) * line_height + glyph_height
    // Example: 2 lines with line_height=21px, glyph_height=19px → 1 * 21 + 19 = 40px ✓
    let total_height = if layout_result.line_count == 0 {
        0.0
    } else if layout_result.line_count == 1 {
        // Single line: use line_height
        metrics.line_height
    } else {
        // Multiple lines: spacing between lines + glyph height for last line
        (layout_result.line_count.saturating_sub(1) as f32 * metrics.line_height)
            + final_glyph_height
    };

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
    #[allow(
        dead_code,
        reason = "Reserved for future multi-line layout improvements"
    )]
    max_ascent: f32,
    #[allow(
        dead_code,
        reason = "Reserved for future multi-line layout improvements"
    )]
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
    let mut actual_max_width = 0.0f32;

    for line_idx in 0..buffer.lines.len() {
        if let Some(layout_lines) = buffer.line_layout(font_sys, line_idx) {
            layout_line_count += layout_lines.len();
            for layout_line in layout_lines {
                actual_max_width = actual_max_width.max(layout_line.w);
            }
        }
    }

    WrappedLayoutResult {
        line_count: layout_line_count,
        max_ascent: 0.0,  // Not used anymore
        max_descent: 0.0, // Not used anymore
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
