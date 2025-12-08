//! Main rendering state module.
//!
//! This module contains the `RenderState` struct which coordinates all rendering components.
//! Instead of being a god object, `RenderState` now uses composition to delegate
//! responsibilities to focused components.

// Component modules
// pub(crate) mod bind_group_cache;
pub(crate) mod error_scope;
pub(crate) mod gpu_context;
pub(crate) mod offscreen_target;
pub(crate) mod opacity_compositor;
pub(crate) mod pipeline_cache;
pub(crate) mod rectangle_renderer;
pub(crate) mod render_orchestrator;
pub(crate) mod resource_tracker;
pub(crate) mod text_renderer_state;

// Re-export commonly used items
use crate::error::submit_with_validation;
use crate::pipelines::Vertex;
use crate::text::{batch_layer_texts_with_scissor, batch_texts_with_scissor, map_text_item};
use anyhow::{Error as AnyhowError, Result as AnyResult};
use bytemuck::cast_slice;
use error_scope::ErrorScopeGuard;
use gpu_context::GpuContext;
use log::{debug, error};
use offscreen_target::OffscreenTarget;
use pipeline_cache::PipelineCache;
use rectangle_renderer::RectangleRenderer;
use renderer::display_list::{DisplayItem, DisplayList};
use renderer::renderer::{DrawRect, DrawText};
use resource_tracker::ResourceTracker;
use std::sync::Arc;
pub use text_renderer_state::GlyphBounds;
use text_renderer_state::TextRendererState;
use tracing::info_span;
use wgpu::util::DeviceExt as _;
use wgpu::*;
use winit::dpi::PhysicalSize;
use winit::window::Window;

/// Composite info for a pre-rendered opacity group.
pub(crate) type OpacityComposite = (
    usize,
    usize,
    Texture,
    TextureView,
    u32,
    u32,
    f32,
    Bounds,
    BindGroup,
);

/// Compact representation for preprocessed layer data.
pub(crate) type LayerEntry = Option<(Vec<DisplayItem>, Vec<OpacityComposite>, Vec<(usize, usize)>)>;

/// Type alias for scissor rectangle: (x, y, width, height).
pub(crate) type ScissorRect = (u32, u32, u32, u32);

/// Parameters for offscreen rendering passes.
pub(crate) struct OffscreenRenderParams<'render> {
    pub(crate) encoder: &'render mut CommandEncoder,
    pub(crate) view: &'render TextureView,
    pub(crate) translated_items: &'render [DisplayItem],
    pub(crate) tex_width: u32,
    pub(crate) tex_height: u32,
    pub(crate) ctx: RenderContext,
}

/// Parameters for rendering rectangles pass.
struct RenderRectanglesParams<'render_pass> {
    encoder: &'render_pass mut CommandEncoder,
    texture_view: &'render_pass TextureView,
    use_retained: bool,
    use_layers: bool,
    is_offscreen: bool,
    main_load: LoadOp<Color>,
}

/// Parameters for text rendering pass.
struct RenderTextParams<'text_pass> {
    encoder: &'text_pass mut CommandEncoder,
    texture_view: &'text_pass TextureView,
    text_load: LoadOp<Color>,
    use_retained: bool,
    use_layers: bool,
}

/// Vertex structure for texture quad rendering.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct TexVertex {
    pub(crate) pos: [f32; 2],
    pub(crate) tex_coords: [f32; 2],
}

/// Render performance metrics for observability and debugging.
#[derive(Debug, Clone, Default)]
pub struct RenderMetrics {
    pub frame_time_ms: f64,
    pub draw_calls: u32,
    pub vertices_rendered: u32,
    pub texture_memory_bytes: u64,
    pub error_count: u32,
    pub opacity_groups_rendered: u32,
}

/// Limits to prevent pathological content from exhausting resources.
#[derive(Debug, Clone)]
pub struct RenderLimits {
    pub max_display_items: usize,
    pub max_texture_size: u32,
    pub max_draw_calls_per_frame: u32,
    pub max_nested_opacity_groups: u32,
    pub max_texture_memory_bytes: u64,
}

impl Default for RenderLimits {
    fn default() -> Self {
        Self {
            max_display_items: 100_000,
            max_texture_size: 8192,
            max_draw_calls_per_frame: 10_000,
            max_nested_opacity_groups: 32,
            max_texture_memory_bytes: 512 * 1024 * 1024,
        }
    }
}

/// Feature flags for graceful degradation when GPU is struggling.
#[derive(Debug, Clone)]
pub struct FeatureFlags {
    pub opacity_compositing: bool,
    pub text_rendering: bool,
    pub complex_transforms: bool,
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            opacity_compositing: true,
            text_rendering: true,
            complex_transforms: true,
        }
    }
}

/// Rendering context that encapsulates viewport and size information.
#[derive(Debug, Copy, Clone)]
pub(crate) struct RenderContext {
    viewport_size: PhysicalSize<u32>,
}

impl RenderContext {
    pub(crate) const fn new(size: PhysicalSize<u32>) -> Self {
        Self {
            viewport_size: size,
        }
    }

    pub(crate) fn width(self) -> u32 {
        self.viewport_size.width.max(1)
    }

    pub(crate) fn height(self) -> u32 {
        self.viewport_size.height.max(1)
    }
}

#[derive(Debug, Clone)]
pub enum Layer {
    Background,
    Content(DisplayList),
    Chrome(DisplayList),
}

/// `RenderState` coordinates rendering components using composition instead of inheritance.
/// Each component has a single, well-defined responsibility.
pub struct RenderState {
    /// GPU context managing device, queue, surface, and window.
    gpu: GpuContext,
    /// Pipeline cache managing all rendering pipelines.
    pipelines: PipelineCache,
    /// Text rendering state managing glyphon resources.
    text: TextRendererState,
    /// Rectangle renderer managing vertex buffers.
    rectangles: RectangleRenderer,
    /// Offscreen rendering target managing offscreen textures and readback.
    offscreen: OffscreenTarget,
    /// Resource tracker keeping GPU resources alive through submission.
    resources: ResourceTracker,

    /// Display list of rectangles to render (immediate mode).
    display_list: Vec<DrawRect>,
    /// Display list of text items to render (immediate mode).
    text_list: Vec<DrawText>,
    /// Retained display list (when set, becomes source of truth).
    retained_display_list: Option<DisplayList>,
    /// Optional layers for multi-DL compositing.
    layers: Vec<Layer>,
    /// Clear color for the framebuffer (canvas background). RGBA in [0,1].
    clear_color: [f32; 4],
}

impl RenderState {
    /// Create the GPU device/surface and initialize all rendering components.
    ///
    /// # Errors
    /// Returns an error if GPU initialization fails.
    pub async fn new(window: Arc<Window>) -> Result<Self, AnyhowError> {
        let gpu = GpuContext::new(Arc::clone(&window)).await?;
        let pipelines = PipelineCache::new(gpu.device(), gpu.render_format());
        let text =
            TextRendererState::new(gpu.device(), gpu.queue(), gpu.render_format(), gpu.size());
        let rectangles = RectangleRenderer::new(
            pipelines.initial_vertex_buffer().clone(),
            pipelines.initial_vertex_count(),
        );
        let offscreen = OffscreenTarget::new();
        let resources = ResourceTracker::new();

        Ok(Self {
            gpu,
            pipelines,
            text,
            rectangles,
            offscreen,
            resources,
            display_list: Vec::new(),
            text_list: Vec::new(),
            retained_display_list: None,
            layers: Vec::new(),
            clear_color: [1.0, 1.0, 1.0, 1.0],
        })
    }

    /// Window getter for integrations that require it.
    pub fn get_window(&self) -> &Window {
        self.gpu.window()
    }

    /// Set the framebuffer clear color (canvas background). RGBA in [0,1].
    pub const fn set_clear_color(&mut self, rgba: [f32; 4]) {
        self.clear_color = rgba;
    }

    /// Get glyph bounds from the last prepared text rendering.
    /// Returns per-glyph bounding boxes in screen coordinates.
    #[inline]
    pub fn glyph_bounds(&self) -> &[GlyphBounds] {
        self.text.glyph_bounds()
    }

    /// Handle window resize and reconfigure the surface.
    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.gpu.resize(new_size);
        self.offscreen.clear();
    }

    /// Clear any compositor layers.
    pub fn clear_layers(&mut self) {
        self.layers.clear();
    }

    /// Reset rendering state for the next frame.
    pub fn reset_for_next_frame(&mut self) {
        let _unused = self.gpu.device().poll(PollType::Wait);
        self.resources.clear();
        self.layers.clear();
        self.text.reset(self.gpu.device());
        self.display_list.clear();
        self.text_list.clear();
    }

    /// Push a new compositor layer to be rendered in order.
    pub fn push_layer(&mut self, layer: Layer) {
        self.layers.push(layer);
    }

    /// Update the current display list to be drawn each frame.
    pub fn set_display_list(&mut self, list: Vec<DrawRect>) {
        self.display_list = list;
    }

    /// Update the current text list to be drawn each frame.
    pub fn set_text_list(&mut self, list: Vec<DrawText>) {
        self.text_list = list;
    }

    /// Install a retained display list as the source of truth for rendering.
    pub fn set_retained_display_list(&mut self, list: DisplayList) {
        self.layers.clear();
        self.retained_display_list = Some(list);
        self.display_list.clear();
        self.text_list.clear();
    }

    /// Render a frame by clearing and drawing quads from the current display list.
    ///
    /// # Errors
    /// Returns an error if surface acquisition or rendering fails.
    pub fn render(&mut self) -> Result<(), AnyhowError> {
        let _span = info_span!("renderer.render").entered();
        self.resources.clear();

        let surface_texture = self.gpu.get_current_texture()?;
        let texture_view = surface_texture.texture.create_view(&TextureViewDescriptor {
            format: Some(self.gpu.render_format()),
            ..Default::default()
        });

        let mut encoder = self
            .gpu
            .device()
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("onscreen-frame"),
            });
        self.record_draw_passes(&texture_view, &mut encoder, false)?;

        let command_buffer = encoder.finish();
        submit_with_validation(self.gpu.device(), self.gpu.queue(), [command_buffer])?;

        self.resources.clear();
        self.gpu.window().pre_present_notify();
        surface_texture.present();
        Ok(())
    }

    /// Render the current display list to an RGBA buffer.
    ///
    /// # Errors
    /// Returns an error if rendering or texture readback fails.
    ///
    /// # Panics
    /// Panics if pixel chunks are not exactly 4 bytes.
    pub fn render_to_rgba(&mut self) -> Result<Vec<u8>, AnyhowError> {
        self.resources.clear();

        let validation_scope = ErrorScopeGuard::push(self.gpu.device(), "pre-render-validation");
        validation_scope.check()?;

        self.offscreen
            .ensure_texture(self.gpu.device(), self.gpu.size(), self.gpu.render_format());
        self.render_to_offscreen()?;

        let mut copy_encoder =
            self.gpu
                .device()
                .create_command_encoder(&CommandEncoderDescriptor {
                    label: Some("texture-copy-encoder"),
                });

        let width = self.gpu.size().width.max(1);
        let height = self.gpu.size().height.max(1);
        let bpp = 4u32;
        let row_bytes = width * bpp;
        let padded_bpr = row_bytes.div_ceil(256) * 256;
        let buffer_size = u64::from(padded_bpr) * u64::from(height);

        self.offscreen
            .ensure_readback_buffer(self.gpu.device(), padded_bpr, buffer_size);
        self.offscreen
            .copy_to_readback(&mut copy_encoder, width, height, padded_bpr)?;

        let copy_command_buffer = {
            let scope = ErrorScopeGuard::push(self.gpu.device(), "copy_encoder.finish");
            let buffer = copy_encoder.finish();
            scope.check()?;
            buffer
        };
        submit_with_validation(self.gpu.device(), self.gpu.queue(), [copy_command_buffer])?;

        let mut out = self
            .offscreen
            .read_pixels(self.gpu.device(), (width, height, bpp, padded_bpr))?;
        self.convert_bgra_to_rgba(&mut out);
        Ok(out)
    }

    /// Record all draw passes (rectangles and text) for the current frame.
    ///
    /// # Errors
    /// Returns an error if rendering fails.
    fn record_draw_passes(
        &mut self,
        texture_view: &TextureView,
        encoder: &mut CommandEncoder,
        use_retained: bool,
    ) -> AnyResult<()> {
        let use_layers = !self.layers.is_empty();
        let is_offscreen = false;
        let main_load = LoadOp::Load;
        let text_load = LoadOp::Load;

        self.prepare_text_for_rendering(use_retained, use_layers);
        self.render_rectangles_pass(&mut RenderRectanglesParams {
            encoder,
            texture_view,
            use_retained,
            use_layers,
            is_offscreen,
            main_load,
        })?;

        let text_scope = ErrorScopeGuard::push(self.gpu.device(), "render-text-pass");
        self.render_text_pass(&mut RenderTextParams {
            encoder,
            texture_view,
            text_load,
            use_retained,
            use_layers,
        });
        if let Err(err) = text_scope.check() {
            error!(target: "wgpu_renderer", "render_text_pass error scope caught error: {err:?}");
            return Err(err);
        }

        Ok(())
    }

    /// Render rectangles pass with support for layers, retained, and immediate modes.
    ///
    /// # Errors
    /// Returns an error if rendering fails.
    fn render_rectangles_pass(&mut self, params: &mut RenderRectanglesParams<'_>) -> AnyResult<()> {
        if !params.is_offscreen {
            self.render_clear_pass(params.encoder, params.texture_view);
        }

        if params.use_layers {
            let mut components = render_orchestrator::RenderComponents {
                gpu: &self.gpu,
                pipelines: &self.pipelines,
                text: &mut self.text,
                rectangles: &mut self.rectangles,
                offscreen: &self.offscreen,
                resources: &mut self.resources,
            };
            render_orchestrator::render_layers_rectangles(
                params.encoder,
                params.texture_view,
                params.main_load,
                &self.layers,
                &mut components,
            );
        } else if params.use_retained {
            let mut components = render_orchestrator::RenderComponents {
                gpu: &self.gpu,
                pipelines: &self.pipelines,
                text: &mut self.text,
                rectangles: &mut self.rectangles,
                offscreen: &self.offscreen,
                resources: &mut self.resources,
            };
            render_orchestrator::render_retained_rectangles(
                params.encoder,
                params.texture_view,
                params.main_load,
                self.retained_display_list.as_ref(),
                &mut components,
            )?;
        } else {
            self.render_immediate_rectangles(
                params.encoder,
                params.texture_view,
                params.is_offscreen,
            );
        }
        Ok(())
    }

    /// Render immediate mode rectangles in a single batched draw call.
    fn render_immediate_rectangles(
        &mut self,
        encoder: &mut CommandEncoder,
        texture_view: &TextureView,
        is_offscreen: bool,
    ) {
        let mut vertices: Vec<Vertex> = Vec::with_capacity(self.display_list.len() * 6);
        for rect in &self.display_list {
            let rgba = [rect.color[0], rect.color[1], rect.color[2], 1.0];
            self.push_rect_vertices_ndc(
                &mut vertices,
                [rect.x, rect.y, rect.width, rect.height],
                rgba,
            );
        }
        let vertex_bytes = cast_slice(vertices.as_slice());
        let vertex_buffer = self
            .gpu
            .device()
            .create_buffer_init(&util::BufferInitDescriptor {
                label: Some("rect-vertices"),
                contents: vertex_bytes,
                usage: BufferUsages::VERTEX,
            });
        self.rectangles.set_vertex_buffer(vertex_buffer);
        self.rectangles.set_vertex_count(vertices.len() as u32);
        let immediate_load = if is_offscreen {
            LoadOp::Clear(Color::TRANSPARENT)
        } else {
            LoadOp::Load
        };
        self.render_immediate_pass(encoder, texture_view, immediate_load);
    }

    /// Render text pass for layer or retained mode.
    fn render_text_pass(&self, params: &mut RenderTextParams<'_>) {
        let mut pass = params.encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("text-pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: params.texture_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: params.text_load,
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        debug!(target: "wgpu_renderer", "start text-pass");
        pass.push_debug_group("text-pass");
        if params.use_layers {
            let batches = batch_layer_texts_with_scissor(
                &self.layers,
                self.gpu.size().width,
                self.gpu.size().height,
            );
            render_orchestrator::draw_text_batches(&mut pass, batches, &self.gpu, &self.text);
        } else if params.use_retained
            && let Some(display_list) = &self.retained_display_list
        {
            let batches = batch_texts_with_scissor(
                display_list,
                self.gpu.size().width,
                self.gpu.size().height,
            );
            render_orchestrator::draw_text_batches(&mut pass, batches, &self.gpu, &self.text);
        }
        pass.pop_debug_group();
        debug!(target: "wgpu_renderer", "end text-pass");
    }

    /// Render to offscreen texture.
    ///
    /// # Errors
    /// Returns an error if rendering or command submission fails.
    fn render_to_offscreen(&mut self) -> Result<(), AnyhowError> {
        let tmp_view = self.offscreen.get_texture(self.gpu.render_format())?;
        let mut encoder = self
            .gpu
            .device()
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("render-to-rgba"),
            });
        self.record_draw_passes(&tmp_view, &mut encoder, true)?;
        let command_buffer = encoder.finish();
        submit_with_validation(self.gpu.device(), self.gpu.queue(), [command_buffer])?;
        self.resources.clear();
        Ok(())
    }

    /// Convert BGRA to RGBA if needed.
    ///
    /// # Panics
    /// Panics if pixel chunks are not exactly 4 bytes.
    fn convert_bgra_to_rgba(&self, out: &mut [u8]) {
        match self.gpu.render_format() {
            TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => {
                for pixel in out.chunks_exact_mut(4) {
                    assert!(
                        pixel.len() > 2,
                        "pixel chunks from chunks_exact_mut(4) must have at least 3 elements"
                    );
                    let blue = pixel[0];
                    let red = pixel[2];
                    pixel[0] = red;
                    pixel[2] = blue;
                }
            }
            _ => {}
        }
    }

    /// Render clear pass for non-offscreen rendering.
    fn render_clear_pass(&self, encoder: &mut CommandEncoder, texture_view: &TextureView) {
        debug!(target: "wgpu_renderer", "start clear-pass");
        {
            let _clear_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("clear-pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: texture_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color {
                            r: f64::from(self.clear_color[0]),
                            g: f64::from(self.clear_color[1]),
                            b: f64::from(self.clear_color[2]),
                            a: f64::from(self.clear_color[3]),
                        }),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        debug!(target: "wgpu_renderer", "end clear-pass");
    }

    /// Helper method to render immediate mode pass.
    fn render_immediate_pass(
        &self,
        encoder: &mut CommandEncoder,
        texture_view: &TextureView,
        load_op: LoadOp<Color>,
    ) {
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("main-pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: texture_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: load_op,
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            debug!(target: "wgpu_renderer", "start main-pass");
            pass.push_debug_group("main-pass(immediate)");
            pass.set_pipeline(self.pipelines.main_pipeline());
            self.rectangles.render(&mut pass);
            pass.pop_debug_group();
            debug!(target: "wgpu_renderer", "end main-pass");
        };
    }

    /// Prepare text for rendering based on display mode.
    fn prepare_text_for_rendering(&mut self, use_retained: bool, use_layers: bool) {
        if use_retained {
            if let Some(display_list) = &self.retained_display_list {
                self.text_list = display_list
                    .items
                    .iter()
                    .filter_map(map_text_item)
                    .collect();
            }
            self.glyphon_prepare();
        } else if !use_layers {
            self.glyphon_prepare();
        }
    }

    /// Prepare glyphon buffers for the current text list.
    fn glyphon_prepare(&mut self) {
        let scale = self.gpu.window().scale_factor() as f32;
        self.text.prepare(
            self.gpu.device(),
            self.gpu.queue(),
            &self.text_list,
            (self.gpu.size(), scale),
        );
    }

    /// Push rectangle vertices in NDC coordinates.
    #[inline]
    fn push_rect_vertices_ndc(&self, out: &mut Vec<Vertex>, rect_xywh: [f32; 4], color: [f32; 4]) {
        let framebuffer_width = self.gpu.size().width.max(1) as f32;
        let framebuffer_height = self.gpu.size().height.max(1) as f32;
        let [rect_x, rect_y, rect_width, rect_height] = rect_xywh;
        if rect_width <= 0.0 || rect_height <= 0.0 {
            return;
        }
        let x0 = (rect_x / framebuffer_width).mul_add(2.0, -1.0);
        let x1 = ((rect_x + rect_width) / framebuffer_width).mul_add(2.0, -1.0);
        let y0 = (rect_y / framebuffer_height).mul_add(-2.0, 1.0);
        let y1 = ((rect_y + rect_height) / framebuffer_height).mul_add(-2.0, 1.0);
        out.extend_from_slice(&[
            Vertex {
                position: [x0, y0],
                color,
            },
            Vertex {
                position: [x1, y0],
                color,
            },
            Vertex {
                position: [x1, y1],
                color,
            },
            Vertex {
                position: [x0, y0],
                color,
            },
            Vertex {
                position: [x1, y1],
                color,
            },
            Vertex {
                position: [x0, y1],
                color,
            },
        ]);
    }
}

/// Pixel bounds (x, y, width, height)
pub(crate) type Bounds = (f32, f32, f32, f32);
