//! Text rendering with Glyphon for the WGPU backend.

use super::error_scope::ErrorScopeGuard;
use crate::text::{
    TextBatch, batch_layer_texts_with_scissor, batch_texts_with_scissor, map_text_item,
};
use glyphon::{
    Attrs, Buffer as GlyphonBuffer, Cache, Color as GlyphonColor, FontSystem, Metrics, Resolution,
    Shaping, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use log::{debug, error};
use renderer::display_list::{DisplayList, Scissor};
use renderer::renderer::DrawText;
use std::sync::Arc;
use wgpu::*;
use winit::dpi::PhysicalSize;
use winit::window::Window;

/// Glyphon state management.
#[allow(dead_code, reason = "API type for extracted text module")]
pub struct GlyphonState {
    /// Glyphon font system for text rendering.
    pub font_system: FontSystem,
    /// Glyphon swash cache for glyph rasterization.
    pub swash_cache: SwashCache,
    /// Glyphon text atlas for caching rendered glyphs.
    #[allow(dead_code, reason = "Used by glyphon text rendering system")]
    pub text_atlas: TextAtlas,
    /// Glyphon text renderer.
    pub text_renderer: TextRenderer,
    /// Glyphon cache for text layout.
    #[allow(dead_code, reason = "Cache maintained for glyphon state management")]
    pub glyphon_cache: Cache,
    /// Glyphon viewport for text rendering.
    pub viewport: Viewport,
}

/// Create glyphon buffers from text items.
#[allow(dead_code, reason = "API function for extracted text module")]
pub fn create_glyphon_buffers(
    font_system: &mut FontSystem,
    items: &[DrawText],
    scale: f32,
) -> Vec<GlyphonBuffer> {
    let mut buffers = Vec::with_capacity(items.len());
    for item in items {
        let mut buffer = GlyphonBuffer::new(
            font_system,
            Metrics::new(item.font_size * scale, item.font_size * scale),
        );
        let attrs = Attrs::new();
        buffer.set_text(font_system, &item.text, &attrs, Shaping::Advanced);
        buffers.push(buffer);
    }
    buffers
}

/// Create glyphon text areas from buffers and items.
#[allow(dead_code, reason = "API function for extracted text module")]
pub fn create_text_areas<'buffer>(
    buffers: &'buffer [GlyphonBuffer],
    items: &[DrawText],
    scale: f32,
    framebuffer_width: u32,
    framebuffer_height: u32,
) -> Vec<TextArea<'buffer>> {
    let mut areas = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let color = GlyphonColor(0xFF00_0000);
        let bounds = match item.bounds {
            Some((left, top, right, bottom)) => TextBounds {
                left: i32::try_from((left as f32 * scale).round() as u32).unwrap_or(i32::MAX),
                top: i32::try_from((top as f32 * scale).round() as u32).unwrap_or(i32::MAX),
                right: i32::try_from((right as f32 * scale).round() as u32).unwrap_or(i32::MAX),
                bottom: i32::try_from((bottom as f32 * scale).round() as u32).unwrap_or(i32::MAX),
            },
            None => TextBounds {
                left: 0,
                top: 0,
                right: i32::try_from(framebuffer_width).unwrap_or(i32::MAX),
                bottom: i32::try_from(framebuffer_height).unwrap_or(i32::MAX),
            },
        };
        areas.push(TextArea {
            buffer: &buffers[index],
            left: item.x * scale,
            top: item.y * scale,
            scale: 1.0,
            bounds,
            default_color: color,
            custom_glyphs: &[],
        });
    }
    areas
}

/// Parameters for glyphon preparation.
#[allow(dead_code, reason = "API type for extracted text module")]
pub struct GlyphonPrepareParams<'glyphon> {
    /// GPU device for error scope management.
    pub device: &'glyphon Arc<Device>,
    /// GPU queue for viewport updates.
    pub queue: &'glyphon Queue,
    /// Window for scale factor.
    pub window: &'glyphon Arc<Window>,
    /// Current framebuffer size.
    pub size: PhysicalSize<u32>,
    /// Text items to prepare.
    pub items: &'glyphon [DrawText],
}

/// Prepare glyphon buffers for a specific set of text items.
#[allow(dead_code, reason = "API function for extracted text module")]
pub fn glyphon_prepare_for(state: &mut GlyphonState, params: &GlyphonPrepareParams<'_>) {
    let GlyphonPrepareParams {
        device,
        queue,
        window,
        size,
        items,
    } = *params;

    let framebuffer_width = size.width;
    let framebuffer_height = size.height;
    let scale: f32 = window.scale_factor() as f32;
    let buffers = create_glyphon_buffers(&mut state.font_system, items, scale);
    let areas = create_text_areas(
        &buffers,
        items,
        scale,
        framebuffer_width,
        framebuffer_height,
    );

    let viewport_scope_for = ErrorScopeGuard::push(device, "glyphon-viewport-update-for");
    state.viewport.update(
        queue,
        Resolution {
            width: framebuffer_width,
            height: framebuffer_height,
        },
    );
    if let Err(error) = viewport_scope_for.check() {
        error!(target: "wgpu_renderer", "Glyphon viewport.update() (for) generated error: {error:?}");
        return;
    }

    let areas_len = areas.len();
    let prepare_scope_for = ErrorScopeGuard::push(device, "glyphon-text-prepare-for");
    let prep_res = state.text_renderer.prepare(
        device,
        queue,
        &mut state.font_system,
        &mut state.text_atlas,
        &state.viewport,
        areas,
        &mut state.swash_cache,
    );
    if let Err(error) = prepare_scope_for.check() {
        error!(target: "wgpu_renderer", "Glyphon text_renderer.prepare() (for) generated validation error: {error:?}");
    }
    debug!(
        target: "wgpu_renderer",
        "glyphon_prepare_for: items={} areas={} viewport={}x{} result={:?}",
        items.len(),
        areas_len,
        framebuffer_width,
        framebuffer_height,
        prep_res
    );
}

/// Parameters for drawing text batches.
#[allow(dead_code, reason = "API type for extracted text module")]
pub struct DrawTextBatchParams<'text_draw> {
    /// GPU device for error scopes.
    pub device: &'text_draw Arc<Device>,
    /// Current framebuffer size.
    pub size: PhysicalSize<u32>,
    /// Text items to draw.
    pub items: &'text_draw [DrawText],
    /// Optional scissor rect.
    pub scissor_opt: Option<Scissor>,
}

/// Draw a batch of text items with optional scissor rect.
///
/// Note: This function is currently a stub. The actual implementation
/// is in `RenderState::draw_text_batch` which has access to the render pass.
#[allow(dead_code, reason = "API function stub for extracted text module")]
#[inline]
pub const fn draw_text_batch(_state: &GlyphonState, _params: &DrawTextBatchParams<'_>) {
    // This is a stub - actual implementation is in RenderState
    // The extraction was incomplete and this function needs proper refactoring
}

/// Parameters for layer text batching.
#[allow(dead_code, reason = "API type for extracted text module")]
pub struct LayerTextBatchParams<'layer_batch> {
    /// Compositor layers.
    pub layers: &'layer_batch [super::Layer],
    /// Framebuffer width.
    pub width: u32,
    /// Framebuffer height.
    pub height: u32,
}

/// Batch layer texts for rendering.
#[allow(dead_code, reason = "API function for extracted text module")]
pub fn batch_layer_texts(params: &LayerTextBatchParams<'_>) -> Vec<TextBatch> {
    batch_layer_texts_with_scissor(params.layers, params.width, params.height)
}

/// Parameters for display list text batching.
#[allow(dead_code, reason = "API type for extracted text module")]
pub struct DisplayListTextBatchParams<'dl_batch> {
    /// Display list to batch.
    pub display_list: &'dl_batch DisplayList,
    /// Framebuffer width.
    pub width: u32,
    /// Framebuffer height.
    pub height: u32,
}

/// Batch display list texts for rendering.
#[allow(dead_code, reason = "API function for extracted text module")]
pub fn batch_display_list_texts(params: &DisplayListTextBatchParams<'_>) -> Vec<TextBatch> {
    batch_texts_with_scissor(params.display_list, params.width, params.height)
}

/// Extract text items from display list.
#[allow(dead_code, reason = "API function for extracted text module")]
pub fn extract_text_items(display_list: &DisplayList) -> Vec<DrawText> {
    display_list
        .items
        .iter()
        .filter_map(map_text_item)
        .collect()
}
