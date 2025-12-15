//! Text preparation for offscreen rendering.

use crate::offscreen::initialization::GlyphonState;
use anyhow::Result as AnyhowResult;
use glyphon::{
    Attrs as GlyphonAttrs, Buffer as GlyphonBuffer, CacheKeyFlags, Color as GlyphonColor,
    Metrics as GlyphonMetrics, Shaping as GlyphonShaping, TextArea, TextBounds, cosmic_text::Wrap,
};
use renderer::display_list::{DisplayItem, DisplayList};
use renderer::renderer::DrawText;
use wgpu::{Device, Queue};

/// Map a display item to text if it's a text item, otherwise return None.
fn map_text_item(item: &DisplayItem) -> Option<DrawText> {
    if let DisplayItem::Text {
        x,
        y,
        text,
        color,
        font_size,
        font_weight,
        font_family,
        line_height,
        bounds,
    } = item
    {
        Some(DrawText {
            x: *x,
            y: *y,
            text: text.clone(),
            color: *color,
            font_size: *font_size,
            font_weight: *font_weight,
            font_family: font_family.clone(),
            line_height: *line_height,
            bounds: *bounds,
        })
    } else {
        None
    }
}

pub type PreparedText = (Vec<DrawText>, Vec<GlyphonBuffer>);

/// Parameters for preparing text items.
pub struct PrepareTextParams<'prepare> {
    pub display_list: &'prepare DisplayList,
    pub glyphon_state: &'prepare mut GlyphonState,
    pub device: &'prepare Device,
    pub queue: &'prepare Queue,
    pub width: u32,
    pub height: u32,
}

/// Helper function to get unrounded line_height from font metrics.
fn get_line_height_unrounded(
    item: &DrawText,
    font_system: &mut glyphon::FontSystem,
) -> f32 {
    use glyphon::cosmic_text::{Attrs, Family, Weight};

    let attrs = Attrs::new()
        .family(match item.font_family.as_deref() {
            Some("monospace") => Family::Monospace,
            Some("serif") => Family::Serif,
            Some(name) => Family::Name(name),
            None => Family::SansSerif,
        })
        .weight(Weight(item.font_weight));

    let font_matches = font_system.get_font_matches(&attrs);
    let weight = Weight(item.font_weight);

    if let Some(first_match) = font_matches.first() {
        if let Some(font) = font_system.get_font(first_match.id, weight) {
            let metrics = font.as_swash().metrics(&[]);
            let ascent = metrics.ascent * item.font_size / metrics.units_per_em as f32;
            let descent = metrics.descent * item.font_size / metrics.units_per_em as f32;
            return ascent + descent;
        }
    }
    item.line_height // Fallback to rounded value
}

/// Prepare text items for rendering.
///
/// # Errors
/// Returns an error if text preparation fails.
pub fn prepare_text_items(params: &mut PrepareTextParams<'_>) -> AnyhowResult<PreparedText> {
    let texts: Vec<DrawText> = params
        .display_list
        .items
        .iter()
        .filter_map(map_text_item)
        .collect();

    let mut buffers: Vec<GlyphonBuffer> = Vec::with_capacity(texts.len());
    for item in &texts {
        // IMPORTANT: Recalculate unrounded line_height from font metrics.
        // The line_height in DisplayItem is rounded to match Chrome's layout calculations,
        // but glyphon needs the original unrounded value for accurate text shaping and wrapping.
        let line_height_unrounded = get_line_height_unrounded(item, &mut params.glyphon_state.font_system);

        let mut buffer = GlyphonBuffer::new(
            &mut params.glyphon_state.font_system,
            GlyphonMetrics::new(item.font_size, line_height_unrounded),
        );
        let attrs = GlyphonAttrs::new().cache_key_flags(CacheKeyFlags::SUBPIXEL_RENDERING);

        // Enable text wrapping by setting buffer size based on bounds BEFORE setting text
        if let Some((left, _top, right, _bottom)) = item.bounds {
            let width = (right - left) as f32;
            buffer.set_wrap(&mut params.glyphon_state.font_system, Wrap::WordOrGlyph);
            buffer.set_size(&mut params.glyphon_state.font_system, Some(width), None);
        }

        buffer.set_text(
            &mut params.glyphon_state.font_system,
            &item.text,
            &attrs,
            GlyphonShaping::Advanced,
            None,
        );

        buffer.shape_until_scroll(&mut params.glyphon_state.font_system, false);

        buffers.push(buffer);
    }
    // First pass: count layout lines for each buffer (needs mutable borrow)
    let mut line_counts: Vec<usize> = Vec::with_capacity(texts.len());
    for buffer in &mut buffers {
        let mut layout_line_count = 0;
        for line_idx in 0..buffer.lines.len() {
            if let Some(layout_lines) = buffer.line_layout(&mut params.glyphon_state.font_system, line_idx) {
                layout_line_count += layout_lines.len();
            }
        }
        line_counts.push(layout_line_count);
    }

    let mut areas: Vec<TextArea> = Vec::with_capacity(texts.len());
    for (index, item) in texts.iter().enumerate() {
        let layout_line_count = line_counts[index];

        // Get unrounded line_height for bounds adjustment
        let line_height_unrounded = get_line_height_unrounded(item, &mut params.glyphon_state.font_system);


        // Convert RGB [f32; 3] to Glyphon RGBA u32 format: 0xAARRGGBB
        // Cosmic-text Color format: ((a << 24) | (r << 16) | (g << 8) | b)
        let red = (item.color[0] * 255.0).clamp(0.0, 255.0) as u32;
        let green = (item.color[1] * 255.0).clamp(0.0, 255.0) as u32;
        let blue = (item.color[2] * 255.0).clamp(0.0, 255.0) as u32;
        let alpha = 0xFF; // Opaque
        let color = GlyphonColor((alpha << 24) | (red << 16) | (green << 8) | blue);
        // Adjust bounds to account for unrounded line_height rendering
        // Layout calculates height with rounded line_height, but glyphon renders with unrounded line_height.
        // We need to expand the bottom bound to prevent clipping.
        let bounds = match item.bounds {
            Some((left, top, right, bottom)) => {
                // Calculate the actual rendered height based on unrounded line_height
                let rendered_height = layout_line_count as f32 * line_height_unrounded;
                // Calculate the layout height based on rounded line_height (what the bounds currently represent)
                let layout_height = layout_line_count as f32 * item.line_height;
                // Adjust bottom bound to include the extra height needed for unrounded rendering
                let height_adjustment = (rendered_height - layout_height).ceil() as i32;
                let adjusted_bottom = bottom + height_adjustment;


                TextBounds {
                    left,
                    top,
                    right,
                    bottom: adjusted_bottom,
                }
            },
            None => TextBounds {
                left: 0,
                top: 0,
                right: i32::try_from(params.width).unwrap_or(i32::MAX),
                bottom: i32::try_from(params.height).unwrap_or(i32::MAX),
            },
        };
        areas.push(TextArea {
            buffer: &buffers[index],
            left: item.x,
            top: item.y,
            scale: 1.0,
            bounds,
            default_color: color,
            custom_glyphs: &[],
        });
    }
    params.glyphon_state.text_renderer.prepare(
        params.device,
        params.queue,
        &mut params.glyphon_state.font_system,
        &mut params.glyphon_state.text_atlas,
        &params.glyphon_state.viewport,
        areas,
        &mut params.glyphon_state.swash_cache,
    )?;
    Ok((texts, buffers))
}
