//! Text preparation for offscreen rendering.

use crate::offscreen::initialization::GlyphonState;
use anyhow::Result as AnyhowResult;
use glyphon::{
    Attrs as GlyphonAttrs, Buffer as GlyphonBuffer, CacheKeyFlags, Color as GlyphonColor,
    Metrics as GlyphonMetrics, Shaping as GlyphonShaping, TextArea, TextBounds,
    cosmic_text::{Family, Wrap},
};
use renderer::display_list::{DisplayItem, DisplayList};
use renderer::renderer::DrawText;
use std::fs::File;
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
        matched_font_weight,
        font_family,
        line_height,
        line_height_unrounded,
        bounds,
        measured_width,
    } = item
    {
        Some(DrawText {
            x: *x,
            y: *y,
            text: text.clone(),
            color: *color,
            font_size: *font_size,
            font_weight: *font_weight,
            matched_font_weight: *matched_font_weight,
            font_family: font_family.clone(),
            line_height: *line_height,
            line_height_unrounded: *line_height_unrounded,
            bounds: *bounds,
            measured_width: *measured_width,
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

/// Prepare text items for rendering.
///
/// # Errors
/// Returns an error if text preparation fails.
pub fn prepare_text_items(params: &mut PrepareTextParams<'_>) -> AnyhowResult<PreparedText> {
    use std::fs::OpenOptions;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("C:/temp/font_debug.txt")
        .ok();

    debug_log(&mut file, "=== PREPARE_TEXT_ITEMS CALLED ===");

    let texts: Vec<DrawText> = params
        .display_list
        .items
        .iter()
        .filter_map(map_text_item)
        .collect();

    debug_log(
        &mut file,
        &format!("=== Found {} text items ===", texts.len()),
    );

    let mut buffers = create_text_buffers(&texts, params, &mut file);
    // First pass: count layout lines for each buffer (needs mutable borrow)
    let mut line_counts: Vec<usize> = Vec::with_capacity(texts.len());
    for buffer in &mut buffers {
        let mut layout_line_count = 0;
        for line_idx in 0..buffer.lines.len() {
            if let Some(layout_lines) =
                buffer.line_layout(&mut params.glyphon_state.font_system, line_idx)
            {
                layout_line_count += layout_lines.len();
            }
        }
        line_counts.push(layout_line_count);
    }

    let areas = create_text_areas(&texts, &buffers, &line_counts, params.width, params.height);
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

/// Debug logging helper that ignores errors.
fn debug_log(file: &mut Option<File>, message: &str) {
    use std::io::Write as _;
    if let Some(f) = file {
        drop(writeln!(f, "{message}"));
    }
}

/// Create text areas for rendering.
fn create_text_areas<'buffer>(
    texts: &[DrawText],
    buffers: &'buffer [GlyphonBuffer],
    line_counts: &[usize],
    width: u32,
    height: u32,
) -> Vec<TextArea<'buffer>> {
    let mut areas: Vec<TextArea> = Vec::with_capacity(texts.len());
    for (index, item) in texts.iter().enumerate() {
        let layout_line_count = line_counts[index];

        // Use unrounded line_height from layout for bounds adjustment
        let line_height_unrounded = item.line_height_unrounded;

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
            }
            None => TextBounds {
                left: 0,
                top: 0,
                right: i32::try_from(width).unwrap_or(i32::MAX),
                bottom: i32::try_from(height).unwrap_or(i32::MAX),
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
    areas
}

/// Create text buffers for all text items.
fn create_text_buffers(
    texts: &[DrawText],
    params: &mut PrepareTextParams<'_>,
    file: &mut Option<File>,
) -> Vec<GlyphonBuffer> {
    let mut buffers: Vec<GlyphonBuffer> = Vec::with_capacity(texts.len());
    for item in texts {
        let buffer = create_single_buffer(item, params, file);
        buffers.push(buffer);
    }
    buffers
}

/// Create a single text buffer for a text item.
fn create_single_buffer(
    item: &DrawText,
    params: &mut PrepareTextParams<'_>,
    file: &mut Option<File>,
) -> GlyphonBuffer {
    use glyphon::cosmic_text::Weight;

    // Use unrounded line_height from layout for accurate rendering.
    // The line_height field is rounded for layout, but line_height_unrounded
    // contains the original unrounded value for glyphon rendering.
    let mut buffer = GlyphonBuffer::new(
        &mut params.glyphon_state.font_system,
        GlyphonMetrics::new(item.font_size, item.line_height_unrounded),
    );

    // Get the font family
    let family = get_font_family_from_option(item.font_family.as_ref());

    // CRITICAL: Match font weight using cosmic-text's matching algorithm
    // We can't just pass the requested weight directly because fonts may not have
    // that exact weight. For example, Arial only has 400 and 700, so requesting
    // 300 or 600 needs to be matched to the closest available weight.
    let requested_attrs = GlyphonAttrs::new()
        .family(family)
        .weight(Weight(item.font_weight));

    let font_matches = params
        .glyphon_state
        .font_system
        .get_font_matches(&requested_attrs);
    let matched_weight = font_matches
        .first()
        .map_or(item.font_weight, |first_match| first_match.font_weight);

    debug_log(
        file,
        &format!(
            "FONT_MATCH: requested={}, matched={}, family={:?}, text='{}'",
            item.font_weight,
            matched_weight,
            family,
            &item.text[..item.text.len().min(10)]
        ),
    );

    let attrs = GlyphonAttrs::new()
        .family(family)
        .weight(Weight(matched_weight))
        .cache_key_flags(CacheKeyFlags::SUBPIXEL_RENDERING);

    // Enable text wrapping by setting buffer size based on MEASURED width, not bounds
    // The bounds may have rounding errors from integer conversion, so use the original
    // measured width from layout to ensure consistent wrapping behavior
    if item.bounds.is_some() {
        let width = item.measured_width;
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

    buffer
}

/// Get font family from an optional font family string.
fn get_font_family_from_option(family_opt: Option<&String>) -> Family<'static> {
    // We need to own the string to return Family<'static>
    family_opt.map_or_else(get_default_font_family, |family_str| {
        // Map to Family and convert to static by using known constants
        match family_str.to_lowercase().as_str() {
            "sans-serif" => {
                #[cfg(target_os = "windows")]
                {
                    Family::Name("Arial")
                }
                #[cfg(target_os = "macos")]
                {
                    Family::Name("Helvetica")
                }
                #[cfg(not(any(target_os = "windows", target_os = "macos")))]
                {
                    Family::SansSerif
                }
            }
            "serif" => {
                #[cfg(target_os = "windows")]
                {
                    Family::Name("Times New Roman")
                }
                #[cfg(not(target_os = "windows"))]
                {
                    Family::Serif
                }
            }
            "monospace" => {
                #[cfg(target_os = "windows")]
                {
                    Family::Name("Consolas")
                }
                #[cfg(not(target_os = "windows"))]
                {
                    Family::Monospace
                }
            }
            "cursive" => Family::Cursive,
            "fantasy" => Family::Fantasy,
            // For custom font names, we can't convert to 'static easily
            // so fall back to default
            _ => get_default_font_family(),
        }
    })
}

/// Get the default font family for the current platform.
fn get_default_font_family() -> Family<'static> {
    // Default to sans-serif like Chrome
    #[cfg(target_os = "windows")]
    {
        Family::Name("Arial")
    }
    #[cfg(target_os = "macos")]
    {
        Family::Name("Helvetica")
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Family::SansSerif
    }
}
