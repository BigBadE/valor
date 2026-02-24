//! Text measurement using cosmic-text for shaping and layout.
//!
//! Provides single-line and wrapped text measurement that matches
//! Chrome's rounding behaviour (round ascent, descent, leading
//! separately, then sum).

use cosmic_text::{Attrs, Buffer, FontSystem, Metrics, Shaping, Wrap};

use crate::font_system::get_font_metrics;

/// Result of measuring a single line of text.
#[derive(Debug, Clone, Copy)]
pub struct TextMetrics {
    /// Advance width of the shaped text in pixels.
    pub width: f32,
    /// Line height in pixels (rounded to match Chrome).
    pub height: f32,
    /// Ascent above baseline in pixels (rounded).
    pub ascent: f32,
    /// Descent below baseline in pixels (rounded).
    pub descent: f32,
}

/// Result of measuring text that may wrap across multiple lines.
#[derive(Debug, Clone, Copy)]
pub struct WrappedTextMetrics {
    /// Total height of all lines in pixels.
    pub total_height: f32,
    /// Number of lines after wrapping.
    pub line_count: usize,
    /// Maximum line width across all wrapped lines.
    pub max_line_width: f32,
    /// Single-line height (for use in layout calculations).
    pub line_height: f32,
    /// Ascent above baseline in pixels (rounded).
    pub ascent: f32,
    /// Descent below baseline in pixels (rounded).
    pub descent: f32,
}

/// Resolve font metrics, falling back to CSS default 1.2 line-height.
fn resolve_metrics(
    font_system: &mut FontSystem,
    attrs: &Attrs<'_>,
    font_size: f32,
) -> (f32, f32, f32) {
    get_font_metrics(font_system, attrs, font_size).map_or_else(
        || {
            let fallback_height = (font_size * 1.2).round();
            (fallback_height, font_size, 0.0)
        },
        |metrics| (metrics.line_height, metrics.ascent, metrics.descent),
    )
}

/// Measure a single line of text (no wrapping).
///
/// Returns the advance width and the Chrome-compatible line height.
pub fn measure_text(
    font_system: &mut FontSystem,
    text: &str,
    attrs: &Attrs<'_>,
    font_size: f32,
) -> TextMetrics {
    let (line_height, ascent, descent) = resolve_metrics(font_system, attrs, font_size);

    let metrics = Metrics::new(font_size, line_height);
    let mut buffer = Buffer::new(font_system, metrics);

    buffer.set_size(font_system, None, None);
    buffer.set_wrap(font_system, Wrap::None);
    buffer.set_text(font_system, text, attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(font_system, false);

    let width = buffer
        .layout_runs()
        .map(|run| run.line_w)
        .fold(0.0f32, f32::max);

    // Snap to 1/64px grid with ceiling, matching Chrome's LayoutUnit::ceil()
    // for text run widths (ensures inline boxes are wide enough for content).
    let width = (width * 64.0).ceil() / 64.0;

    TextMetrics {
        width,
        height: line_height,
        ascent,
        descent,
    }
}

/// Measure the advance width of text without computing full metrics.
///
/// Slightly cheaper than `measure_text` when only the width is needed.
pub fn measure_text_width(
    font_system: &mut FontSystem,
    text: &str,
    attrs: &Attrs<'_>,
    font_size: f32,
) -> f32 {
    measure_text(font_system, text, attrs, font_size).width
}

/// Measure text that may wrap within `max_width` pixels.
///
/// Uses word-or-glyph wrapping (same as CSS `overflow-wrap: break-word`).
pub fn measure_text_wrapped(
    font_system: &mut FontSystem,
    text: &str,
    attrs: &Attrs<'_>,
    font_size: f32,
    max_width: f32,
) -> WrappedTextMetrics {
    let (line_height, ascent, descent) = resolve_metrics(font_system, attrs, font_size);

    let glyph_height = ascent + descent;

    let metrics = Metrics::new(font_size, line_height);
    let mut buffer = Buffer::new(font_system, metrics);

    buffer.set_size(font_system, Some(max_width), None);
    buffer.set_wrap(font_system, Wrap::WordOrGlyph);
    buffer.set_text(font_system, text, attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(font_system, false);

    let mut line_count: usize = 0;
    let mut max_line_width: f32 = 0.0;

    // Count visual lines by inspecting each buffer line's layout entries.
    // A single buffer line (logical line) can produce multiple visual lines
    // when text wraps. `layout_runs()` yields runs tagged with buffer line
    // index (`line_i`), not visual line index, so we count layout entries
    // directly to get the correct visual line count.
    for line in &buffer.lines {
        let visual_lines = line
            .layout_opt()
            .as_ref()
            .map_or(1, |layouts| layouts.len().max(1));
        line_count += visual_lines;
    }

    for run in buffer.layout_runs() {
        // Snap to 1/64px grid with ceiling, matching Chrome's LayoutUnit::ceil().
        let run_w = (run.line_w * 64.0).ceil() / 64.0;
        max_line_width = max_line_width.max(run_w);
    }

    // Empty text still produces one line.
    if line_count == 0 {
        line_count = 1;
    }

    // Chrome computes total height as:
    //   (line_count - 1) * line_height + glyph_height
    // This means the last line uses just ascent+descent (no trailing leading).
    let total_height = if line_count == 1 {
        line_height
    } else {
        (line_count as f32 - 1.0).mul_add(line_height, glyph_height)
    };

    WrappedTextMetrics {
        total_height,
        line_count,
        max_line_width,
        line_height,
        ascent,
        descent,
    }
}
