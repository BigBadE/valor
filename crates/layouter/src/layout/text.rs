//! Text shaping and measurement utilities for inline layout.
//!
//! This module centralizes text width measurement and provides a shaping-aware
//! path behind the `shaping` feature flag. When shaping is disabled, it falls
//! back to a simple character-count approximation scaled by font size.

use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;

#[cfg(feature = "shaping")]
use glyphon::{Buffer as GlyphonBuffer, Metrics, Attrs, Shaping, FontSystem};
#[cfg(not(feature = "shaping"))]
use unicode_bidi::BidiInfo;
use unicode_linebreak::{linebreaks, BreakOpportunity};

/// Metrics for a measured text run.
#[derive(Debug, Clone, Copy)]
pub struct TextMetrics {
    /// Horizontal advance in pixels (integer layout units).
    pub width: i32,
    /// Suggested line height in pixels if available/derived.
    pub line_height: i32,
}

#[cfg(feature = "shaping")]
static FONT_SYSTEM: Lazy<Mutex<FontSystem>> = Lazy::new(|| Mutex::new(FontSystem::new()));

/// A tiny global cache for measured text runs to avoid repeated shaping.
/// Keyed by (text, rounded_font_size_px, fallback_char_width).
type MeasureKey = (String, u32, i32);
static MEASURE_CACHE: Lazy<Mutex<HashMap<MeasureKey, TextMetrics>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// Collapse ASCII whitespace sequences into a single space and trim leading/trailing
/// whitespace similar to CSS Text 3's white-space: normal behavior. This is a
/// simplified approximation that treats spaces, tabs, newlines, and carriage returns
/// as collapsible whitespace. Non-breaking spaces (\u{00A0}) are preserved.
pub fn collapse_whitespace(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_whitespace = false;
    for ch in input.chars() {
        match ch {
            ' ' | '\t' | '\n' | '\r' => {
                if !in_whitespace && !output.is_empty() {
                    output.push(' ');
                }
                in_whitespace = true;
            }
            _ => {
                output.push(ch);
                in_whitespace = false;
            }
        }
    }
    // Trim any trailing space produced by collapsing
    if output.ends_with(' ') {
        output.pop();
    }
    output
}

/// Reorder the text into visual order for display when shaping is not available.
///
/// When the `shaping` feature is enabled, this is a no-op because the shaping
/// engine (glyphon/HarfBuzz) performs bidi reordering internally.
pub fn reorder_bidi_for_display(input: &str) -> String {
    #[cfg(feature = "shaping")]
    {
        // Shaping path handles bidi internally; return as-is
        input.to_string()
    }
    #[cfg(not(feature = "shaping"))]
    {
        if input.is_empty() {
            String::new()
        } else {
            let bidi_info = BidiInfo::new(input, None);
            let mut out = String::with_capacity(input.len());
            for para in &bidi_info.paragraphs {
                let line_range = para.range.clone();
                let reordered = bidi_info.reorder_line(para, line_range);
                out.push_str(&reordered);
                // Preserve paragraph boundaries
                if out.is_empty() || out.ends_with('\n') { /* ok */ } else { out.push('\n'); }
            }
            // Trim trailing newline added by loop if not present in input
            if !input.ends_with('\n') && out.ends_with('\n') { out.pop(); }
            out
        }
    }
}

/// Measure the width of the provided text using the given font size.
///
/// - When compiled with `shaping` feature, this uses glyphon (harfbuzz/swash-backed)
///   shaping to compute an accurate glyph advance width for the string.
/// - Otherwise, it approximates using a fixed average character width scaled
///   from a 16px baseline.
pub fn measure_text_width(text: &str, font_size: f32, fallback_char_width: i32) -> TextMetrics {
    if text.is_empty() {
        return TextMetrics { width: 0, line_height: (font_size * 1.2).round() as i32 };
    }

    // Cache lookup first
    let key: MeasureKey = (text.to_string(), font_size.round() as u32, fallback_char_width);
    if let Ok(cache) = MEASURE_CACHE.lock()
        && let Some(m) = cache.get(&key)
    { return *m; }

    let metrics = {
        #[cfg(feature = "shaping")]
        {
            let mut fs = FONT_SYSTEM.lock().expect("FontSystem lock poisoned");
            let metrics = Metrics::new(font_size, font_size);
            let mut buffer = GlyphonBuffer::new(&mut fs, metrics);
            let attrs = Attrs::new();
            buffer.set_text(&mut fs, text, &attrs, Shaping::Advanced);
            let width = buffer.layout_runs().map(|run| run.line_w).sum::<f32>().round() as i32;
            let line_height = font_size.round() as i32; // renderer uses font_size as vertical scale; refine later
            TextMetrics { width, line_height }
        }
        #[cfg(not(feature = "shaping"))]
        {
            // Fallback: scale approximate average glyph width from a 16px baseline
            let scale = (font_size / 16.0).max(0.01);
            let width = ((text.chars().count() as f32 * fallback_char_width as f32) * scale).round() as i32;
            let line_height = (font_size * 1.1).round() as i32;
            TextMetrics { width, line_height }
        }
    };
    if let Ok(mut cache) = MEASURE_CACHE.lock() { cache.insert(key, metrics); }
    metrics
}

/// Greedy UAX #14 line breaking: compute the widths of each laid-out line for a given
/// text and constraints. The first line may have a reduced remaining width when the
/// current inline cursor is not at the start of the line.
pub fn greedy_line_break_widths(
    text: &str,
    font_size: f32,
    fallback_char_width: i32,
    first_line_remaining: i32,
    line_width: i32,
) -> Vec<i32> {
    if text.is_empty() { return Vec::new(); }
    let mut lines: Vec<i32> = Vec::new();
    let mut current_start: usize = 0;
    let mut last_fit_index: usize = 0;
    let mut last_fit_width: i32 = 0;
    let mut remaining = first_line_remaining.max(0);
    if remaining == 0 { remaining = line_width.max(0); }

    // Collect candidate break points (byte indices). Always include end of text.
    let mut break_points: Vec<usize> = Vec::new();
    for (idx, op) in linebreaks(text) {
        if matches!(op, BreakOpportunity::Mandatory | BreakOpportunity::Allowed) {
            break_points.push(idx);
        }
    }
    if *break_points.last().unwrap_or(&0) != text.len() { break_points.push(text.len()); }

    let mut i = 0usize;
    while i < break_points.len() {
        let idx = break_points[i];
        if idx < current_start { i += 1; continue; }
        let slice = &text[current_start..idx];
        let m = measure_text_width(slice, font_size, fallback_char_width);
        if m.width <= remaining {
            last_fit_index = idx;
            last_fit_width = m.width;
            i += 1;
            continue;
        }
        // If nothing fits on this line, force break at the smallest unit (the first candidate)
        if last_fit_index == current_start {
            // Avoid infinite loop: place at least something (even if overflows)
            lines.push(m.width.min(remaining));
            current_start = idx;
            remaining = line_width;
            last_fit_index = current_start;
            last_fit_width = 0;
            i += 1;
            continue;
        }
        // Commit the last fit and start a new line
        lines.push(last_fit_width);
        current_start = last_fit_index;
        remaining = line_width;
        last_fit_index = current_start;
        last_fit_width = 0;
    }

    // Commit the tail
    if current_start < text.len() {
        let tail = &text[current_start..];
        let m = measure_text_width(tail, font_size, fallback_char_width);
        // Respect remaining for the final line if still on the first line
        let width = if lines.is_empty() { m.width.min(remaining) } else { m.width };
        lines.push(width);
    }

    lines
}
