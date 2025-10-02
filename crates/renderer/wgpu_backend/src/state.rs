use crate::error::submit_with_validation;
use crate::pipelines::{Vertex, build_pipeline_and_buffers, build_texture_pipeline};
use crate::text::{
    TextBatch, batch_layer_texts_with_scissor, batch_texts_with_scissor, map_text_item,
};
use anyhow::{Error as AnyhowError, Result as AnyResult, anyhow};
use bytemuck::cast_slice;
use glyphon::{
    Attrs, Buffer as GlyphonBuffer, Cache, Color as GlyphonColor, FontSystem, Metrics, Resolution,
    Shaping, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use log::debug;
use pollster::block_on;
use renderer::compositor::OpacityCompositor;
use renderer::display_list::{
    DisplayItem, DisplayList, Scissor, StackingContextBoundary, batch_display_list,
};
use renderer::renderer::{DrawRect, DrawText};
use std::sync::Arc;
use std::sync::mpsc::channel;
use tracing::info_span;
use wgpu::util::DeviceExt as _;
use wgpu::{MultisampleState, PollType, *};
use winit::dpi::PhysicalSize;
use winit::window::Window;

// pollster is used via crate::error helpers.

/// Result type for offscreen rendering operations.
type OffscreenRenderResult = (Texture, TextureView, u32, u32, BindGroup);

/// Composite info for a pre-rendered opacity group.
/// Contains (`start_index`, `end_index`, `texture`, `texture_view`, `tex_w`, `tex_h`, `alpha`, `bounds`, `bind_group`).
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

/// Compact representation for preprocessed layer data: either no content
/// or (items, composites, excluded ranges).
pub(crate) type LayerEntry = Option<(Vec<DisplayItem>, Vec<OpacityComposite>, Vec<(usize, usize)>)>;

/// Type alias for scissor rectangle: (x, y, width, height).
type ScissorRect = Option<(u32, u32, u32, u32)>;

/// Parameters for offscreen rendering passes.
struct OffscreenRenderParams<'render> {
    /// Command encoder for recording render commands.
    encoder: &'render mut CommandEncoder,
    /// Texture view to render into.
    view: &'render TextureView,
    /// Display items translated to local coordinates.
    translated_items: &'render [DisplayItem],
    /// Texture width in pixels.
    tex_width: u32,
    /// Texture height in pixels.
    tex_height: u32,
    /// Render context with viewport information.
    ctx: RenderContext,
}

/// Parameters for retained pass rendering to reduce argument count.
struct RetainedPassParams<'items> {
    /// Display items to render.
    items: &'items [DisplayItem],
    /// Opacity composites to apply.
    comps: Vec<OpacityComposite>,
}

/// Parameters for rendering rectangles pass.
struct RenderRectanglesParams<'render_pass> {
    /// Command encoder for recording render commands.
    encoder: &'render_pass mut CommandEncoder,
    /// Texture view to render into.
    texture_view: &'render_pass TextureView,
    /// Whether to use retained mode rendering.
    use_retained: bool,
    /// Whether to use layered rendering.
    use_layers: bool,
    /// Whether this is an offscreen render.
    is_offscreen: bool,
    /// Load operation for the main pass.
    main_load: LoadOp<Color>,
}

/// Parameters for text rendering pass.
struct RenderTextParams<'text_pass> {
    /// Command encoder for recording render commands.
    encoder: &'text_pass mut CommandEncoder,
    /// Texture view to render into.
    texture_view: &'text_pass TextureView,
    /// Load operation for the text pass.
    text_load: LoadOp<Color>,
    /// Whether to use retained mode rendering.
    use_retained: bool,
    /// Whether to use layered rendering.
    use_layers: bool,
}

/// Vertex structure for texture quad rendering.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct TexVertex {
    /// Position in NDC coordinates.
    pos: [f32; 2],
    /// UV texture coordinates.
    tex_coords: [f32; 2],
}

/// Render performance metrics for observability and debugging.
#[derive(Debug, Clone, Default)]
pub struct RenderMetrics {
    /// Frame rendering time in milliseconds.
    pub frame_time_ms: f64,
    /// Number of draw calls issued.
    pub draw_calls: u32,
    /// Total vertices rendered in the frame.
    pub vertices_rendered: u32,
    /// Total texture memory used in bytes.
    pub texture_memory_bytes: u64,
    /// Number of errors encountered during rendering.
    pub error_count: u32,
    /// Number of opacity groups rendered.
    pub opacity_groups_rendered: u32,
}

/// Limits to prevent pathological content from exhausting resources.
#[derive(Debug, Clone)]
pub struct RenderLimits {
    /// Maximum number of display items allowed per frame.
    pub max_display_items: usize,
    /// Maximum texture dimension size in pixels.
    pub max_texture_size: u32,
    /// Maximum number of draw calls allowed per frame.
    pub max_draw_calls_per_frame: u32,
    /// Maximum depth of nested opacity groups.
    pub max_nested_opacity_groups: u32,
    /// Maximum total texture memory in bytes.
    pub max_texture_memory_bytes: u64,
}

impl Default for RenderLimits {
    fn default() -> Self {
        Self {
            max_display_items: 100_000,
            max_texture_size: 8192,
            max_draw_calls_per_frame: 10_000,
            max_nested_opacity_groups: 32,
            max_texture_memory_bytes: 512 * 1024 * 1024, // 512 MB
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

/// RAII guard for WGPU error scopes.
/// Automatically pushes an error scope on creation and pops it on drop.
/// CRITICAL: Must call `check()` before dropping to avoid error scope imbalance.
pub(crate) struct ErrorScopeGuard {
    /// GPU device for error scope management.
    device: Arc<Device>,
    /// Label for debugging error scopes.
    label: &'static str,
    /// Whether `check()` has been called.
    checked: bool,
}

impl ErrorScopeGuard {
    /// Push an error scope and return a guard that will pop it on drop.
    pub(crate) fn push(device: &Arc<Device>, label: &'static str) -> Self {
        device.push_error_scope(ErrorFilter::Validation);
        Self {
            device: Arc::clone(device),
            label,
            checked: false,
        }
    }

    /// Check for errors in this scope. Must be called before dropping.
    /// This is now foolproof - it sets the checked flag and delegates to `do_check`.
    ///
    /// # Errors
    /// Returns an error if a WGPU validation error is detected.
    pub(crate) fn check(mut self) -> AnyResult<()> {
        self.checked = true;
        self.do_check()
    }

    /// Check for WGPU errors by polling the error scope.
    ///
    /// # Errors
    /// Returns an error if a WGPU validation error is detected.
    fn do_check(&self) -> AnyResult<()> {
        let fut = self.device.pop_error_scope();
        let res = block_on(fut);
        if let Some(err) = res {
            log::error!(target: "wgpu_renderer", "WGPU uncaptured error: {err:?}");
            return Err(anyhow!(
                "wgpu validation error in scope '{}': {err:?}",
                self.label
            ));
        }
        Ok(())
    }
}

impl Drop for ErrorScopeGuard {
    fn drop(&mut self) {
        if !self.checked {
            // CRITICAL: If check() wasn't called, this is a bug that will cause error scope imbalance
            log::error!(
                target: "wgpu_renderer",
                "ErrorScopeGuard '{}' dropped without calling check() - this will cause error scope imbalance!",
                self.label
            );
            // Pop the scope anyway to prevent imbalance, but log the error
            if let Err(error) = self.do_check() {
                log::error!(target: "wgpu_renderer", "Unchecked error in scope '{}': {error:?}", self.label);
            }
        }
    }
}

/// Rendering context that encapsulates viewport and size information.
/// This is passed as a parameter instead of mutating shared state.
#[derive(Debug, Copy, Clone)]
struct RenderContext {
    /// The viewport size for rendering.
    viewport_size: PhysicalSize<u32>,
}

impl RenderContext {
    /// Create a new render context with the given size.
    const fn new(size: PhysicalSize<u32>) -> Self {
        Self {
            viewport_size: size,
        }
    }

    /// Get the viewport width (minimum 1).
    fn width(self) -> u32 {
        self.viewport_size.width.max(1)
    }

    /// Get the viewport height (minimum 1).
    fn height(self) -> u32 {
        self.viewport_size.height.max(1)
    }
}

#[derive(Debug, Clone)]
pub enum Layer {
    Background,
    Content(DisplayList),
    Chrome(DisplayList),
}

/// `RenderState` owns the GPU device/surface and a minimal pipeline to draw rectangles from layout.
pub struct RenderState {
    /// Window handle for the render target.
    window: Arc<Window>,
    /// GPU device for creating resources.
    device: Arc<Device>,
    /// Command queue for submitting work to the GPU.
    queue: Queue,
    /// Current framebuffer size.
    size: PhysicalSize<u32>,
    /// Optional surface for presenting to the window.
    surface: Option<Surface<'static>>,
    /// Surface texture format.
    surface_format: TextureFormat,
    /// Render target format.
    render_format: TextureFormat,
    /// Main rendering pipeline for rectangles.
    pipeline: RenderPipeline,
    /// Textured quad rendering pipeline.
    tex_pipeline: RenderPipeline,
    /// Bind group layout for textured quads.
    tex_bind_layout: BindGroupLayout,
    /// Linear sampler for texture sampling.
    linear_sampler: Sampler,
    /// Vertex buffer for rendering.
    vertex_buffer: Buffer,
    /// Number of vertices in the vertex buffer.
    vertex_count: u32,
    /// Display list of rectangles to render.
    display_list: Vec<DrawRect>,
    /// Display list of text items to render.
    text_list: Vec<DrawText>,
    /// Retained display list for Phase 6. When set via `set_retained_display_list`,
    /// it becomes the source of truth and is flattened into the immediate lists.
    retained_display_list: Option<DisplayList>,
    // Glyphon text rendering state
    /// Glyphon font system for text rendering.
    font_system: FontSystem,
    /// Glyphon swash cache for glyph rasterization.
    swash_cache: SwashCache,
    /// Glyphon text atlas for caching rendered glyphs.
    #[allow(dead_code, reason = "Used by glyphon text rendering system")]
    text_atlas: TextAtlas,
    /// Glyphon text renderer.
    text_renderer: TextRenderer,
    /// Glyphon cache for text layout.
    #[allow(dead_code, reason = "Cache maintained for glyphon state management")]
    glyphon_cache: Cache,
    /// Glyphon viewport for text rendering.
    viewport: Viewport,
    /// Optional layers for multi-DL compositing; when non-empty, `render()` draws these instead of the single retained list.
    layers: Vec<Layer>,
    /// Clear color for the framebuffer (canvas background). RGBA in [0,1].
    clear_color: [f32; 4],
    /// Persistent offscreen render target for readback-based renders
    offscreen_tex: Option<Texture>,
    /// Persistent readback buffer sized for current framebuffer (padded bytes-per-row)
    readback_buf: Option<Buffer>,
    /// Padded bytes per row for readback buffer.
    readback_padded_bpr: u32,
    /// Total size of readback buffer in bytes.
    readback_size: u64,
    /// Keep GPU resources alive until after submission to avoid encoder invalidation at finish.
    live_textures: Vec<Texture>,
    /// Keep transient GPU buffers (vertex/uniform) alive through submission.
    live_buffers: Vec<Buffer>,
}

/// Surface configuration result: (surface, `surface_format`, `render_format`).
type SurfaceConfig = (Option<Surface<'static>>, TextureFormat, TextureFormat);

/// Glyphon rendering resources for initialization.
struct GlyphonResources {
    /// Font system for text rendering.
    font_system: FontSystem,
    /// Text atlas for glyph caching.
    text_atlas: TextAtlas,
    /// Text renderer for drawing.
    text_renderer: TextRenderer,
    /// Glyphon cache.
    glyphon_cache: Cache,
    /// Viewport for coordinate transformation.
    viewport: Viewport,
}

impl RenderState {
    /// Preprocess a layer with the given encoder to collect opacity composites.
    ///
    /// # Errors
    /// Returns an error if opacity composite collection fails.
    #[inline]
    fn preprocess_layer_with_encoder(
        &mut self,
        encoder: &mut CommandEncoder,
        layer: &Layer,
    ) -> AnyResult<LayerEntry> {
        match layer {
            Layer::Background => Ok(None),
            Layer::Content(display_list) | Layer::Chrome(display_list) => {
                let items: Vec<DisplayItem> = display_list.items.clone();
                let comps = self.collect_opacity_composites(encoder, &items)?;
                let ranges = Self::build_exclude_ranges(&comps);
                Ok(Some((items, comps, ranges)))
            }
        }
    }
    /// Collect opacity composites from display items for offscreen rendering.
    ///
    /// # Errors
    /// Returns an error if offscreen rendering fails.
    #[inline]
    fn collect_opacity_composites(
        &mut self,
        encoder: &mut CommandEncoder,
        items: &[DisplayItem],
    ) -> AnyResult<Vec<OpacityComposite>> {
        let mut out: Vec<OpacityComposite> = Vec::new();
        let mut index = 0usize;
        while index < items.len() {
            if let DisplayItem::BeginStackingContext { boundary } = &items[index]
                && matches!(boundary, StackingContextBoundary::Opacity { alpha } if *alpha < 1.0)
            {
                let end = OpacityCompositor::find_stacking_context_end(items, index + 1);
                let group_items = &items[index + 1..end];
                let alpha = match boundary {
                    StackingContextBoundary::Opacity { alpha } => *alpha,
                    _ => 1.0,
                };
                let bounds = OpacityCompositor::compute_items_bounds(group_items)
                    .unwrap_or((0.0, 0.0, 1.0, 1.0));
                let (tex, view, tex_width, tex_height, bind_group) = self
                    .render_items_to_offscreen_bounded_with_bind_group(
                        encoder,
                        group_items,
                        bounds,
                        alpha,
                    )?;
                // Offscreen render pass is complete. Texture will be ready to sample after
                // the encoder is submitted (done by caller after all opacity groups are collected).
                out.push((
                    index, end, tex, view, tex_width, tex_height, alpha, bounds, bind_group,
                ));
                index = end + 1;
                continue;
            }
            index += 1;
        }
        Ok(out)
    }

    /// Draw display items excluding specified ranges (used for opacity groups).
    #[inline]
    fn draw_items_excluding_ranges(
        &mut self,
        pass: &mut RenderPass<'_>,
        items: &[DisplayItem],
        exclude: &[(usize, usize)],
    ) {
        let mut index = 0usize;
        let mut ex_idx = 0usize;
        while index < items.len() {
            if ex_idx < exclude.len() && index == exclude[ex_idx].0 {
                index = exclude[ex_idx].1 + 1;
                ex_idx += 1;
                continue;
            }
            let next = exclude.get(ex_idx).map_or(items.len(), |range| range.0);
            if index < next {
                // Use draw_items_with_groups to properly handle stacking contexts
                drop(self.draw_items_with_groups(pass, &items[index..next]));
                index = next;
            }
        }
    }

    /// Build exclude ranges from opacity composites for rendering.
    #[inline]
    fn build_exclude_ranges(comps: &[OpacityComposite]) -> Vec<(usize, usize)> {
        let mut ranges = Vec::with_capacity(comps.len());
        for (start, end, ..) in comps {
            ranges.push((*start, *end));
        }
        ranges
    }

    /// Composite opacity groups by drawing textured quads with bind groups.
    #[inline]
    fn composite_groups(&mut self, pass: &mut RenderPass<'_>, comps: Vec<OpacityComposite>) {
        for (_s, _e, tex, _view, _tw, _th, _alpha, bounds, bind_group) in comps {
            // Keep the texture alive until after submit; otherwise some backends invalidate at finish.
            self.live_textures.push(tex);
            self.draw_texture_quad_with_bind_group(pass, &bind_group, bounds);
        }
    }

    /// Draw an opacity group with the specified alpha value.
    ///
    /// # Errors
    /// Returns an error if rendering fails.
    #[inline]
    fn draw_opacity_group(
        &mut self,
        pass: &mut RenderPass<'_>,
        group_items: &[DisplayItem],
        alpha: f32,
    ) -> AnyResult<()> {
        // Offscreen opacity compositing is handled by record_draw_passes() using pre-collected
        // composites before opening the main pass. For any other contexts (including offscreen
        // renders), simply draw the group's items directly here.
        // Note: alpha parameter is reserved for future use or higher-level compositing
        // Use draw_items_with_groups to handle nested stacking contexts
        let _: f32 = alpha; // Reserved for future opacity compositing
        self.draw_items_with_groups(pass, group_items)
    }

    /// Helper method to render layers pass, extracted to reduce nesting.
    fn render_layers_pass(
        &mut self,
        encoder: &mut CommandEncoder,
        texture_view: &TextureView,
        main_load: LoadOp<Color>,
        per_layer: Vec<LayerEntry>,
    ) {
        // Simplified: no error scope needed here, errors caught at submission
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("main-pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: texture_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: main_load,
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            debug!(target: "wgpu_renderer", "start main-pass");
            pass.push_debug_group("main-pass(layers)");
            pass.set_pipeline(&self.pipeline);

            for (items, comps, ranges) in per_layer.into_iter().flatten() {
                self.draw_items_excluding_ranges(&mut pass, &items, &ranges);
                self.composite_groups(&mut pass, comps);
            }
            pass.pop_debug_group();
            debug!(target: "wgpu_renderer", "end main-pass");
        };
    }

    /// Helper method to render retained display list pass, extracted to reduce nesting.
    fn render_retained_pass(
        &mut self,
        encoder: &mut CommandEncoder,
        texture_view: &TextureView,
        main_load: LoadOp<Color>,
        params: RetainedPassParams,
    ) {
        let RetainedPassParams { items, comps } = params;
        log::debug!(target: "wgpu_renderer", "=== CREATING MAIN RENDER PASS (retained) ===");
        log::debug!(target: "wgpu_renderer", "    Composites to apply: {}", comps.len());
        log::debug!(target: "wgpu_renderer", "    Texture view: {texture_view:?}");
        log::debug!(target: "wgpu_renderer", "    Load op: {main_load:?}");
        // Simplified: no error scope needed here, errors caught at submission
        {
            log::debug!(target: "wgpu_renderer", "    About to call begin_render_pass(main-pass)...");
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("main-pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: texture_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: main_load,
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            debug!(target: "wgpu_renderer", "start main-pass");
            pass.push_debug_group("main-pass(retained)");
            pass.set_pipeline(&self.pipeline);

            let ranges = Self::build_exclude_ranges(&comps);
            self.draw_items_excluding_ranges(&mut pass, items, &ranges);
            self.composite_groups(&mut pass, comps);
            pass.pop_debug_group();
            debug!(target: "wgpu_renderer", "end main-pass");
        };
    }

    /// Helper method to render immediate mode pass, extracted to reduce nesting.
    fn render_immediate_pass(
        &self,
        encoder: &mut CommandEncoder,
        texture_view: &TextureView,
        load_op: LoadOp<Color>,
    ) {
        // Simplified: no error scope needed here, errors caught at submission
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
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            if self.vertex_count > 0 {
                pass.draw(0..self.vertex_count, 0..1);
            }
            pass.pop_debug_group();
            debug!(target: "wgpu_renderer", "end main-pass");
            // Explicitly drop the pass to ensure encoder is not borrowed
            drop(pass);
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

    /// Record all render passes (rectangles + text) into the provided encoder.
    ///
    /// Opacity compositing is fully functional using a hybrid approach:
    /// - All offscreen passes are grouped in one encoder (minimal overhead)
    /// - After offscreen rendering, we submit and create a new encoder
    /// - This explicit submission ensures D3D12 resource state transitions (`RENDER_TARGET` → `PIXEL_SHADER_RESOURCE`)
    /// - Main pass uses the new encoder to sample the offscreen textures
    ///
    /// # Errors
    /// Returns an error if rendering or opacity group processing fails.
    /// Render rectangles pass with layer, retained, or immediate mode.
    ///
    /// # Errors
    /// Returns an error if rendering fails.
    fn render_rectangles_pass(&mut self, params: &mut RenderRectanglesParams<'_>) -> AnyResult<()> {
        if !params.is_offscreen {
            self.render_clear_pass(params.encoder, params.texture_view);
        }

        if params.use_layers {
            self.render_layers_rectangles(params.encoder, params.texture_view, params.main_load)?;
        } else if params.use_retained {
            self.render_retained_rectangles(params.encoder, params.texture_view, params.main_load)?;
        } else {
            self.render_immediate_rectangles(
                params.encoder,
                params.texture_view,
                params.is_offscreen,
            );
        }
        Ok(())
    }

    /// Render layers rectangles with opacity compositing.
    ///
    /// # Errors
    /// Returns an error if layer processing or rendering fails.
    fn render_layers_rectangles(
        &mut self,
        encoder: &mut CommandEncoder,
        texture_view: &TextureView,
        main_load: LoadOp<Color>,
    ) -> AnyResult<()> {
        let per_layer: Vec<LayerEntry> = self
            .layers
            .clone()
            .iter()
            .map(|layer| self.preprocess_layer_with_encoder(encoder, layer))
            .collect::<AnyResult<Vec<_>>>()?;

        let has_opacity = per_layer
            .iter()
            .any(|entry| matches!(entry, Some((_, comps, _)) if !comps.is_empty()));
        if has_opacity {
            log::debug!(target: "wgpu_renderer", ">>> Collected layer opacity groups (no mid-frame submission)");
        }

        self.render_layers_pass(encoder, texture_view, main_load, per_layer);
        Ok(())
    }

    /// Render retained display list rectangles with opacity compositing.
    ///
    /// # Errors
    /// Returns an error if rendering or opacity composite collection fails.
    fn render_retained_rectangles(
        &mut self,
        encoder: &mut CommandEncoder,
        texture_view: &TextureView,
        main_load: LoadOp<Color>,
    ) -> AnyResult<()> {
        let Some(display_list) = self.retained_display_list.clone() else {
            return Ok(());
        };
        let items: Vec<DisplayItem> = display_list.items;
        let comps = self.collect_opacity_composites(encoder, &items)?;

        log::debug!(target: "wgpu_renderer", ">>> Collected {} opacity groups (no mid-frame submission)", comps.len());

        self.render_retained_pass(
            encoder,
            texture_view,
            main_load,
            RetainedPassParams {
                items: &items,
                comps,
            },
        );
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
        let vertex_buffer = self.device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("rect-vertices"),
            contents: vertex_bytes,
            usage: BufferUsages::VERTEX,
        });
        self.vertex_buffer = vertex_buffer;
        self.vertex_count = vertices.len() as u32;
        let immediate_load = if is_offscreen {
            LoadOp::Clear(Color::TRANSPARENT)
        } else {
            LoadOp::Load
        };
        self.render_immediate_pass(encoder, texture_view, immediate_load);
    }

    /// Render text pass for layer or retained mode.
    fn render_text_pass(&mut self, params: &mut RenderTextParams<'_>) {
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
            let batches =
                batch_layer_texts_with_scissor(&self.layers, self.size.width, self.size.height);
            self.draw_text_batches(&mut pass, batches);
        } else if params.use_retained
            && let Some(display_list) = &self.retained_display_list
        {
            let batches = batch_texts_with_scissor(display_list, self.size.width, self.size.height);
            self.draw_text_batches(&mut pass, batches);
        }
        pass.pop_debug_group();
        debug!(target: "wgpu_renderer", "end text-pass");
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
        self.render_text_pass(&mut RenderTextParams {
            encoder,
            texture_view,
            text_load,
            use_retained,
            use_layers,
        });

        Ok(())
    }

    /// Push rectangle vertices in NDC coordinates to the vertex buffer.
    #[inline]
    fn push_rect_vertices_ndc(&self, out: &mut Vec<Vertex>, rect_xywh: [f32; 4], color: [f32; 4]) {
        let framebuffer_width = self.size.width.max(1) as f32;
        let framebuffer_height = self.size.height.max(1) as f32;
        let [rect_x, rect_y, rect_width, rect_height] = rect_xywh;
        if rect_width <= 0.0 || rect_height <= 0.0 {
            return;
        }
        let x0 = (rect_x / framebuffer_width).mul_add(2.0, -1.0);
        let x1 = ((rect_x + rect_width) / framebuffer_width).mul_add(2.0, -1.0);
        let y0 = (rect_y / framebuffer_height).mul_add(-2.0, 1.0);
        let y1 = ((rect_y + rect_height) / framebuffer_height).mul_add(-2.0, 1.0);
        // Pass through color; shader handles sRGB->linear conversion for blending.
        let vertex_color = color;
        out.extend_from_slice(&[
            Vertex {
                position: [x0, y0],
                color: vertex_color,
            },
            Vertex {
                position: [x1, y0],
                color: vertex_color,
            },
            Vertex {
                position: [x1, y1],
                color: vertex_color,
            },
            Vertex {
                position: [x0, y0],
                color: vertex_color,
            },
            Vertex {
                position: [x1, y1],
                color: vertex_color,
            },
            Vertex {
                position: [x0, y1],
                color: vertex_color,
            },
        ]);
    }

    /// Draw display items with proper stacking context handling
    /// Spec: CSS 2.2 §9.9.1 - Stacking contexts and paint order
    ///
    /// # Errors
    /// Returns an error if rendering or opacity group processing fails.
    fn draw_items_with_groups(
        &mut self,
        pass: &mut RenderPass<'_>,
        items: &[DisplayItem],
    ) -> AnyResult<()> {
        let mut index = 0usize;

        while index < items.len() {
            match &items[index] {
                DisplayItem::BeginStackingContext { boundary } => {
                    // Find the matching end boundary
                    let end = OpacityCompositor::find_stacking_context_end(items, index + 1);
                    let group_items = &items[index + 1..end];

                    match boundary {
                        StackingContextBoundary::Opacity { alpha } if *alpha < 1.0 => {
                            self.draw_opacity_group(pass, group_items, *alpha)?;
                        }
                        _ => {
                            // Other stacking contexts (transforms, filters, z-index, etc.) - render normally
                            // but recursively handle any nested stacking contexts
                            // TODO: Implement transform matrices and filter effects
                            self.draw_items_with_groups(pass, group_items)?;
                        }
                    }

                    index = end + 1; // Skip to after EndStackingContext
                }
                DisplayItem::EndStackingContext => {
                    // This should be handled by the BeginStackingContext case
                    index += 1;
                }
                _ => {
                    // Regular display item - find the next stacking context boundary
                    let start = index;
                    let mut end = index;
                    while end < items.len() {
                        match &items[end] {
                            DisplayItem::BeginStackingContext { .. } => break,
                            _ => end += 1,
                        }
                    }

                    if start < end {
                        self.draw_items_batched(pass, &items[start..end]);
                    }
                    index = end;
                }
            }
        }
        Ok(())
    }

    /// Context-aware version of `draw_items_with_groups` that uses `RenderContext` instead of self.size.
    /// This prevents state corruption when rendering to different-sized targets.
    ///
    /// # Errors
    /// Returns an error if rendering fails.
    fn draw_items_with_groups_ctx(
        &mut self,
        pass: &mut RenderPass<'_>,
        items: &[DisplayItem],
        ctx: RenderContext,
    ) -> AnyResult<()> {
        // Temporarily change size, ensuring it's restored even on error
        let old_size = self.size;
        self.size = PhysicalSize::new(ctx.width(), ctx.height());
        let result = self.draw_items_with_groups(pass, items);
        self.size = old_size;
        result
    }

    /// Context-aware version of `draw_text_batch` that uses `RenderContext`.
    fn draw_text_batch_ctx(
        &mut self,
        pass: &mut RenderPass<'_>,
        text_items: &[DrawText],
        scissor: ScissorRect,
        ctx: RenderContext,
    ) {
        // Temporarily change size, ensuring it's restored even on error
        let old_size = self.size;
        self.size = PhysicalSize::new(ctx.width(), ctx.height());
        self.draw_text_batch(pass, text_items, scissor);
        self.size = old_size;
    }

    /// Draw display items in batches for efficient rendering.
    #[inline]
    fn draw_items_batched(&mut self, pass: &mut RenderPass<'_>, items: &[DisplayItem]) {
        let sub = DisplayList::from_items(items.to_vec());
        let batches = batch_display_list(&sub, self.size.width, self.size.height);
        for batch in batches {
            let mut vertices: Vec<Vertex> = Vec::with_capacity(batch.quads.len() * 6);
            for quad in &batch.quads {
                self.push_rect_vertices_ndc(
                    &mut vertices,
                    [quad.x, quad.y, quad.width, quad.height],
                    quad.color,
                );
            }
            let vertex_bytes = cast_slice(vertices.as_slice());
            let vertex_buffer = self.device.create_buffer_init(&util::BufferInitDescriptor {
                label: Some("layer-rect-batch"),
                contents: vertex_bytes,
                usage: BufferUsages::VERTEX,
            });
            // Keep buffer alive until submission to avoid backend lifetime edge-cases
            self.live_buffers.push(vertex_buffer.clone());
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            match batch.scissor {
                Some((scissor_x, scissor_y, scissor_width, scissor_height)) => {
                    let framebuffer_width = self.size.width.max(1);
                    let framebuffer_height = self.size.height.max(1);
                    let rect_x = scissor_x.min(framebuffer_width);
                    let rect_y = scissor_y.min(framebuffer_height);
                    let rect_width = scissor_width.min(framebuffer_width.saturating_sub(rect_x));
                    let rect_height = scissor_height.min(framebuffer_height.saturating_sub(rect_y));
                    if rect_width == 0 || rect_height == 0 {
                        // Nothing visible; skip draw for this batch
                        continue;
                    }
                    pass.set_scissor_rect(rect_x, rect_y, rect_width, rect_height);
                }
                None => {
                    pass.set_scissor_rect(0, 0, self.size.width.max(1), self.size.height.max(1));
                }
            }
            if !vertices.is_empty() {
                // Draw if we generated any geometry
                pass.draw(0..(vertices.len() as u32), 0..1);
            }
        }
    }

    /// Create offscreen texture for opacity compositing.
    fn create_offscreen_texture(&self, tex_width: u32, tex_height: u32) -> Texture {
        let offscreen_format = self.render_format;
        self.device.create_texture(&TextureDescriptor {
            label: Some("offscreen-opacity-texture"),
            size: Extent3d {
                width: tex_width,
                height: tex_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: offscreen_format,
            usage: TextureUsages::RENDER_ATTACHMENT
                | TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    }

    /// Translate display items to texture-local coordinates.
    fn translate_items_to_local(
        items: &[DisplayItem],
        offset_x: f32,
        offset_y: f32,
    ) -> Vec<DisplayItem> {
        items
            .iter()
            .map(|item| match item {
                DisplayItem::Rect {
                    x: rect_x,
                    y: rect_y,
                    width: rect_width,
                    height: rect_height,
                    color,
                } => DisplayItem::Rect {
                    x: rect_x - offset_x,
                    y: rect_y - offset_y,
                    width: *rect_width,
                    height: *rect_height,
                    color: *color,
                },
                DisplayItem::Text {
                    x: text_x,
                    y: text_y,
                    text,
                    color,
                    font_size,
                    bounds: text_bounds,
                } => DisplayItem::Text {
                    x: text_x - offset_x,
                    y: text_y - offset_y,
                    text: text.clone(),
                    color: *color,
                    font_size: *font_size,
                    bounds: text_bounds.map(|(left, top, right, bottom)| {
                        (
                            (left as f32 - offset_x) as i32,
                            (top as f32 - offset_y) as i32,
                            (right as f32 - offset_x) as i32,
                            (bottom as f32 - offset_y) as i32,
                        )
                    }),
                },
                other => other.clone(),
            })
            .collect()
    }

    /// Render items to offscreen texture with tight bounds and create bind group.
    /// Uses RAII guards to ensure state is always properly restored.
    ///
    /// HYBRID ARCHITECTURE: Multiple offscreen passes use the same encoder for efficiency.
    /// After ALL offscreen rendering completes, the caller must submit the encoder before
    /// opening the main pass. This explicit submission ensures D3D12 resource state transitions.
    ///
    /// Returns (texture, view, width, height, `bind_group`).
    ///
    /// # Errors
    /// Returns an error if offscreen rendering or bind group creation fails.
    /// Render rectangles to offscreen texture.
    ///
    /// # Errors
    /// Returns an error if rendering fails.
    fn render_offscreen_rects_pass(
        &mut self,
        params: &mut OffscreenRenderParams<'_>,
    ) -> AnyResult<()> {
        log::debug!(target: "wgpu_renderer", ">>> CREATING offscreen rects pass");
        let mut pass = params.encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("opacity-offscreen-pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: params.view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::TRANSPARENT),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        log::debug!(target: "wgpu_renderer", "    Pass created, setting viewport and pipeline");
        pass.set_viewport(
            0.0,
            0.0,
            params.tex_width as f32,
            params.tex_height as f32,
            0.0,
            1.0,
        );
        pass.set_pipeline(&self.pipeline);
        log::debug!(target: "wgpu_renderer", "    Drawing items");
        self.draw_items_with_groups_ctx(&mut pass, params.translated_items, params.ctx)?;
        log::debug!(target: "wgpu_renderer", "<<< Pass DROPPED");
        Ok(())
    }

    /// Render text to offscreen texture.
    fn render_offscreen_text_pass(&mut self, params: &mut OffscreenRenderParams<'_>) {
        let text_items: Vec<DrawText> = params
            .translated_items
            .iter()
            .filter_map(map_text_item)
            .collect();
        if text_items.is_empty() {
            return;
        }

        log::debug!(target: "wgpu_renderer", ">>> CREATING offscreen text pass");
        self.glyphon_prepare_for(text_items.as_slice());
        let mut text_pass = params.encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("opacity-offscreen-text-pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: params.view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Load,
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        log::debug!(target: "wgpu_renderer", "    Pass created, drawing text");
        text_pass.set_viewport(
            0.0,
            0.0,
            params.tex_width as f32,
            params.tex_height as f32,
            0.0,
            1.0,
        );
        self.draw_text_batch_ctx(&mut text_pass, text_items.as_slice(), None, params.ctx);
        log::debug!(target: "wgpu_renderer", "<<< Text pass DROPPED");
    }

    /// Create bind group for opacity compositing with alpha blending.
    fn create_opacity_bind_group(&mut self, view: &TextureView, alpha: f32) -> BindGroup {
        let alpha_buf = self.device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("opacity-alpha"),
            contents: cast_slice(&[alpha, 0.0f32, 0.0f32, 0.0f32]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });
        self.live_buffers.push(alpha_buf.clone());

        self.device.create_bind_group(&BindGroupDescriptor {
            label: Some("opacity-tex-bind"),
            layout: &self.tex_bind_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&self.linear_sampler),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: alpha_buf.as_entire_binding(),
                },
            ],
        })
    }

    /// Render items to offscreen texture with bind group for opacity compositing.
    ///
    /// # Errors
    /// Returns an error if rendering fails.
    fn render_items_to_offscreen_bounded_with_bind_group(
        &mut self,
        encoder: &mut CommandEncoder,
        items: &[DisplayItem],
        bounds: (f32, f32, f32, f32),
        alpha: f32,
    ) -> AnyResult<OffscreenRenderResult> {
        let (x, y, width, height) = bounds;
        let tex_width = (width.ceil() as u32).max(1);
        let tex_height = (height.ceil() as u32).max(1);

        log::debug!(target: "wgpu_renderer", "render_items_to_offscreen_bounded: bounds=({}, {}, {}, {}), tex_size={}x{}, items={}",
            x, y, width, height, tex_width, tex_height, items.len());

        let texture = self.create_offscreen_texture(tex_width, tex_height);
        let view = texture.create_view(&TextureViewDescriptor {
            label: Some("offscreen-opacity-view"),
            format: Some(self.render_format),
            ..Default::default()
        });

        let ctx = RenderContext::new(PhysicalSize::new(tex_width, tex_height));
        let translated_items = Self::translate_items_to_local(items, x, y);

        self.render_offscreen_rects_pass(&mut OffscreenRenderParams {
            encoder,
            view: &view,
            translated_items: &translated_items,
            tex_width,
            tex_height,
            ctx,
        })?;
        self.render_offscreen_text_pass(&mut OffscreenRenderParams {
            encoder,
            view: &view,
            translated_items: &translated_items,
            tex_width,
            tex_height,
            ctx,
        });

        log::debug!(target: "wgpu_renderer", "Offscreen render passes complete, creating bind group");
        let bind_group = self.create_opacity_bind_group(&view, alpha);
        log::debug!(target: "wgpu_renderer", "Bind group created, texture ready for compositing");

        Ok((texture, view, tex_width, tex_height, bind_group))
    }

    /// Draw a textured quad using a pre-created bind group (called from within render pass)
    fn draw_texture_quad_with_bind_group(
        &mut self,
        pass: &mut RenderPass<'_>,
        bind_group: &BindGroup,
        bounds: Bounds, // x, y, w, h in px
    ) {
        let (rect_x, rect_y, rect_width, rect_height) = bounds;
        log::debug!(target: "wgpu_renderer", ">>> draw_texture_quad_with_bind_group: bounds=({rect_x}, {rect_y}, {rect_width}, {rect_height})");

        // Build a quad covering the group's bounds with UVs 0..1 over the offscreen texture
        let framebuffer_width = self.size.width.max(1) as f32;
        let framebuffer_height = self.size.height.max(1) as f32;
        let x0 = (rect_x / framebuffer_width).mul_add(2.0, -1.0);
        let x1 = ((rect_x + rect_width) / framebuffer_width).mul_add(2.0, -1.0);
        let y0 = (rect_y / framebuffer_height).mul_add(-2.0, 1.0);
        let y1 = ((rect_y + rect_height) / framebuffer_height).mul_add(-2.0, 1.0);
        // UVs cover the full offscreen texture [0,1]
        let uv_left = 0.0;
        let uv_top = 0.0;
        let uv_right = 1.0;
        let uv_bottom = 1.0;
        let verts = [
            TexVertex {
                pos: [x0, y0],
                tex_coords: [uv_left, uv_bottom],
            },
            TexVertex {
                pos: [x1, y0],
                tex_coords: [uv_right, uv_bottom],
            },
            TexVertex {
                pos: [x1, y1],
                tex_coords: [uv_right, uv_top],
            },
            TexVertex {
                pos: [x0, y0],
                tex_coords: [uv_left, uv_bottom],
            },
            TexVertex {
                pos: [x1, y1],
                tex_coords: [uv_right, uv_top],
            },
            TexVertex {
                pos: [x0, y1],
                tex_coords: [uv_left, uv_top],
            },
        ];
        let vertex_buffer = self.device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("opacity-quad-vertices"),
            contents: cast_slice(&verts),
            usage: BufferUsages::VERTEX,
        });
        self.live_buffers.push(vertex_buffer.clone());

        // Constrain drawing to the group's bounds to avoid edge bleed and match compositing region.
        pass.set_pipeline(&self.tex_pipeline);

        let scissor_x = rect_x.max(0.0).floor() as u32;
        let scissor_y = rect_y.max(0.0).floor() as u32;
        let scissor_width = rect_width.max(0.0).ceil() as u32;
        let scissor_height = rect_height.max(0.0).ceil() as u32;

        let framebuffer_width_u32 = self.size.width.max(1);
        let framebuffer_height_u32 = self.size.height.max(1);
        let final_x = scissor_x.min(framebuffer_width_u32);
        let final_y = scissor_y.min(framebuffer_height_u32);
        let final_width = scissor_width.min(framebuffer_width_u32.saturating_sub(final_x));
        let final_height = scissor_height.min(framebuffer_height_u32.saturating_sub(final_y));
        if final_width == 0 || final_height == 0 {
            // Nothing visible; skip draw for this batch
            return;
        }
        pass.set_scissor_rect(final_x, final_y, final_width, final_height);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..6, 0..1);
    }

    /// Compute the bounding box of display items for opacity group sizing.
    #[inline]
    #[allow(
        dead_code,
        reason = "Utility function for future opacity bounds calculation"
    )]
    fn compute_items_bounds(items: &[DisplayItem]) -> Option<Bounds> {
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for item in items {
            match item {
                DisplayItem::Rect {
                    x,
                    y,
                    width,
                    height,
                    color: [_, _, _, _],
                } if *width > 0.0 && *height > 0.0 => {
                    min_x = min_x.min(*x);
                    min_y = min_y.min(*y);
                    max_x = max_x.max(x + width);
                    max_y = max_y.max(y + height);
                }
                DisplayItem::Text {
                    x,
                    y,
                    font_size,
                    bounds,
                    ..
                } => {
                    if let Some((left, top, right, bottom)) = bounds {
                        let left_x = *left as f32;
                        let top_y = *top as f32;
                        let right_x = *right as f32;
                        let bottom_y = *bottom as f32;
                        min_x = min_x.min(left_x);
                        min_y = min_y.min(top_y);
                        max_x = max_x.max(right_x);
                        max_y = max_y.max(bottom_y);
                    } else {
                        // Conservative fallback: treat a line box around baseline
                        let height = (*font_size).max(1.0);
                        let width = (*font_size) * 4.0; // rough estimate
                        min_x = min_x.min(*x);
                        min_y = min_y.min(*y - height);
                        max_x = max_x.max(*x + width);
                        max_y = max_y.max(*y);
                    }
                }
                _ => {}
            }
        }
        min_x.is_finite().then(|| {
            (
                min_x.max(0.0),
                min_y.max(0.0),
                (max_x - min_x).max(0.0),
                (max_y - min_y).max(0.0),
            )
        })
    }

    /// Initialize GPU device and queue.
    ///
    /// # Errors
    /// Returns an error if adapter or device initialization fails.
    async fn initialize_device() -> Result<(Instance, Adapter, Arc<Device>, Queue), AnyhowError> {
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
            log::error!(target: "wgpu_renderer", "Uncaptured WGPU error: {error:?}");
        }));
        Ok((instance, adapter, Arc::new(device), queue))
    }

    /// Setup surface with format selection and configuration.
    fn setup_surface(
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
    fn initialize_glyphon(
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

    /// Create the GPU device/surface and initialize a simple render pipeline.
    ///
    /// # Errors
    /// Returns an error if no suitable GPU adapter is found or if device creation fails.
    pub async fn new(window: Arc<Window>) -> Result<Self, AnyhowError> {
        let (instance, adapter, device, queue) = Self::initialize_device().await?;
        let size = window.inner_size();
        let (surface_opt, surface_format, render_format) =
            Self::setup_surface(&window, &instance, &adapter, &device, size);
        let (pipeline, vertex_buffer, vertex_count) =
            build_pipeline_and_buffers(&device, render_format);
        let (tex_pipeline, tex_bind_layout, linear_sampler) =
            build_texture_pipeline(&device, render_format);
        let glyphon = Self::initialize_glyphon(&device, &queue, render_format, size);
        Ok(Self {
            window,
            device,
            queue,
            size,
            surface: surface_opt,
            surface_format,
            render_format,
            pipeline,
            tex_pipeline,
            tex_bind_layout,
            linear_sampler,
            vertex_buffer,
            vertex_count,
            display_list: Vec::new(),
            text_list: Vec::new(),
            retained_display_list: None,
            font_system: glyphon.font_system,
            swash_cache: SwashCache::new(),
            text_atlas: glyphon.text_atlas,
            text_renderer: glyphon.text_renderer,
            glyphon_cache: glyphon.glyphon_cache,
            viewport: glyphon.viewport,
            layers: Vec::new(),
            clear_color: [1.0, 1.0, 1.0, 1.0],
            offscreen_tex: None,
            readback_buf: None,
            readback_padded_bpr: 0,
            readback_size: 0,
            live_textures: Vec::new(),
            live_buffers: Vec::new(),
        })
    }

    /// Window getter for integrations that require it.
    pub fn get_window(&self) -> &Window {
        &self.window
    }

    /// Set the framebuffer clear color (canvas background). RGBA in [0,1].
    pub const fn set_clear_color(&mut self, rgba: [f32; 4]) {
        self.clear_color = rgba;
    }

    /// Configure the swapchain/surface to match the current size and formats.
    fn configure_surface(&self) {
        let surface_config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: self.surface_format,
            // Request compatibility with the sRGB-format texture view we are going to create later.
            view_formats: vec![self.render_format],
            alpha_mode: CompositeAlphaMode::Auto,
            width: self.size.width,
            height: self.size.height,
            desired_maximum_frame_latency: 2,
            present_mode: PresentMode::AutoVsync,
        };
        if let Some(surface) = &self.surface {
            surface.configure(&self.device, &surface_config);
        }
    }

    /// Handle window resize and reconfigure the surface.
    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.size = new_size;
        self.configure_surface();
        // Clear pooled textures to avoid size/format mismatches
        // Drop persistent readback/offscreen so they are recreated at next render
        self.offscreen_tex = None;
        self.readback_buf = None;
        self.readback_padded_bpr = 0;
        self.readback_size = 0;
    }

    /// Clear any compositor layers; subsequent `render()` will use the single retained list if set.
    pub fn clear_layers(&mut self) {
        self.layers.clear();
    }

    /// Reset rendering state for the next frame. Critical for test isolation and preventing
    /// state corruption when reusing `RenderState` across multiple renders.
    ///
    /// This method:
    /// - Flushes pending GPU operations
    /// - Clears per-frame GPU resources
    /// - Resets compositor layers
    /// - Trims text atlas to prevent memory bloat
    /// - Reinitializes text renderer to prevent glyphon state corruption
    /// - Clears any cached state that could interfere with the next render
    pub fn reset_for_next_frame(&mut self) {
        // Force device to process all pending operations before clearing state
        // This prevents encoder corruption from incomplete operations
        let _unused = self.device.poll(PollType::Wait);

        // Clear per-frame GPU resources
        self.live_textures.clear();
        self.live_buffers.clear();

        // Clear compositor state
        self.layers.clear();

        // Trim text atlas to prevent unbounded growth and reset internal state
        // CRITICAL: Wrap glyphon operations in error scopes
        {
            let scope = ErrorScopeGuard::push(&self.device, "glyphon-atlas-trim");
            self.text_atlas.trim();
            if let Err(error) = scope.check() {
                log::error!(target: "wgpu_renderer", "Glyphon text_atlas.trim() generated validation error: {error:?}");
            }
        }

        // Recreate text renderer to prevent glyphon state corruption after opacity compositing
        // This is critical because glyphon maintains internal GPU state that can become invalid
        {
            let scope = ErrorScopeGuard::push(&self.device, "glyphon-renderer-recreate");
            self.text_renderer = TextRenderer::new(
                &mut self.text_atlas,
                &self.device,
                MultisampleState::default(),
                None,
            );
            if let Err(error) = scope.check() {
                log::error!(target: "wgpu_renderer", "Glyphon TextRenderer::new() generated validation error: {error:?}");
            }
        }

        // Clear display lists
        self.display_list.clear();
        self.text_list.clear();
    }

    /// Push a new compositor layer to be rendered in order.
    pub fn push_layer(&mut self, layer: Layer) {
        self.layers.push(layer);
    }

    /// Update the current display list to be drawn each frame.
    /// Update the current display list to be drawn each frame.
    pub fn set_display_list(&mut self, list: Vec<DrawRect>) {
        self.display_list = list;
    }

    /// Update the current text list to be drawn each frame.
    pub fn set_text_list(&mut self, list: Vec<DrawText>) {
        self.text_list = list;
    }

    /// Install a retained display list as the source of truth for rendering.
    /// When set, `render()` will prefer drawing directly from the retained list
    /// (with clip support) rather than the immediate lists.
    pub fn set_retained_display_list(&mut self, list: DisplayList) {
        // Using a single retained display list implies no layered compositing this frame.
        self.layers.clear();
        self.retained_display_list = Some(list);
        // Clear immediate lists; they will be ignored when retained list is present.
        self.display_list.clear();
        self.text_list.clear();
    }

    /// Render a frame by clearing and drawing quads from the current display list.
    ///
    /// # Errors
    /// Returns an error if surface acquisition or rendering fails.
    pub fn render(&mut self) -> Result<(), AnyhowError> {
        let _span = info_span!("renderer.render").entered();
        // Ensure previous-frame resources are dropped before starting
        self.live_textures.clear();
        self.live_buffers.clear();
        let surface = self
            .surface
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no surface available for on-screen render"))?;
        let surface_texture = surface.get_current_texture()?;
        let texture_view = surface_texture.texture.create_view(&TextureViewDescriptor {
            format: Some(self.render_format),
            ..Default::default()
        });

        // Use single CommandEncoder - simpler and more reliable
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("onscreen-frame"),
            });
        self.record_draw_passes(&texture_view, &mut encoder, false)?;

        // Finish and submit
        let command_buffer = encoder.finish();
        submit_with_validation(&self.device, &self.queue, [command_buffer])?;

        // After submission, it's safe to drop per-frame resources
        self.live_textures.clear();
        self.live_buffers.clear();
        self.window.pre_present_notify();
        surface_texture.present();
        Ok(())
    }

    /// Ensure offscreen texture exists and matches current framebuffer size.
    fn ensure_offscreen_texture(&mut self) {
        let framebuffer_width = self.size.width.max(1);
        let framebuffer_height = self.size.height.max(1);
        let need_offscreen = self.offscreen_tex.as_ref().is_none_or(|tex| {
            let size = tex.size();
            size.width != framebuffer_width
                || size.height != framebuffer_height
                || size.depth_or_array_layers != 1
        });
        if need_offscreen {
            let base_format = match self.render_format {
                TextureFormat::Rgba8UnormSrgb => TextureFormat::Rgba8Unorm,
                TextureFormat::Bgra8UnormSrgb => TextureFormat::Bgra8Unorm,
                other => other,
            };
            let tex = self.device.create_texture(&TextureDescriptor {
                label: Some("offscreen-target"),
                size: Extent3d {
                    width: framebuffer_width,
                    height: framebuffer_height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: base_format,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
                view_formats: &[self.render_format],
            });
            self.offscreen_tex = Some(tex);
        }
    }

    /// Render to offscreen texture and return the texture view.
    ///
    /// # Errors
    /// Returns an error if rendering or command submission fails.
    fn render_to_offscreen(&mut self) -> Result<(), AnyhowError> {
        let tmp_view = self
            .offscreen_tex
            .as_ref()
            .ok_or_else(|| anyhow!("offscreen texture not available"))?
            .create_view(&TextureViewDescriptor {
                format: Some(self.render_format),
                ..Default::default()
            });
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("render-to-rgba"),
            });
        self.record_draw_passes(&tmp_view, &mut encoder, true)?;
        let command_buffer = encoder.finish();
        submit_with_validation(&self.device, &self.queue, [command_buffer])?;
        self.live_textures.clear();
        self.live_buffers.clear();
        Ok(())
    }

    /// Ensure readback buffer exists and is large enough.
    fn ensure_readback_buffer(&mut self, padded_bpr: u32, buffer_size: u64) {
        let need_readback = self.readback_buf.is_none()
            || self.readback_padded_bpr != padded_bpr
            || self.readback_size < buffer_size;
        if need_readback {
            let buf = self.device.create_buffer(&BufferDescriptor {
                label: Some("render-readback"),
                size: buffer_size,
                usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            self.readback_buf = Some(buf);
            self.readback_padded_bpr = padded_bpr;
            self.readback_size = buffer_size;
        }
    }

    /// Copy offscreen texture to readback buffer.
    ///
    /// # Errors
    /// Returns an error if texture or buffer is not available.
    fn copy_texture_to_readback(
        &self,
        copy_encoder: &mut CommandEncoder,
        width: u32,
        height: u32,
        padded_bpr: u32,
    ) -> Result<(), AnyhowError> {
        copy_encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: self
                    .offscreen_tex
                    .as_ref()
                    .ok_or_else(|| anyhow!("offscreen texture not available"))?,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            TexelCopyBufferInfo {
                buffer: self
                    .readback_buf
                    .as_ref()
                    .ok_or_else(|| anyhow!("readback buffer not available"))?,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bpr),
                    rows_per_image: Some(height),
                },
            },
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }

    /// Read back texture data from GPU buffer.
    ///
    /// # Errors
    /// Returns an error if buffer mapping or readback fails.
    fn readback_texture_data(&self, params: ReadbackParams) -> Result<Vec<u8>, AnyhowError> {
        let ReadbackParams {
            width,
            height,
            bytes_per_pixel,
            row_bytes,
            padded_bytes_per_row,
        } = params;
        let readback = self
            .readback_buf
            .as_ref()
            .ok_or_else(|| anyhow!("readback buffer not available"))?;
        let slice = readback.slice(..);
        let (sender, receiver) = channel();
        slice.map_async(MapMode::Read, move |res| {
            drop(sender.send(res));
        });
        loop {
            let _unused = self.device.poll(PollType::Wait);
            if let Ok(res) = receiver.try_recv() {
                res?;
                break;
            }
        }
        let mapped = slice.get_mapped_range();
        let expected_total_bytes =
            (width as usize) * (height as usize) * (bytes_per_pixel as usize);
        let mut out = vec![0u8; expected_total_bytes];
        for row in 0..height as usize {
            let src_off = row * (padded_bytes_per_row as usize);
            let dst_off = row * (row_bytes as usize);
            out[dst_off..dst_off + (row_bytes as usize)]
                .copy_from_slice(&mapped[src_off..src_off + (row_bytes as usize)]);
        }
        drop(mapped);
        readback.unmap();
        Ok(out)
    }

    /// Convert BGRA to RGBA if needed.
    ///
    /// # Panics
    /// Panics if pixel chunks are not exactly 4 bytes (should never happen with `chunks_exact_mut(4)`).
    fn convert_bgra_to_rgba(&self, out: &mut [u8]) {
        match self.render_format {
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

    /// Render a frame and return the framebuffer RGBA bytes.
    /// Render the current display list to an RGBA buffer.
    ///
    /// # Errors
    /// Returns an error if rendering or texture readback fails.
    ///
    /// # Panics
    /// Panics if pixel chunks are not exactly 4 bytes (should never happen with `chunks_exact_mut(4)`).
    pub fn render_to_rgba(&mut self) -> Result<Vec<u8>, AnyhowError> {
        self.live_textures.clear();
        self.live_buffers.clear();

        let validation_scope = ErrorScopeGuard::push(&self.device, "pre-render-validation");
        validation_scope.check()?;

        self.ensure_offscreen_texture();
        self.render_to_offscreen()?;

        let mut copy_encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("texture-copy-encoder"),
            });

        let width = self.size.width.max(1);
        let height = self.size.height.max(1);
        let bpp = 4u32;
        let row_bytes = width * bpp;
        let padded_bpr = row_bytes.div_ceil(256) * 256;
        let buffer_size = u64::from(padded_bpr) * u64::from(height);

        self.ensure_readback_buffer(padded_bpr, buffer_size);
        self.copy_texture_to_readback(&mut copy_encoder, width, height, padded_bpr)?;

        let copy_command_buffer = {
            let scope = ErrorScopeGuard::push(&self.device, "copy_encoder.finish");
            let buffer = copy_encoder.finish();
            scope.check()?;
            buffer
        };
        submit_with_validation(&self.device, &self.queue, [copy_command_buffer])?;

        let mut out = self.readback_texture_data(ReadbackParams {
            width,
            height,
            bytes_per_pixel: bpp,
            row_bytes,
            padded_bytes_per_row: padded_bpr,
        })?;
        self.convert_bgra_to_rgba(&mut out);
        Ok(out)
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

    /// Prepare glyphon buffers for the current text list and upload glyphs into the atlas.
    pub(crate) fn glyphon_prepare(&mut self) {
        let framebuffer_width = self.size.width;
        let framebuffer_height = self.size.height;
        let scale: f32 = self.window.scale_factor() as f32;
        let text_list = self.text_list.clone();
        let buffers = self.create_glyphon_buffers(&text_list, scale);
        let areas = Self::create_text_areas(
            &buffers,
            &text_list,
            scale,
            framebuffer_width,
            framebuffer_height,
        );

        let viewport_scope = ErrorScopeGuard::push(&self.device, "glyphon-viewport-update");
        self.viewport.update(
            &self.queue,
            Resolution {
                width: framebuffer_width,
                height: framebuffer_height,
            },
        );
        if let Err(error) = viewport_scope.check() {
            log::error!(target: "wgpu_renderer", "Glyphon viewport.update() generated error: {error:?}");
            return;
        }

        let areas_count = areas.len();
        let prepare_scope = ErrorScopeGuard::push(&self.device, "glyphon-text-prepare");
        let prep_res = self.text_renderer.prepare(
            &self.device,
            &self.queue,
            &mut self.font_system,
            &mut self.text_atlas,
            &self.viewport,
            areas,
            &mut self.swash_cache,
        );
        if let Err(error) = prepare_scope.check() {
            log::error!(target: "wgpu_renderer", "Glyphon text_renderer.prepare() generated validation error: {error:?}");
        }
        log::debug!(
            target: "wgpu_renderer",
            "glyphon_prepare: areas={areas_count} viewport={framebuffer_width}x{framebuffer_height} result={prep_res:?}"
        );
    }

    /// Prepare glyphon buffers for a specific set of text items.
    pub(crate) fn glyphon_prepare_for(&mut self, items: &[DrawText]) {
        let framebuffer_width = self.size.width;
        let framebuffer_height = self.size.height;
        let scale: f32 = self.window.scale_factor() as f32;
        let buffers = self.create_glyphon_buffers(items, scale);
        let areas = Self::create_text_areas(
            &buffers,
            items,
            scale,
            framebuffer_width,
            framebuffer_height,
        );

        let viewport_scope_for = ErrorScopeGuard::push(&self.device, "glyphon-viewport-update-for");
        self.viewport.update(
            &self.queue,
            Resolution {
                width: framebuffer_width,
                height: framebuffer_height,
            },
        );
        if let Err(error) = viewport_scope_for.check() {
            log::error!(target: "wgpu_renderer", "Glyphon viewport.update() (for) generated error: {error:?}");
            return;
        }

        let areas_len = areas.len();
        let prepare_scope_for = ErrorScopeGuard::push(&self.device, "glyphon-text-prepare-for");
        let prep_res = self.text_renderer.prepare(
            &self.device,
            &self.queue,
            &mut self.font_system,
            &mut self.text_atlas,
            &self.viewport,
            areas,
            &mut self.swash_cache,
        );
        if let Err(error) = prepare_scope_for.check() {
            log::error!(target: "wgpu_renderer", "Glyphon text_renderer.prepare() (for) generated validation error: {error:?}");
        }
        log::debug!(
            target: "wgpu_renderer",
            "glyphon_prepare_for: items={} areas={} viewport={}x{} result={:?}",
            items.len(),
            areas_len,
            framebuffer_width,
            framebuffer_height,
            prep_res
        );
    }

    /// Draw a batch of text items with optional scissor rect.
    #[inline]
    pub(crate) fn draw_text_batch(
        &mut self,
        pass: &mut RenderPass<'_>,
        items: &[DrawText],
        scissor_opt: Option<Scissor>,
    ) {
        self.glyphon_prepare_for(items);
        pass.set_viewport(
            0.0,
            0.0,
            self.size.width as f32,
            self.size.height as f32,
            0.0,
            1.0,
        );
        match scissor_opt {
            Some((x, y, width, height)) => pass.set_scissor_rect(x, y, width, height),
            None => pass.set_scissor_rect(0, 0, self.size.width.max(1), self.size.height.max(1)),
        }
        {
            let scope = ErrorScopeGuard::push(&self.device, "glyphon-text-render");
            if let Err(error) = self
                .text_renderer
                .render(&self.text_atlas, &self.viewport, pass)
            {
                log::error!(target: "wgpu_renderer", "Glyphon text_renderer.render() failed: {error:?}");
            }
            if let Err(error) = scope.check() {
                log::error!(target: "wgpu_renderer", "Glyphon text_renderer.render() generated validation error: {error:?}");
            }
        }
    }

    /// Draw multiple text batches with their respective scissor rects.
    #[inline]
    pub(crate) fn draw_text_batches(&mut self, pass: &mut RenderPass<'_>, batches: Vec<TextBatch>) {
        for (scissor_opt, items) in batches.into_iter().filter(|(_, items)| !items.is_empty()) {
            self.draw_text_batch(pass, &items, scissor_opt);
        }
    }
}

/// Pixel bounds (x, y, width, height)
pub(crate) type Bounds = (f32, f32, f32, f32);

/// Parameters for texture readback operation.
#[derive(Copy, Clone)]
struct ReadbackParams {
    /// Width of the texture in pixels.
    width: u32,
    /// Height of the texture in pixels.
    height: u32,
    /// Number of bytes per pixel (typically 4 for RGBA).
    bytes_per_pixel: u32,
    /// Number of bytes per row (width * `bytes_per_pixel`).
    row_bytes: u32,
    /// Padded bytes per row (aligned to 256 bytes for GPU buffer requirements).
    padded_bytes_per_row: u32,
}
