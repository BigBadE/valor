//! GPU device, surface, and glyphon initialization.

use crate::pipelines::{build_pipeline_and_buffers, build_texture_pipeline};
use anyhow::{Error as AnyhowError, anyhow};
use glyphon::{Cache, FontSystem, Resolution, TextAtlas, TextRenderer, Viewport};
use log::error;
use std::sync::Arc;
use wgpu::*;
use winit::dpi::PhysicalSize;
use winit::window::Window;

/// Surface configuration result: (surface, `surface_format`, `render_format`).
#[allow(dead_code, reason = "API type for extracted initialization module")]
pub type SurfaceConfig = (Option<Surface<'static>>, TextureFormat, TextureFormat);

/// Glyphon rendering resources for initialization.
#[allow(dead_code, reason = "API type for extracted initialization module")]
pub struct GlyphonResources {
    /// Font system for text rendering.
    pub font_system: FontSystem,
    /// Text atlas for glyph caching.
    pub text_atlas: TextAtlas,
    /// Text renderer for drawing.
    pub text_renderer: TextRenderer,
    /// Glyphon cache.
    pub glyphon_cache: Cache,
    /// Viewport for coordinate transformation.
    pub viewport: Viewport,
}

/// Initialize GPU device and queue.
///
/// # Errors
/// Returns an error if adapter or device initialization fails.
#[allow(dead_code, reason = "API function for extracted initialization module")]
pub async fn initialize_surface() -> Result<(Instance, Adapter, Arc<Device>, Queue), AnyhowError> {
    let instance = Instance::new(&InstanceDescriptor {
        backends: Backends::DX12 | Backends::VULKAN | Backends::GL,
        flags: InstanceFlags::VALIDATION | InstanceFlags::DEBUG,
        ..Default::default()
    });
    let adapter = instance
        .request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
        .map_err(|err| anyhow!("Failed to find a suitable GPU adapter: {err}"))?;
    let device_descriptor = DeviceDescriptor {
        label: Some("valor-render-device"),
        required_features: Features::empty(),
        required_limits: Limits::default(),
        memory_hints: MemoryHints::default(),
        trace: Trace::default(),
    };
    let (device, queue) = adapter
        .request_device(&device_descriptor)
        .await
        .map_err(|err| anyhow!("Failed to create GPU device: {err}"))?;
    device.on_uncaptured_error(Box::new(|error| {
        error!(target: "wgpu_renderer", "Uncaptured WGPU error: {error:?}");
    }));
    Ok((instance, adapter, Arc::new(device), queue))
}

/// Setup surface with format selection and configuration.
#[allow(dead_code, reason = "API function for extracted initialization module")]
pub fn setup_surface(
    window: &Arc<Window>,
    instance: &Instance,
    adapter: &Adapter,
    device: &Arc<Device>,
    size: PhysicalSize<u32>,
) -> SurfaceConfig {
    instance.create_surface(Arc::clone(window)).map_or_else(
        |_| {
            (
                None,
                TextureFormat::Rgba8Unorm,
                TextureFormat::Rgba8UnormSrgb,
            )
        },
        |surface| {
            let capabilities = surface.get_capabilities(adapter);
            if capabilities.formats.is_empty() {
                (
                    None,
                    TextureFormat::Rgba8Unorm,
                    TextureFormat::Rgba8UnormSrgb,
                )
            } else {
                let sfmt = capabilities
                    .formats
                    .iter()
                    .copied()
                    .find(|format| {
                        matches!(
                            format,
                            TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb
                        )
                    })
                    .unwrap_or(capabilities.formats[0]);
                let surface_fmt = match sfmt {
                    TextureFormat::Rgba8UnormSrgb => TextureFormat::Rgba8Unorm,
                    other => other,
                };
                let render_fmt = TextureFormat::Rgba8UnormSrgb;
                let surface_config = SurfaceConfiguration {
                    usage: TextureUsages::RENDER_ATTACHMENT,
                    format: surface_fmt,
                    view_formats: vec![render_fmt],
                    alpha_mode: CompositeAlphaMode::Auto,
                    width: size.width,
                    height: size.height,
                    desired_maximum_frame_latency: 2,
                    present_mode: PresentMode::AutoVsync,
                };
                surface.configure(device, &surface_config);
                (Some(surface), surface_fmt, render_fmt)
            }
        },
    )
}

/// Initialize Glyphon text rendering subsystem.
#[allow(dead_code, reason = "API function for extracted initialization module")]
pub fn initialize_glyphon(
    device: &Arc<Device>,
    queue: &Queue,
    render_format: TextureFormat,
    size: PhysicalSize<u32>,
) -> GlyphonResources {
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
    GlyphonResources {
        font_system,
        text_atlas,
        text_renderer,
        glyphon_cache,
        viewport,
    }
}

/// Pipeline initialization bundle.
#[allow(dead_code, reason = "API type for extracted initialization module")]
pub struct PipelineBundle {
    /// Main rectangle rendering pipeline.
    pub pipeline: RenderPipeline,
    /// Textured quad rendering pipeline.
    pub tex_pipeline: RenderPipeline,
    /// Bind group layout for textured quads.
    pub tex_bind_layout: BindGroupLayout,
    /// Linear sampler for texture sampling.
    pub linear_sampler: Sampler,
    /// Initial vertex buffer.
    pub vertex_buffer: Buffer,
    /// Initial vertex count.
    pub vertex_count: u32,
}

/// Initialize rendering pipelines.
#[allow(dead_code, reason = "API function for extracted initialization module")]
pub fn initialize_pipelines(device: &Arc<Device>, render_format: TextureFormat) -> PipelineBundle {
    let (pipeline, vertex_buffer, vertex_count) = build_pipeline_and_buffers(device, render_format);
    let (tex_pipeline, tex_bind_layout, linear_sampler) =
        build_texture_pipeline(device, render_format);
    PipelineBundle {
        pipeline,
        tex_pipeline,
        tex_bind_layout,
        linear_sampler,
        vertex_buffer,
        vertex_count,
    }
}
