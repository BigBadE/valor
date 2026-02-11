//! GPU initialization for offscreen rendering.

use anyhow::Result as AnyhowResult;
use glyphon::{
    Cache as GlyphonCache, FontSystem, Resolution, SwashCache, TextAtlas, TextRenderer, Viewport,
};
use pollster::block_on;
use wgpu::{RenderPipeline, *};

/// GPU context for offscreen rendering.
pub struct OffscreenGpuContext {
    /// WGPU device for creating GPU resources.
    pub device: Device,
    /// Command queue for submitting GPU work.
    pub queue: Queue,
}

/// Persistent GPU context that can be reused across multiple renders.
/// This includes all the expensive-to-create resources.
pub struct PersistentGpuContext {
    /// GPU device.
    pub device: Device,
    /// Command queue.
    pub queue: Queue,
    /// Render pipeline.
    pub pipeline: RenderPipeline,
    /// Glyphon cache for text rendering.
    pub glyphon_cache: GlyphonCache,
    /// Glyphon state (font system, text atlas, renderer, viewport).
    pub glyphon_state: GlyphonState,
    /// Current render format.
    pub render_format: TextureFormat,
}

/// Initialize GPU device and queue for offscreen rendering.
///
/// # Errors
/// Returns an error if GPU adapter or device initialization fails.
pub fn initialize_gpu() -> AnyhowResult<OffscreenGpuContext> {
    let instance = Instance::new(&InstanceDescriptor::default());
    let adapter = block_on(instance.request_adapter(&RequestAdapterOptions::default()))
        .map_err(|err| anyhow::anyhow!("wgpu adapter not found: {err}"))?;
    let (device, queue) = block_on(adapter.request_device(&DeviceDescriptor {
        label: Some("offscreen-render-device"),
        required_features: Features::DUAL_SOURCE_BLENDING,
        ..Default::default()
    }))?;
    Ok(OffscreenGpuContext { device, queue })
}

/// Create an offscreen render texture.
pub fn create_render_texture(
    device: &Device,
    width: u32,
    height: u32,
    render_format: TextureFormat,
) -> (Texture, TextureView) {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("offscreen-target"),
        size: Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: render_format,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let texture_view = texture.create_view(&TextureViewDescriptor::default());
    (texture, texture_view)
}

/// Glyphon text rendering state.
pub struct GlyphonState {
    /// Font system for loading and managing fonts.
    pub font_system: FontSystem,
    /// Texture atlas for glyph caching.
    pub text_atlas: TextAtlas,
    /// Text renderer for drawing glyphs.
    pub text_renderer: TextRenderer,
    /// Viewport for coordinate transformation.
    pub viewport: Viewport,
    /// Swash cache for rasterizing glyphs.
    pub swash_cache: SwashCache,
}

/// Parameters for initializing Glyphon.
pub struct GlyphonInitParams<'init> {
    /// GPU device reference.
    pub device: &'init Device,
    /// Command queue reference.
    pub queue: &'init Queue,
    /// Glyph cache reference.
    pub glyphon_cache: &'init GlyphonCache,
    /// Render texture format.
    pub render_format: TextureFormat,
    /// Viewport width in pixels.
    pub width: u32,
    /// Viewport height in pixels.
    pub height: u32,
}

/// Initialize Glyphon text rendering state.
pub fn initialize_glyphon(params: &GlyphonInitParams<'_>) -> GlyphonState {
    let mut font_system = FontSystem::new();
    font_system.db_mut().load_system_fonts();
    let mut text_atlas = TextAtlas::new(
        params.device,
        params.queue,
        params.glyphon_cache,
        params.render_format,
    );
    let text_renderer = TextRenderer::new(
        &mut text_atlas,
        params.device,
        MultisampleState::default(),
        None,
    );
    let mut viewport = Viewport::new(params.device, params.glyphon_cache);
    viewport.update(
        params.queue,
        Resolution {
            width: params.width,
            height: params.height,
        },
    );
    let swash_cache = SwashCache::new();
    GlyphonState {
        font_system,
        text_atlas,
        text_renderer,
        viewport,
        swash_cache,
    }
}

/// Initialize a persistent GPU context that can be reused across renders.
///
/// # Errors
/// Returns an error if GPU initialization fails.
pub fn initialize_persistent_context(
    width: u32,
    height: u32,
) -> AnyhowResult<PersistentGpuContext> {
    let OffscreenGpuContext { device, queue } = initialize_gpu()?;
    let render_format = TextureFormat::Bgra8Unorm;
    let pipeline = super::rendering::build_pipeline(&device, render_format);
    let glyphon_cache = GlyphonCache::new(&device);
    let glyphon_state = initialize_glyphon(&GlyphonInitParams {
        device: &device,
        queue: &queue,
        glyphon_cache: &glyphon_cache,
        render_format,
        width,
        height,
    });
    Ok(PersistentGpuContext {
        device,
        queue,
        pipeline,
        glyphon_cache,
        glyphon_state,
        render_format,
    })
}
