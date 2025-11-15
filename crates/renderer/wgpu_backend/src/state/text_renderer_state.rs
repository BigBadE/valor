//! Text rendering state for WGPU backend.
//!
//! This module contains the `TextRendererState` struct which encapsulates all text
//! rendering functionality using Glyphon. This is a focused component with a single
//! responsibility: managing text rendering resources and operations.

use super::error_scope::ErrorScopeGuard;
use glyphon::{
    Attrs, Buffer as GlyphonBuffer, Cache, Color as GlyphonColor, FontSystem, Metrics,
    RenderError as GlyphonRenderError, Resolution, Shaping, SwashCache, TextArea, TextAtlas,
    TextBounds, TextRenderer, Viewport,
};
use log::{debug, error};
use renderer::renderer::DrawText;
use std::sync::Arc;
use wgpu::*;
use winit::dpi::PhysicalSize;

/// Text renderer state encapsulating all Glyphon text rendering resources.
/// This struct has a single responsibility: managing text rendering.
pub struct TextRendererState {
    /// Glyphon font system for text rendering.
    font_system: FontSystem,
    /// Glyphon swash cache for glyph rasterization.
    swash_cache: SwashCache,
    /// Glyphon text atlas for caching rendered glyphs.
    text_atlas: TextAtlas,
    /// Glyphon text renderer.
    text_renderer: TextRenderer,
    /// Glyphon cache. Kept alive but not directly accessed.
    _glyphon_cache: Cache,
    /// Glyphon viewport for text rendering.
    viewport: Viewport,
}

impl TextRendererState {
    /// Create a new text renderer state with all Glyphon resources initialized.
    pub fn new(
        device: &Arc<Device>,
        queue: &Queue,
        render_format: TextureFormat,
        size: PhysicalSize<u32>,
    ) -> Self {
        let glyphon_cache = Cache::new(device);
        let mut text_atlas = TextAtlas::new(device, queue, &glyphon_cache, render_format);
        let text_renderer =
            TextRenderer::new(&mut text_atlas, device, MultisampleState::default(), None);
        let mut viewport = Viewport::new(device, &glyphon_cache);
        viewport.update(
            queue,
            Resolution {
                width: size.width,
                height: size.height,
            },
        );
        let mut font_system = FontSystem::new();
        font_system.db_mut().load_system_fonts();

        Self {
            font_system,
            swash_cache: SwashCache::new(),
            text_atlas,
            text_renderer,
            _glyphon_cache: glyphon_cache,
            viewport,
        }
    }

    /// Prepare glyphon buffers for rendering with the current text list.
    pub fn prepare(
        &mut self,
        device: &Arc<Device>,
        queue: &Queue,
        items: &[DrawText],
        viewport_params: (PhysicalSize<u32>, f32), // (size, scale)
    ) {
        let (size, scale) = viewport_params;
        let buffers = self.create_glyphon_buffers(items, scale);
        let areas = Self::create_text_areas(&buffers, items, scale, size.width, size.height);

        let viewport_scope = ErrorScopeGuard::push(device, "glyphon-viewport-update");
        self.viewport.update(
            queue,
            Resolution {
                width: size.width,
                height: size.height,
            },
        );
        if let Err(error) = viewport_scope.check() {
            error!(target: "wgpu_renderer", "Glyphon viewport.update() generated error: {error:?}");
            return;
        }

        let areas_count = areas.len();
        let prepare_scope = ErrorScopeGuard::push(device, "glyphon-text-prepare");
        let prep_res = self.text_renderer.prepare(
            device,
            queue,
            &mut self.font_system,
            &mut self.text_atlas,
            &self.viewport,
            areas,
            &mut self.swash_cache,
        );
        if let Err(error) = prepare_scope.check() {
            error!(target: "wgpu_renderer", "Glyphon text_renderer.prepare() generated validation error: {error:?}");
        }
        debug!(
            target: "wgpu_renderer",
            "glyphon_prepare: areas={} viewport={}x{} result={:?}",
            areas_count,
            size.width,
            size.height,
            prep_res
        );
    }

    /// Render prepared text to the current render pass.
    ///
    /// # Errors
    /// Returns an error if text rendering fails.
    pub fn render(
        &self,
        device: &Arc<Device>,
        pass: &mut RenderPass<'_>,
    ) -> Result<(), GlyphonRenderError> {
        let scope = ErrorScopeGuard::push(device, "glyphon-text-render");
        let result = self
            .text_renderer
            .render(&self.text_atlas, &self.viewport, pass);
        if let Err(error) = scope.check() {
            error!(target: "wgpu_renderer", "Glyphon text_renderer.render() generated validation error: {error:?}");
        }
        result
    }

    /// Reset text renderer for the next frame.
    /// This recreates the text renderer to prevent glyphon state corruption.
    pub fn reset(&mut self, device: &Arc<Device>) {
        // Trim text atlas to prevent unbounded growth
        {
            let scope = ErrorScopeGuard::push(device, "glyphon-atlas-trim");
            self.text_atlas.trim();
            if let Err(error) = scope.check() {
                error!(target: "wgpu_renderer", "Glyphon text_atlas.trim() generated validation error: {error:?}");
            }
        }

        // Recreate text renderer to prevent glyphon state corruption
        {
            let scope = ErrorScopeGuard::push(device, "glyphon-renderer-recreate");
            self.text_renderer = TextRenderer::new(
                &mut self.text_atlas,
                device,
                MultisampleState::default(),
                None,
            );
            if let Err(error) = scope.check() {
                error!(target: "wgpu_renderer", "Glyphon TextRenderer::new() generated validation error: {error:?}");
            }
        }
    }

    /// Create glyphon buffers from text items.
    fn create_glyphon_buffers(&mut self, items: &[DrawText], scale: f32) -> Vec<GlyphonBuffer> {
        let mut buffers = Vec::with_capacity(items.len());
        for item in items {
            let mut buffer = GlyphonBuffer::new(
                &mut self.font_system,
                Metrics::new(item.font_size * scale, item.font_size * scale),
            );
            let attrs = Attrs::new();
            buffer.set_text(&mut self.font_system, &item.text, &attrs, Shaping::Advanced);
            buffers.push(buffer);
        }
        buffers
    }

    /// Create glyphon text areas from buffers and items.
    fn create_text_areas<'buffer>(
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
                    bottom: i32::try_from((bottom as f32 * scale).round() as u32)
                        .unwrap_or(i32::MAX),
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
}
