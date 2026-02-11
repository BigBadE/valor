/// Inline layout module implementing CSS inline formatting context.
///
/// This module handles:
/// - Line breaking and text wrapping
/// - Baseline alignment
/// - Inline-block positioning
/// - Text metrics and font shaping
/// - Bidirectional text (bidi)
///
/// Spec: https://www.w3.org/TR/CSS22/visuren.html#inline-formatting
use crate::{BlockMarker, ConstrainedMarker, InlineMarker, SizeQuery, Subpixels};
use rewrite_core::{NodeId, ScopedDb};
use rewrite_css::{CssKeyword, CssValue, FontSizeQuery, LineHeightQuery, VerticalAlignQuery};

/// Represents a line box in an inline formatting context.
#[derive(Debug, Clone)]
pub struct LineBox {
    /// The offset of this line box from the containing block's top edge.
    pub block_offset: Subpixels,
    /// The inline offset (left edge for LTR).
    pub inline_offset: Subpixels,
    /// The height of this line box.
    pub height: Subpixels,
    /// The baseline offset from the top of the line box.
    pub baseline_offset: Subpixels,
    /// Inline boxes (text runs, inline-blocks) in this line.
    pub inline_boxes: Vec<InlineBox>,
}

/// Represents an inline box (text run or inline-level element).
#[derive(Debug, Clone)]
pub struct InlineBox {
    /// The node that generated this inline box.
    pub node: NodeId,
    /// Inline offset within the line box.
    pub inline_offset: Subpixels,
    /// Width of this inline box.
    pub width: Subpixels,
    /// Height of this inline box.
    pub height: Subpixels,
    /// Baseline offset from the top of this inline box.
    pub baseline_offset: Subpixels,
    /// Type of inline box.
    pub box_type: InlineBoxType,
}

/// Type of inline box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineBoxType {
    /// Text run (rendered text).
    Text,
    /// Inline-block element.
    InlineBlock,
    /// Replaced element (image, etc.).
    Replaced,
    /// Inline box for formatting (span, etc.).
    Inline,
}

/// Baseline alignment mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineAlignment {
    /// Align to the baseline of the line box.
    Baseline,
    /// Align to the top of the line box.
    Top,
    /// Align to the bottom of the line box.
    Bottom,
    /// Align to the middle (halfway between baseline and x-height).
    Middle,
    /// Align to text-top (top of the font's em box).
    TextTop,
    /// Align to text-bottom (bottom of the font's em box).
    TextBottom,
    /// Align to subscript position.
    Sub,
    /// Align to superscript position.
    Super,
    /// Custom vertical offset.
    Custom(Subpixels),
}

/// Text metrics for font shaping and baseline calculation.
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
    /// X-height (height of lowercase 'x') in subpixels.
    pub x_height: Subpixels,
}

impl FontMetrics {
    /// Calculate font metrics from CSS properties.
    pub fn from_element(scoped: &mut ScopedDb) -> Self {
        let font_size = get_font_size(scoped);
        let line_height = get_line_height(scoped, font_size);

        // Estimate ascent/descent based on typical font proportions
        // Real implementation would query font metrics from a font library
        let ascent = (font_size as f32 * 0.75) as Subpixels; // ~75% above baseline
        let descent = (font_size as f32 * 0.25) as Subpixels; // ~25% below baseline
        let x_height = (font_size as f32 * 0.5) as Subpixels; // ~50% of font size

        Self {
            font_size,
            ascent,
            descent,
            line_height,
            x_height,
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

/// Get the font size for an element in subpixels.
fn get_font_size(scoped: &mut ScopedDb) -> Subpixels {
    // Query font-size property (should return subpixels)
    // For now, use a default of 16px = 1024 subpixels
    let font_size_value = scoped.query::<FontSizeQuery>();

    match font_size_value {
        CssValue::Length(len) => {
            // Convert length to subpixels
            use rewrite_css::LengthValue;
            match len {
                LengthValue::Px(px) => (px * 64.0) as Subpixels,
                LengthValue::Em(em) => {
                    // Em is relative to parent font size
                    let parent_size = scoped.parent::<FontSizeQuery>();
                    match parent_size {
                        CssValue::Length(LengthValue::Px(parent_px)) => {
                            (em * parent_px * 64.0) as Subpixels
                        }
                        _ => (em * 16.0 * 64.0) as Subpixels, // Default 16px base
                    }
                }
                LengthValue::Rem(rem) => (rem * 16.0 * 64.0) as Subpixels, // Assume 16px root
                _ => 16 * 64,                                              // Default 16px
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
            // Absolute length
            use rewrite_css::LengthValue;
            match len {
                LengthValue::Px(px) => (px * 64.0) as Subpixels,
                LengthValue::Em(em) => (em * font_size as f32) as Subpixels,
                _ => (font_size as f32 * 1.2) as Subpixels, // Default 1.2x
            }
        }
        CssValue::Keyword(CssKeyword::Normal) | _ => {
            // Default: 1.2x font size
            (font_size as f32 * 1.2) as Subpixels
        }
    }
}

/// Get baseline alignment mode from vertical-align property.
pub fn get_baseline_alignment(scoped: &mut ScopedDb) -> BaselineAlignment {
    let vertical_align = scoped.query::<VerticalAlignQuery>();

    match vertical_align {
        CssValue::Keyword(CssKeyword::Baseline) => BaselineAlignment::Baseline,
        CssValue::Keyword(CssKeyword::Top) => BaselineAlignment::Top,
        CssValue::Keyword(CssKeyword::Bottom) => BaselineAlignment::Bottom,
        CssValue::Keyword(CssKeyword::Middle) => BaselineAlignment::Middle,
        CssValue::Keyword(CssKeyword::TextTop) => BaselineAlignment::TextTop,
        CssValue::Keyword(CssKeyword::TextBottom) => BaselineAlignment::TextBottom,
        CssValue::Keyword(CssKeyword::Sub) => BaselineAlignment::Sub,
        CssValue::Keyword(CssKeyword::Super) => BaselineAlignment::Super,
        CssValue::Length(len) => {
            // Custom offset
            use rewrite_css::LengthValue;
            let offset = match len {
                LengthValue::Px(px) => (px * 64.0) as Subpixels,
                LengthValue::Em(em) => {
                    let font_size = get_font_size(scoped);
                    (em * font_size as f32) as Subpixels
                }
                _ => 0,
            };
            BaselineAlignment::Custom(offset)
        }
        _ => BaselineAlignment::Baseline,
    }
}

/// Compute the vertical offset for an inline box based on baseline alignment.
pub fn compute_baseline_offset(
    alignment: BaselineAlignment,
    font_metrics: &FontMetrics,
    line_box_height: Subpixels,
    line_box_baseline: Subpixels,
) -> Subpixels {
    match alignment {
        BaselineAlignment::Baseline => {
            // Align to line box baseline
            line_box_baseline - font_metrics.ascent
        }
        BaselineAlignment::Top => {
            // Align to top of line box
            0
        }
        BaselineAlignment::Bottom => {
            // Align to bottom of line box
            line_box_height - font_metrics.total_height()
        }
        BaselineAlignment::Middle => {
            // Align to middle of line box
            (line_box_height - font_metrics.total_height()) / 2
        }
        BaselineAlignment::TextTop => {
            // Align to top of the em box
            line_box_baseline - font_metrics.font_size
        }
        BaselineAlignment::TextBottom => {
            // Align to bottom of the em box
            line_box_baseline
        }
        BaselineAlignment::Sub => {
            // Subscript: ~0.2em below baseline
            let offset = (font_metrics.font_size as f32 * 0.2) as Subpixels;
            line_box_baseline - font_metrics.ascent + offset
        }
        BaselineAlignment::Super => {
            // Superscript: ~0.4em above baseline
            let offset = (font_metrics.font_size as f32 * 0.4) as Subpixels;
            line_box_baseline - font_metrics.ascent - offset
        }
        BaselineAlignment::Custom(offset) => {
            // Custom vertical offset
            line_box_baseline - font_metrics.ascent - offset
        }
    }
}

/// Simple line breaking algorithm (greedy).
///
/// This is a simplified implementation. A full implementation would:
/// - Use Unicode line breaking algorithm (UAX #14)
/// - Handle soft hyphens and word breaking
/// - Support language-specific breaking rules
/// - Handle bidirectional text
pub fn break_line(
    text: &str,
    available_width: Subpixels,
    char_width: Subpixels, // Average character width for simplification
) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut current_line_start = 0;
    let mut current_width = 0;
    let mut last_break_opportunity = 0;

    let chars_per_line = (available_width / char_width).max(1) as usize;

    for (idx, ch) in text.char_indices() {
        current_width += char_width;

        // Check for break opportunities (spaces, hyphens)
        if ch.is_whitespace() || ch == '-' {
            last_break_opportunity = idx + ch.len_utf8();
        }

        // Line is too long, need to break
        if current_width > available_width {
            if last_break_opportunity > current_line_start {
                // Break at the last opportunity
                lines.push(&text[current_line_start..last_break_opportunity]);
                current_line_start = last_break_opportunity;
            } else {
                // Force break (no opportunity found)
                lines.push(&text[current_line_start..idx]);
                current_line_start = idx;
            }
            current_width = 0;
            last_break_opportunity = current_line_start;
        }
    }

    // Add remaining text
    if current_line_start < text.len() {
        lines.push(&text[current_line_start..]);
    }

    if lines.is_empty() {
        lines.push("");
    }

    lines
}

/// Layout inline content into line boxes.
///
/// This is a simplified implementation that creates line boxes for text content.
/// A full implementation would:
/// - Handle inline-level elements (span, em, strong, etc.)
/// - Process inline-blocks
/// - Handle floats and text wrapping around them
/// - Support bidirectional text
/// - Implement proper font shaping and text measurement
pub fn layout_inline_content(scoped: &mut ScopedDb, available_width: Subpixels) -> Vec<LineBox> {
    let mut line_boxes = Vec::new();
    let font_metrics = FontMetrics::from_element(scoped);

    // Get text content (simplified: assumes text node children)
    // Real implementation would walk the DOM tree and collect text nodes
    let text_content = get_text_content(scoped);

    if text_content.is_empty() {
        // Empty content: create one empty line box
        line_boxes.push(LineBox {
            block_offset: 0,
            inline_offset: 0,
            height: font_metrics.line_height,
            baseline_offset: font_metrics.baseline_from_top(),
            inline_boxes: vec![],
        });
        return line_boxes;
    }

    // Simple character width estimation (real implementation uses font shaping)
    let char_width = (font_metrics.font_size as f32 * 0.6) as Subpixels; // ~60% of font size

    // Break text into lines
    let lines = break_line(&text_content, available_width, char_width);

    let mut current_block_offset = 0;

    for line_text in lines {
        let line_width = (line_text.len() as i32 * char_width).min(available_width);

        let inline_box = InlineBox {
            node: scoped.node(),
            inline_offset: 0,
            width: line_width,
            height: font_metrics.total_height(),
            baseline_offset: font_metrics.ascent,
            box_type: InlineBoxType::Text,
        };

        let line_box = LineBox {
            block_offset: current_block_offset,
            inline_offset: 0,
            height: font_metrics.line_height,
            baseline_offset: font_metrics.baseline_from_top(),
            inline_boxes: vec![inline_box],
        };

        line_boxes.push(line_box);
        current_block_offset += font_metrics.line_height;
    }

    line_boxes
}

/// Get text content from an element (placeholder).
///
/// Real implementation would:
/// - Walk the DOM tree
/// - Collect text from text nodes
/// - Handle whitespace collapsing
/// - Process text-transform
fn get_text_content(scoped: &mut ScopedDb) -> String {
    // Placeholder: return empty string
    // Real implementation would use TextContentQuery
    String::new()
}

/// Calculate the total height needed for inline content.
pub fn calculate_inline_content_height(
    scoped: &mut ScopedDb,
    available_width: Subpixels,
) -> Subpixels {
    let line_boxes = layout_inline_content(scoped, available_width);
    line_boxes.iter().map(|line| line.height).sum()
}

/// Calculate the intrinsic width of inline content (min-content).
///
/// This is the width needed without any line breaking.
pub fn calculate_inline_intrinsic_width(scoped: &mut ScopedDb) -> Subpixels {
    let font_metrics = FontMetrics::from_element(scoped);
    let text_content = get_text_content(scoped);

    // Simple character width estimation
    let char_width = (font_metrics.font_size as f32 * 0.6) as Subpixels;

    (text_content.len() as i32 * char_width)
}

/// Calculate positions for inline-level elements within a line box.
pub fn position_inline_boxes(
    line_boxes: &mut [LineBox],
    text_align: TextAlign,
    available_width: Subpixels,
) {
    for line_box in line_boxes {
        let total_inline_width: Subpixels = line_box.inline_boxes.iter().map(|b| b.width).sum();
        let free_space = available_width - total_inline_width;

        let start_offset = match text_align {
            TextAlign::Left => 0,
            TextAlign::Right => free_space.max(0),
            TextAlign::Center => (free_space.max(0)) / 2,
            TextAlign::Justify => {
                // Justify: distribute space between words (simplified)
                0 // Would need word counting and space distribution
            }
        };

        let mut current_offset = start_offset;
        for inline_box in &mut line_box.inline_boxes {
            inline_box.inline_offset = current_offset;
            current_offset += inline_box.width;
        }
    }
}

/// Text alignment mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Right,
    Center,
    Justify,
}

/// Get text alignment from CSS text-align property.
pub fn get_text_align(scoped: &mut ScopedDb) -> TextAlign {
    use rewrite_css::TextAlignQuery;
    let text_align = scoped.query::<TextAlignQuery>();

    match text_align {
        CssValue::Keyword(CssKeyword::Left) => TextAlign::Left,
        CssValue::Keyword(CssKeyword::Right) => TextAlign::Right,
        CssValue::Keyword(CssKeyword::Center) => TextAlign::Center,
        CssValue::Keyword(CssKeyword::Justify) => TextAlign::Justify,
        CssValue::Keyword(CssKeyword::Start) => {
            // TODO: Check writing mode direction
            TextAlign::Left
        }
        CssValue::Keyword(CssKeyword::End) => {
            // TODO: Check writing mode direction
            TextAlign::Right
        }
        _ => TextAlign::Left, // Default
    }
}
