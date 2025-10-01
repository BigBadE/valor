use crate::error::submit_with_validation;
use crate::pipelines::{Vertex, build_pipeline_and_buffers, build_texture_pipeline};
use crate::text::{batch_layer_texts_with_scissor, batch_texts_with_scissor, map_text_item};
use anyhow::Result as AnyResult;
use anyhow::anyhow;
use glyphon::{Cache, FontSystem, Resolution, SwashCache, TextAtlas, TextRenderer, Viewport};
use log::debug;
use renderer::display_list::{
    DisplayItem, DisplayList, StackingContextBoundary, batch_display_list,
};
use renderer::renderer::{DrawRect, DrawText};
use std::sync::Arc;
use tracing::info_span;
use wgpu::util::DeviceExt;
use wgpu::*;
use winit::dpi::PhysicalSize;
use winit::window::Window;

// pollster is used via crate::error helpers.

// Composite info for a pre-rendered opacity group.
// (start_index, end_index, texture, texture_view, tex_w, tex_h, alpha, bounds, bind_group)
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

// Compact representation for preprocessed layer data: either no content
// or (items, composites, excluded ranges).
pub(crate) type LayerEntry = Option<(Vec<DisplayItem>, Vec<OpacityComposite>, Vec<(usize, usize)>)>;

// Type alias for scissor rectangle: (x, y, width, height)
type ScissorRect = Option<(u32, u32, u32, u32)>;

// Vertex structure for texture quad rendering
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct TexVertex {
    pos: [f32; 2],
    uv: [f32; 2],
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
/// CRITICAL: Must call check() before dropping to avoid error scope imbalance.
pub(crate) struct ErrorScopeGuard {
    device: Arc<Device>,
    label: &'static str,
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

    /// Check for errors and return a Result. This MUST be called before the guard drops.
    /// Consumes self to prevent reuse.
    pub(crate) fn check(mut self) -> AnyResult<()> {
        self.checked = true;
        if let Some(err) = pollster::block_on(self.device.pop_error_scope()) {
            // Log the FULL error details to understand what's actually failing
            log::error!(target: "wgpu_renderer", "WGPU VALIDATION ERROR in scope: '{}'", self.label);
            log::error!(target: "wgpu_renderer", "Error type: {err:?}");
            log::error!(target: "wgpu_renderer", "Full details: {err:#?}");
            return Err(anyhow!("wgpu scoped error in {}: {err:?}", self.label));
        }
        Ok(())
    }
}

impl Drop for ErrorScopeGuard {
    fn drop(&mut self) {
        if !self.checked {
            // CRITICAL: If check() wasn't called, this is a bug that will cause error scope imbalance
            log::error!(target: "wgpu_renderer", "ERROR SCOPE NOT CHECKED: '{}' - This will cause scope imbalance!", self.label);
            // Pop anyway to prevent complete corruption, but log the error
            let _ = pollster::block_on(self.device.pop_error_scope());
        }
    }
}

/// Rendering context that encapsulates viewport and size information.
/// This is passed as a parameter instead of mutating shared state.
#[derive(Debug, Copy, Clone)]
struct RenderContext {
    viewport_size: PhysicalSize<u32>,
}

impl RenderContext {
    fn new(size: PhysicalSize<u32>) -> Self {
        Self {
            viewport_size: size,
        }
    }

    fn width(&self) -> u32 {
        self.viewport_size.width.max(1)
    }

    fn height(&self) -> u32 {
        self.viewport_size.height.max(1)
    }
}

// TODO: Full render graph implementation for future optimization
// For now, we use the single-encoder pattern which is the critical architectural fix

#[derive(Debug, Clone)]
pub enum Layer {
    Background,
    Content(DisplayList),
    Chrome(DisplayList),
}

/// RenderState owns the GPU device/surface and a minimal pipeline to draw rectangles from layout.
pub struct RenderState {
    pub(crate) window: Arc<Window>,
    pub(crate) device: Arc<Device>,
    pub(crate) queue: Queue,
    pub(crate) size: PhysicalSize<u32>,
    surface: Option<Surface<'static>>,
    surface_format: TextureFormat,
    render_format: TextureFormat,
    pipeline: RenderPipeline,
    tex_pipeline: RenderPipeline,
    tex_bind_layout: BindGroupLayout,
    linear_sampler: Sampler,
    vertex_buffer: Buffer,
    vertex_count: u32,
    display_list: Vec<DrawRect>,
    pub(crate) text_list: Vec<DrawText>,
    /// Retained display list for Phase 6. When set via set_retained_display_list,
    /// it becomes the source of truth and is flattened into the immediate lists.
    retained_display_list: Option<DisplayList>,
    // Glyphon text rendering state
    pub(crate) font_system: FontSystem,
    pub(crate) swash_cache: SwashCache,
    pub(crate) text_atlas: TextAtlas,
    pub(crate) text_renderer: TextRenderer,
    #[allow(dead_code)]
    pub(crate) glyphon_cache: Cache,
    pub(crate) viewport: Viewport,
    /// Optional layers for multi-DL compositing; when non-empty, render() draws these instead of the single retained list.
    layers: Vec<Layer>,
    /// Clear color for the framebuffer (canvas background). RGBA in [0,1].
    clear_color: [f32; 4],
    /// Persistent offscreen render target for readback-based renders
    offscreen_tex: Option<Texture>,
    /// Persistent readback buffer sized for current framebuffer (padded bytes-per-row)
    readback_buf: Option<Buffer>,
    readback_padded_bpr: u32,
    readback_size: u64,
    /// Keep GPU resources alive until after submission to avoid encoder invalidation at finish.
    live_textures: Vec<Texture>,
    /// Keep transient GPU buffers (vertex/uniform) alive through submission.
    live_buffers: Vec<Buffer>,
}

impl RenderState {
    #[inline]
    fn preprocess_layer_with_encoder(
        &mut self,
        encoder: &mut CommandEncoder,
        layer: &Layer,
    ) -> AnyResult<LayerEntry> {
        match layer {
            Layer::Background => Ok(None),
            Layer::Content(dl) | Layer::Chrome(dl) => {
                let items: Vec<DisplayItem> = dl.items.clone();
                let comps = self.collect_opacity_composites(encoder, &items)?;
                let ranges = self.build_exclude_ranges(&comps);
                Ok(Some((items, comps, ranges)))
            }
        }
    }
    #[inline]
    fn collect_opacity_composites(
        &mut self,
        encoder: &mut CommandEncoder,
        items: &[DisplayItem],
    ) -> AnyResult<Vec<OpacityComposite>> {
        let mut out: Vec<OpacityComposite> = Vec::new();
        let mut i = 0usize;
        while i < items.len() {
            if let DisplayItem::BeginStackingContext { boundary } = &items[i]
                && matches!(boundary, StackingContextBoundary::Opacity { alpha } if *alpha < 1.0)
            {
                let end = self.find_stacking_context_end(items, i + 1);
                let group_items = &items[i + 1..end];
                let alpha = match boundary {
                    StackingContextBoundary::Opacity { alpha } => *alpha,
                    _ => 1.0,
                };
                let bounds = self
                    .compute_items_bounds(group_items)
                    .unwrap_or((0.0, 0.0, 1.0, 1.0));
                let (tex, view, tw, th, bind_group) = self
                    .render_items_to_offscreen_bounded_with_bind_group(
                        encoder,
                        group_items,
                        bounds,
                        alpha,
                    )?;
                out.push((i, end, tex, view, tw, th, alpha, bounds, bind_group));
                i = end + 1;
                continue;
            }
            i += 1;
        }
        Ok(out)
    }

    #[inline]
    fn draw_items_excluding_ranges(
        &mut self,
        pass: &mut RenderPass<'_>,
        items: &[DisplayItem],
        exclude: &[(usize, usize)],
    ) {
        let mut i = 0usize;
        let mut ex_idx = 0usize;
        while i < items.len() {
            if ex_idx < exclude.len() && i == exclude[ex_idx].0 {
                i = exclude[ex_idx].1 + 1;
                ex_idx += 1;
                continue;
            }
            let next = exclude.get(ex_idx).map(|r| r.0).unwrap_or(items.len());
            if i < next {
                // Use draw_items_with_groups to properly handle stacking contexts
                let _ = self.draw_items_with_groups(pass, &items[i..next]);
                i = next;
            }
        }
    }

    #[inline]
    fn build_exclude_ranges(&self, comps: &[OpacityComposite]) -> Vec<(usize, usize)> {
        let mut ranges = Vec::with_capacity(comps.len());
        for (s, e, ..) in comps.iter() {
            ranges.push((*s, *e));
        }
        ranges
    }

    #[inline]
    fn composite_groups(
        &mut self,
        pass: &mut RenderPass<'_>,
        comps: Vec<OpacityComposite>,
    ) -> AnyResult<()> {
        for (_s, _e, tex, _view, _tw, _th, _alpha, bounds, bind_group) in comps.into_iter() {
            // Keep the texture alive until after submit; otherwise some backends invalidate at finish.
            self.live_textures.push(tex);
            self.draw_texture_quad_with_bind_group(pass, &bind_group, bounds)?;
        }
        Ok(())
    }

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
        let _ = alpha; // alpha handled in higher-level compositing when applicable
        // Use draw_items_with_groups to handle nested stacking contexts
        self.draw_items_with_groups(pass, group_items)
    }

    /// Helper method to render layers pass, extracted to reduce nesting.
    fn render_layers_pass(
        &mut self,
        encoder: &mut CommandEncoder,
        texture_view: &TextureView,
        main_load: LoadOp<Color>,
        per_layer: Vec<LayerEntry>,
    ) -> AnyResult<()> {
        let scope = ErrorScopeGuard::push(&self.device, "main-pass(layers)");
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
                self.composite_groups(&mut pass, comps)?;
            }
            pass.pop_debug_group();
            debug!(target: "wgpu_renderer", "end main-pass");
        }
        scope.check()
    }

    /// Helper method to render retained display list pass, extracted to reduce nesting.
    fn render_retained_pass(
        &mut self,
        encoder: &mut CommandEncoder,
        texture_view: &TextureView,
        main_load: LoadOp<Color>,
        items: &[DisplayItem],
        comps: Vec<OpacityComposite>,
    ) -> AnyResult<()> {
        log::debug!(target: "wgpu_renderer", "=== CREATING MAIN RENDER PASS (retained) ===");
        log::debug!(target: "wgpu_renderer", "    Composites to apply: {}", comps.len());
        log::debug!(target: "wgpu_renderer", "    Texture view: {texture_view:?}");
        log::debug!(target: "wgpu_renderer", "    Load op: {main_load:?}");
        let scope = ErrorScopeGuard::push(&self.device, "main-pass(retained)");
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

            let ranges = self.build_exclude_ranges(&comps);
            self.draw_items_excluding_ranges(&mut pass, items, &ranges);
            self.composite_groups(&mut pass, comps)?;
            pass.pop_debug_group();
            debug!(target: "wgpu_renderer", "end main-pass");
        }
        scope.check()
    }

    /// Helper method to render immediate mode pass, extracted to reduce nesting.
    fn render_immediate_pass(
        &self,
        encoder: &mut CommandEncoder,
        texture_view: &TextureView,
        load_op: LoadOp<Color>,
    ) -> AnyResult<()> {
        let scope = ErrorScopeGuard::push(&self.device, "main-pass(immediate)");
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
        }
        scope.check()
    }

    /// Check if a display item is an opacity stacking context.
    fn is_opacity_context(item: &DisplayItem) -> bool {
        matches!(
            item,
            DisplayItem::BeginStackingContext {
                boundary: StackingContextBoundary::Opacity { alpha }
            } if *alpha < 1.0
        )
    }

    /// Check if display list has opacity groups needing offscreen rendering.
    fn has_opacity_groups(&self, use_retained: bool) -> bool {
        if use_retained {
            if let Some(dl) = &self.retained_display_list {
                return dl.items.iter().any(Self::is_opacity_context);
            }
            false
        } else if !self.layers.is_empty() {
            self.layers.iter().any(|layer| {
                if let Layer::Content(dl) | Layer::Chrome(dl) = layer {
                    dl.items.iter().any(Self::is_opacity_context)
                } else {
                    false
                }
            })
        } else {
            false
        }
    }

    /// Record all render passes (rectangles + text) into the provided texture view.
    /// Internally splits into multiple command buffers when needed for D3D12 resource transitions.
    /// Returns true if the passed encoder was used, false if split path handled everything internally.
    fn record_draw_passes(
        &mut self,
        texture_view: &TextureView,
        encoder: &mut CommandEncoder,
        use_retained: bool,
        allow_split: bool, // Only split for direct swapchain rendering
    ) -> AnyResult<bool> {
        // Check if we have opacity groups that need offscreen rendering
        let needs_offscreen = self.has_opacity_groups(use_retained);

        // Only use split path for direct swapchain rendering, not for render_to_rgba
        // (which already renders to an intermediate texture)
        if needs_offscreen && allow_split {
            // D3D12 path: Split into multiple command buffers
            // Phase 1: Render offscreen, submit, then Phase 2: Render main+text
            self.record_draw_passes_split(texture_view, use_retained)?;
            Ok(false) // Did NOT use the passed encoder
        } else {
            // No offscreen rendering needed, use single encoder
            self.record_draw_passes_single(texture_view, encoder, use_retained)?;
            Ok(true) // DID use the passed encoder
        }
    }

    /// Record all passes without offscreen rendering (single encoder path).
    fn record_draw_passes_single(
        &mut self,
        texture_view: &TextureView,
        encoder: &mut CommandEncoder,
        use_retained: bool,
    ) -> AnyResult<()> {
        let use_layers = !self.layers.is_empty();
        let is_offscreen = false;

        // Determine load operations
        let main_load = LoadOp::Load;
        let text_load = LoadOp::Load;

        // Prepare text via glyphon for single-list paths
        if use_retained {
            if let Some(dl) = &self.retained_display_list {
                self.text_list = dl.items.iter().filter_map(map_text_item).collect();
            }
            self.glyphon_prepare();
        } else if !use_layers {
            // Immediate path uses whatever self.text_list was set to externally
            self.glyphon_prepare();
        }

        // First pass: rectangles only
        {
            // For offscreen, we'll clear in the main pass itself to avoid multi-pass hazards.
            if !is_offscreen {
                debug!(target: "wgpu_renderer", "start clear-pass");
                let scope = ErrorScopeGuard::push(&self.device, "clear-pass");
                {
                    let _clear_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                        label: Some("clear-pass"),
                        color_attachments: &[Some(RenderPassColorAttachment {
                            view: texture_view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: Operations {
                                load: LoadOp::Clear(Color {
                                    r: self.clear_color[0] as f64,
                                    g: self.clear_color[1] as f64,
                                    b: self.clear_color[2] as f64,
                                    a: self.clear_color[3] as f64,
                                }),
                                store: StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                }
                scope.check()?;
                debug!(target: "wgpu_renderer", "end clear-pass");
            }

            if use_layers {
                // Pre-collect opacity composites for each layer BEFORE opening the main pass
                // Offscreen rendering uses the same encoder with properly scoped render passes
                let per_layer: Vec<LayerEntry> = self
                    .layers
                    .clone()
                    .iter()
                    .map(|l| self.preprocess_layer_with_encoder(encoder, l))
                    .collect::<AnyResult<Vec<_>>>()?;

                // Open the main pass and draw layers: non-group items first, then composite groups
                self.render_layers_pass(encoder, texture_view, main_load, per_layer)?;
            } else if use_retained {
                if let Some(dl) = self.retained_display_list.clone() {
                    let items: Vec<DisplayItem> = dl.items;
                    // No offscreen rendering in single-encoder path
                    self.render_retained_pass(encoder, texture_view, main_load, &items, vec![])?;
                }
            } else {
                // Immediate path: batch all rects into one draw call as before
                let mut vertices: Vec<Vertex> = Vec::with_capacity(self.display_list.len() * 6);
                for rect in &self.display_list {
                    let rgba = [rect.color[0], rect.color[1], rect.color[2], 1.0];
                    self.push_rect_vertices_ndc(
                        &mut vertices,
                        [rect.x, rect.y, rect.width, rect.height],
                        rgba,
                    );
                }
                let vertex_bytes = bytemuck::cast_slice(vertices.as_slice());
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
                self.render_immediate_pass(encoder, texture_view, immediate_load)?;
            }
        }

        // Second pass: text rendering
        {
            let scope = ErrorScopeGuard::push(&self.device, "text-pass");
            {
                let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                    label: Some("text-pass"),
                    color_attachments: &[Some(RenderPassColorAttachment {
                        view: texture_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: Operations {
                            load: text_load,
                            store: StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                debug!(target: "wgpu_renderer", "start text-pass");
                pass.push_debug_group("text-pass");
                if use_layers {
                    let batches = batch_layer_texts_with_scissor(
                        &self.layers,
                        self.size.width,
                        self.size.height,
                    );
                    self.draw_text_batches(&mut pass, batches);
                } else if use_retained && let Some(dl) = &self.retained_display_list {
                    let batches = batch_texts_with_scissor(dl, self.size.width, self.size.height);
                    self.draw_text_batches(&mut pass, batches);
                }
                pass.pop_debug_group();
                debug!(target: "wgpu_renderer", "end text-pass");
            }
            scope.check()?;
        }

        // All render passes complete - single encoder pattern
        Ok(())
    }

    /// Record passes with offscreen rendering (split encoder path for D3D12).
    /// Phase 1: Render all offscreen textures and submit.
    /// Phase 2: Render main pass (using offscreen textures) + text pass.
    fn record_draw_passes_split(
        &mut self,
        texture_view: &TextureView,
        use_retained: bool,
    ) -> AnyResult<()> {
        log::debug!(target: "wgpu_renderer", "=== Using SPLIT encoder path for D3D12 compatibility ===");

        let use_layers = !self.layers.is_empty();
        let main_load = LoadOp::Load;
        let text_load = LoadOp::Load;

        // Prepare text via glyphon
        if use_retained {
            if let Some(dl) = &self.retained_display_list {
                self.text_list = dl.items.iter().filter_map(map_text_item).collect();
            }
            self.glyphon_prepare();
        } else if !use_layers {
            self.glyphon_prepare();
        }

        // PHASE 1: Render all offscreen opacity groups
        log::debug!(target: "wgpu_renderer", "PHASE 1: Rendering offscreen opacity groups");
        let mut offscreen_encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("offscreen-opacity-encoder"),
            });
        offscreen_encoder.push_debug_group("offscreen-opacity-phase");

        let comps = if use_retained {
            if let Some(dl) = self.retained_display_list.clone() {
                let items: Vec<DisplayItem> = dl.items;
                self.collect_opacity_composites(&mut offscreen_encoder, &items)?
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        offscreen_encoder.pop_debug_group();
        let offscreen_cb = {
            let scope = ErrorScopeGuard::push(&self.device, "offscreen-encoder.finish");
            let cb = offscreen_encoder.finish();
            scope.check()?;
            cb
        };

        log::debug!(target: "wgpu_renderer", "PHASE 1 complete: {} opacity groups rendered, submitting...", comps.len());
        submit_with_validation(&self.device, &self.queue, [offscreen_cb])?;
        log::debug!(target: "wgpu_renderer", "PHASE 1 submitted successfully");

        // Wait for Phase 1 GPU work to complete before starting Phase 2
        // This ensures D3D12 resource transitions are fully processed
        let _ = self.device.poll(wgpu::PollType::Wait);
        log::debug!(target: "wgpu_renderer", "PHASE 1 GPU work complete");

        // PHASE 2: Render main pass + text pass using offscreen textures
        log::debug!(target: "wgpu_renderer", "PHASE 2: Rendering main and text passes");
        let mut main_encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("main-and-text-encoder"),
            });
        main_encoder.push_debug_group("main-and-text-phase");

        // Clear pass
        {
            debug!(target: "wgpu_renderer", "start clear-pass");
            let scope = ErrorScopeGuard::push(&self.device, "clear-pass");
            {
                let _clear_pass = main_encoder.begin_render_pass(&RenderPassDescriptor {
                    label: Some("clear-pass"),
                    color_attachments: &[Some(RenderPassColorAttachment {
                        view: texture_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: Operations {
                            load: LoadOp::Clear(Color {
                                r: self.clear_color[0] as f64,
                                g: self.clear_color[1] as f64,
                                b: self.clear_color[2] as f64,
                                a: self.clear_color[3] as f64,
                            }),
                            store: StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
            }
            scope.check()?;
            debug!(target: "wgpu_renderer", "end clear-pass");
        }

        // Main pass with opacity compositing
        if use_retained && let Some(dl) = self.retained_display_list.clone() {
            let items: Vec<DisplayItem> = dl.items;
            // No offscreen rendering in single-encoder path
            {
                let scope = ErrorScopeGuard::push(&self.device, "main-pass-retained");
                self.render_retained_pass(&mut main_encoder, texture_view, main_load, &items, comps)?;
                scope.check()?;
            }
        }

        // Text pass
        {
            let scope = ErrorScopeGuard::push(&self.device, "text-pass");
            {
                let mut pass = main_encoder.begin_render_pass(&RenderPassDescriptor {
                    label: Some("text-pass"),
                    color_attachments: &[Some(RenderPassColorAttachment {
                        view: texture_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: Operations {
                            load: text_load,
                            store: StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                debug!(target: "wgpu_renderer", "start text-pass");
                pass.push_debug_group("text-pass");
                if use_retained && let Some(dl) = &self.retained_display_list {
                    let batches = batch_texts_with_scissor(dl, self.size.width, self.size.height);
                    self.draw_text_batches(&mut pass, batches);
                }
                pass.pop_debug_group();
                debug!(target: "wgpu_renderer", "end text-pass");
            }
            scope.check()?;
        }

        main_encoder.pop_debug_group();
        let main_cb = {
            let scope = ErrorScopeGuard::push(&self.device, "main-encoder.finish");
            let cb = main_encoder.finish();
            scope.check()?;
            cb
        };

        log::debug!(target: "wgpu_renderer", "PHASE 2 complete, submitting...");
        submit_with_validation(&self.device, &self.queue, [main_cb])?;
        log::debug!(target: "wgpu_renderer", "PHASE 2 submitted successfully");

        Ok(())
    }

    #[inline]
    fn push_rect_vertices_ndc(&self, out: &mut Vec<Vertex>, rect_xywh: [f32; 4], color: [f32; 4]) {
        let fw = self.size.width.max(1) as f32;
        let fh = self.size.height.max(1) as f32;
        let [x, y, w, h] = rect_xywh;
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let x0 = (x / fw) * 2.0 - 1.0;
        let x1 = ((x + w) / fw) * 2.0 - 1.0;
        let y0 = 1.0 - (y / fh) * 2.0;
        let y1 = 1.0 - ((y + h) / fh) * 2.0;
        // Pass through color; shader handles sRGB->linear conversion for blending.
        let c = color;
        out.extend_from_slice(&[
            Vertex {
                position: [x0, y0],
                color: c,
            },
            Vertex {
                position: [x1, y0],
                color: c,
            },
            Vertex {
                position: [x1, y1],
                color: c,
            },
            Vertex {
                position: [x0, y0],
                color: c,
            },
            Vertex {
                position: [x1, y1],
                color: c,
            },
            Vertex {
                position: [x0, y1],
                color: c,
            },
        ]);
    }

    /// Draw display items with proper stacking context handling
    /// Spec: CSS 2.2 §9.9.1 - Stacking contexts and paint order
    fn draw_items_with_groups(
        &mut self,
        pass: &mut RenderPass<'_>,
        items: &[DisplayItem],
    ) -> AnyResult<()> {
        let mut i = 0usize;

        while i < items.len() {
            match &items[i] {
                DisplayItem::BeginStackingContext { boundary } => {
                    // Find the matching end boundary
                    let end = self.find_stacking_context_end(items, i + 1);
                    let group_items = &items[i + 1..end];

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

                    i = end + 1; // Skip to after EndStackingContext
                }
                DisplayItem::EndStackingContext => {
                    // This should be handled by the BeginStackingContext case
                    i += 1;
                }
                _ => {
                    // Regular display item - find the next stacking context boundary
                    let start = i;
                    let mut end = i;
                    while end < items.len() {
                        match &items[end] {
                            DisplayItem::BeginStackingContext { .. } => break,
                            _ => end += 1,
                        }
                    }

                    if start < end {
                        self.draw_items_batched(pass, &items[start..end]);
                    }
                    i = end;
                }
            }
        }
        Ok(())
    }

    /// Context-aware version of draw_items_with_groups that uses RenderContext instead of self.size.
    /// This prevents state corruption when rendering to different-sized targets.
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

    /// Context-aware version of draw_text_batch that uses RenderContext.
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

    /// Find the matching EndStackingContext for a BeginStackingContext
    /// Spec: Proper nesting of stacking context boundaries
    #[inline]
    fn find_stacking_context_end(&self, items: &[DisplayItem], start: usize) -> usize {
        let mut depth = 1i32;
        let mut j = start;
        while j < items.len() {
            match &items[j] {
                DisplayItem::BeginStackingContext { .. } => depth += 1,
                DisplayItem::EndStackingContext => {
                    depth -= 1;
                    if depth == 0 {
                        return j;
                    }
                }
                _ => {}
            }
            j += 1;
        }
        items.len() // fallback if unmatched
    }

    #[inline]
    fn draw_items_batched(&mut self, pass: &mut RenderPass<'_>, items: &[DisplayItem]) {
        let sub = DisplayList::from_items(items.to_vec());
        let batches = batch_display_list(&sub, self.size.width, self.size.height);
        for b in batches.into_iter() {
            let mut vertices: Vec<Vertex> = Vec::with_capacity(b.quads.len() * 6);
            for q in b.quads.iter() {
                self.push_rect_vertices_ndc(&mut vertices, [q.x, q.y, q.width, q.height], q.color);
            }
            let vertex_bytes = bytemuck::cast_slice(vertices.as_slice());
            let vertex_buffer = self.device.create_buffer_init(&util::BufferInitDescriptor {
                label: Some("layer-rect-batch"),
                contents: vertex_bytes,
                usage: BufferUsages::VERTEX,
            });
            // Keep buffer alive until submission to avoid backend lifetime edge-cases
            self.live_buffers.push(vertex_buffer.clone());
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            match b.scissor {
                Some((x, y, w, h)) => {
                    let fw = self.size.width.max(1);
                    let fh = self.size.height.max(1);
                    let rx = x.min(fw);
                    let ry = y.min(fh);
                    let rw = w.min(fw.saturating_sub(rx));
                    let rh = h.min(fh.saturating_sub(ry));
                    if rw == 0 || rh == 0 {
                        // Nothing visible; skip draw for this batch
                        continue;
                    }
                    pass.set_scissor_rect(rx, ry, rw, rh)
                }
                None => {
                    pass.set_scissor_rect(0, 0, self.size.width.max(1), self.size.height.max(1))
                }
            }
            if !vertices.is_empty() {
                // Draw if we generated any geometry
                pass.draw(0..(vertices.len() as u32), 0..1);
            }
        }
    }

    /// Render items to offscreen texture with tight bounds and create bind group.
    /// Uses RAII guards to ensure state is always properly restored.
    ///
    /// PRODUCTION-GRADE ARCHITECTURE: Uses the shared encoder (single encoder per frame).
    /// Render passes are properly scoped to ensure automatic texture state transitions.
    /// This is the correct pattern used by Chromium, Firefox, and Safari.
    ///
    /// Returns (texture, view, width, height, bind_group).
    #[allow(clippy::type_complexity)]
    fn render_items_to_offscreen_bounded_with_bind_group(
        &mut self,
        encoder: &mut CommandEncoder,
        items: &[DisplayItem],
        bounds: (f32, f32, f32, f32),
        alpha: f32,
    ) -> AnyResult<(Texture, TextureView, u32, u32, BindGroup)> {
        let (x, y, width, height) = bounds;
        let tex_width = (width.ceil() as u32).max(1);
        let tex_height = (height.ceil() as u32).max(1);

        log::debug!(target: "wgpu_renderer", "render_items_to_offscreen_bounded: bounds=({}, {}, {}, {}), tex_size={}x{}, items={}",
            x, y, width, height, tex_width, tex_height, items.len());

        // With split-encoder approach, D3D12 can handle sRGB textures as RENDER_ATTACHMENT
        // because the resource transition happens between command list submissions
        let offscreen_format = self.render_format;

        // Create texture for this offscreen pass
        // CRITICAL: The texture MUST have TEXTURE_BINDING usage to be used as a shader resource
        let texture = self.device.create_texture(&TextureDescriptor {
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
            // CRITICAL: All three usages are required:
            // - RENDER_ATTACHMENT: to render into the texture
            // - TEXTURE_BINDING: to use it as a shader resource in the main pass
            // - COPY_SRC: for potential readback operations
            usage: TextureUsages::RENDER_ATTACHMENT
                | TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        log::debug!(target: "wgpu_renderer", "Created offscreen texture: format={:?}, usage={:?}",
            offscreen_format, texture.usage());

        // Use same format for view as texture
        let view = texture.create_view(&TextureViewDescriptor {
            label: Some("offscreen-opacity-view"),
            format: Some(offscreen_format),
            ..Default::default()
        });

        // Create rendering context for offscreen texture
        let ctx = RenderContext::new(PhysicalSize::new(tex_width, tex_height));

        // Use the shared encoder - render passes will be properly scoped
        // WGPU automatically handles texture state transitions when render passes end

        // Translate items to texture-local coordinates
        let translated_items: Vec<DisplayItem> = items
            .iter()
            .map(|item| match item {
                DisplayItem::Rect {
                    x: rx,
                    y: ry,
                    width: rw,
                    height: rh,
                    color,
                } => DisplayItem::Rect {
                    x: rx - x,
                    y: ry - y,
                    width: *rw,
                    height: *rh,
                    color: *color,
                },
                DisplayItem::Text {
                    x: tx,
                    y: ty,
                    text,
                    color,
                    font_size,
                    bounds,
                } => DisplayItem::Text {
                    x: tx - x,
                    y: ty - y,
                    text: text.clone(),
                    color: *color,
                    font_size: *font_size,
                    bounds: bounds.map(|(l, t, r, b)| {
                        (
                            (l as f32 - x) as i32,
                            (t as f32 - y) as i32,
                            (r as f32 - x) as i32,
                            (b as f32 - y) as i32,
                        )
                    }),
                },
                other => other.clone(),
            })
            .collect();

        // Rects pass with error scope guard
        {
            log::debug!(target: "wgpu_renderer", ">>> CREATING offscreen rects pass");
            let scope = ErrorScopeGuard::push(&self.device, "opacity-offscreen-pass");
            {
                log::debug!(target: "wgpu_renderer", "    begin_render_pass(opacity-offscreen-pass)");
                let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                    label: Some("opacity-offscreen-pass"),
                    color_attachments: &[Some(RenderPassColorAttachment {
                        view: &view,
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
                pass.set_viewport(0.0, 0.0, tex_width as f32, tex_height as f32, 0.0, 1.0);
                pass.set_pipeline(&self.pipeline);
                log::debug!(target: "wgpu_renderer", "    Drawing items");
                self.draw_items_with_groups_ctx(&mut pass, &translated_items, ctx)?;
                log::debug!(target: "wgpu_renderer", "    About to drop pass");
            }
            log::debug!(target: "wgpu_renderer", "<<< Pass DROPPED, checking error scope");
            // CRITICAL: Check for errors immediately after pass drops
            let check_result = scope.check();
            if let Err(e) = &check_result {
                log::error!(target: "wgpu_renderer", "Offscreen rects pass had error: {e:?}");
            }
            check_result?;
            log::debug!(target: "wgpu_renderer", "    Error scope checked OK");
        }

        // Text pass with error scope guard
        let text_items: Vec<DrawText> = translated_items.iter().filter_map(map_text_item).collect();
        if !text_items.is_empty() {
            log::debug!(target: "wgpu_renderer", ">>> CREATING offscreen text pass");
            self.glyphon_prepare_for(text_items.as_slice());
            let scope = ErrorScopeGuard::push(&self.device, "opacity-offscreen-text-pass");
            {
                log::debug!(target: "wgpu_renderer", "    begin_render_pass(opacity-offscreen-text-pass)");
                let mut text_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                    label: Some("opacity-offscreen-text-pass"),
                    color_attachments: &[Some(RenderPassColorAttachment {
                        view: &view,
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
                text_pass.set_viewport(0.0, 0.0, tex_width as f32, tex_height as f32, 0.0, 1.0);
                self.draw_text_batch_ctx(&mut text_pass, text_items.as_slice(), None, ctx);
                log::debug!(target: "wgpu_renderer", "    About to drop text pass");
            }
            log::debug!(target: "wgpu_renderer", "<<< Text pass DROPPED, checking error scope");
            scope.check()?;
            log::debug!(target: "wgpu_renderer", "    Text error scope checked OK");
        }

        // Render passes are complete and properly scoped
        // WGPU has automatically transitioned the texture from RENDER_ATTACHMENT to TEXTURE_BINDING state
        // NOW create the bind group while encoder is between render passes
        log::debug!(target: "wgpu_renderer", "Offscreen render passes complete, creating bind group");

        // Create uniform buffer for alpha (std140-like padded to 16 bytes)
        let alpha_buf = self.device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("opacity-alpha"),
            contents: bytemuck::cast_slice(&[alpha, 0.0f32, 0.0f32, 0.0f32]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });
        self.live_buffers.push(alpha_buf.clone());

        let bind_group = self.device.create_bind_group(&BindGroupDescriptor {
            label: Some("opacity-tex-bind"),
            layout: &self.tex_bind_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&view),
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
        });

        log::debug!(target: "wgpu_renderer", "Bind group created, texture ready for compositing");

        Ok((texture, view, tex_width, tex_height, bind_group))
    }

    /// Draw a textured quad using a pre-created bind group (called from within render pass)
    fn draw_texture_quad_with_bind_group(
        &mut self,
        pass: &mut RenderPass<'_>,
        bind_group: &BindGroup,
        bounds: Bounds, // x, y, w, h in px
    ) -> AnyResult<()> {
        let (x, y, w, h) = bounds;
        log::debug!(target: "wgpu_renderer", ">>> draw_texture_quad_with_bind_group: bounds=({x}, {y}, {w}, {h})");

        // Build a quad covering the group's bounds with UVs 0..1 over the offscreen texture
        let fw = self.size.width.max(1) as f32;
        let fh = self.size.height.max(1) as f32;
        let x0 = (x / fw) * 2.0 - 1.0;
        let x1 = ((x + w) / fw) * 2.0 - 1.0;
        let y0 = 1.0 - (y / fh) * 2.0;
        let y1 = 1.0 - ((y + h) / fh) * 2.0;
        // UVs cover the full offscreen texture [0,1]
        let u0 = 0.0;
        let v0 = 0.0;
        let u1 = 1.0;
        let v1 = 1.0;
        let verts = [
            TexVertex {
                pos: [x0, y0],
                uv: [u0, v1],
            },
            TexVertex {
                pos: [x1, y0],
                uv: [u1, v1],
            },
            TexVertex {
                pos: [x1, y1],
                uv: [u1, v0],
            },
            TexVertex {
                pos: [x0, y0],
                uv: [u0, v1],
            },
            TexVertex {
                pos: [x1, y1],
                uv: [u1, v0],
            },
            TexVertex {
                pos: [x0, y1],
                uv: [u0, v0],
            },
        ];
        let vb = self.device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("opacity-quad-vertices"),
            contents: bytemuck::cast_slice(&verts),
            usage: BufferUsages::VERTEX,
        });
        self.live_buffers.push(vb.clone());

        // Constrain drawing to the group's bounds to avoid edge bleed and match compositing region.
        pass.set_pipeline(&self.tex_pipeline);

        let sx = x.max(0.0).floor() as u32;
        let sy = y.max(0.0).floor() as u32;
        let sw = w.max(0.0).ceil() as u32;
        let sh = h.max(0.0).ceil() as u32;

        let fw_u = self.size.width.max(1);
        let fh_u = self.size.height.max(1);
        let rx = sx.min(fw_u);
        let ry = sy.min(fh_u);
        let rw = sw.min(fw_u.saturating_sub(rx));
        let rh = sh.min(fh_u.saturating_sub(ry));
        if rw == 0 || rh == 0 {
            // Nothing visible to draw; avoid zero-sized scissor which can invalidate encoder on some backends.
            return Ok(());
        }
        pass.set_scissor_rect(rx, ry, rw, rh);
        pass.set_vertex_buffer(0, vb.slice(..));
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..6, 0..1);
        Ok(())
    }

    #[inline]
    fn compute_items_bounds(&self, items: &[DisplayItem]) -> Option<Bounds> {
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for it in items {
            match it {
                DisplayItem::Rect {
                    x,
                    y,
                    width,
                    height,
                    ..
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
                    if let Some((l, t, r, b)) = bounds {
                        let lx = *l as f32;
                        let ty = *t as f32;
                        let rx = *r as f32;
                        let by = *b as f32;
                        min_x = min_x.min(lx);
                        min_y = min_y.min(ty);
                        max_x = max_x.max(rx);
                        max_y = max_y.max(by);
                    } else {
                        // Conservative fallback: treat a line box around baseline
                        let h = (*font_size).max(1.0);
                        let w = (*font_size) * 4.0; // rough estimate
                        min_x = min_x.min(*x);
                        min_y = min_y.min(*y - h);
                        max_x = max_x.max(*x + w);
                        max_y = max_y.max(*y);
                    }
                }
                _ => {}
            }
        }
        if min_x.is_finite() {
            Some((
                min_x.max(0.0),
                min_y.max(0.0),
                (max_x - min_x).max(0.0),
                (max_y - min_y).max(0.0),
            ))
        } else {
            None
        }
    }

    /// Create the GPU device/surface and initialize a simple render pipeline.
    pub async fn new(window: Arc<Window>) -> RenderState {
        // Enable DX12 validation layer for detailed error messages
        let instance = Instance::new(&InstanceDescriptor {
            backends: Backends::DX12 | Backends::GL,
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
            .unwrap();
        // Enable validation layers in debug builds for better error reporting
        let device_descriptor = DeviceDescriptor {
            label: Some("valor-render-device"),
            required_features: Features::empty(),
            required_limits: Limits::default(),
            memory_hints: MemoryHints::default(),
            trace: Default::default(),
        };

        let (device, queue) = adapter.request_device(&device_descriptor).await.unwrap();

        // Set up error callback for better debugging
        device.on_uncaptured_error(Box::new(|error| {
            log::error!(target: "wgpu_renderer", "Uncaptured WGPU error: {error:?}");
        }));

        // Wrap device in Arc for safe shared ownership and to eliminate unsafe code
        let device = Arc::new(device);

        let size = window.inner_size();

        // Try to create a surface; if capabilities are empty, fall back to offscreen mode.
        let (surface_opt, surface_format, render_format) = match instance
            .create_surface(window.clone())
        {
            Ok(surface) => {
                let capabilities = surface.get_capabilities(&adapter);
                if capabilities.formats.is_empty() {
                    // Headless path: no surface formats available
                    (
                        None,
                        TextureFormat::Rgba8Unorm,
                        TextureFormat::Rgba8UnormSrgb,
                    )
                } else {
                    // Prefer RGBA8 for consistent channel ordering; fall back to the first available format
                    let sfmt = capabilities
                        .formats
                        .iter()
                        .copied()
                        .find(|f| {
                            matches!(f, TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb)
                        })
                        .unwrap_or(capabilities.formats[0]);
                    // Use the base (non-sRGB) surface format, but render to an sRGB view
                    let surface_fmt = match sfmt {
                        TextureFormat::Rgba8UnormSrgb => TextureFormat::Rgba8Unorm,
                        other => other,
                    };
                    let render_fmt = TextureFormat::Rgba8UnormSrgb;
                    // Configure the surface before creating the pipeline
                    let surface_config = SurfaceConfiguration {
                        usage: TextureUsages::RENDER_ATTACHMENT,
                        format: surface_fmt,
                        // Request compatibility with the sRGB-format texture view we are going to create later.
                        view_formats: vec![render_fmt],
                        alpha_mode: CompositeAlphaMode::Auto,
                        width: size.width,
                        height: size.height,
                        desired_maximum_frame_latency: 2,
                        present_mode: PresentMode::AutoVsync,
                    };
                    surface.configure(&device, &surface_config);
                    (Some(surface), surface_fmt, render_fmt)
                }
            }
            Err(_) => (
                None,
                TextureFormat::Rgba8Unorm,
                TextureFormat::Rgba8UnormSrgb,
            ),
        };

        // Build pipeline and buffers now that formats are known
        let (pipeline, vertex_buffer, vertex_count) =
            build_pipeline_and_buffers(&device, render_format);
        let (tex_pipeline, tex_bind_layout, linear_sampler) =
            build_texture_pipeline(&device, render_format);

        // Initialize glyphon text subsystem
        let glyphon_cache_local = Cache::new(&device);
        let mut text_atlas_local =
            TextAtlas::new(&device, &queue, &glyphon_cache_local, render_format);
        let text_renderer_local = TextRenderer::new(
            &mut text_atlas_local,
            &device,
            MultisampleState::default(),
            None,
        );
        let mut viewport_local = Viewport::new(&device, &glyphon_cache_local);
        viewport_local.update(
            &queue,
            Resolution {
                width: size.width,
                height: size.height,
            },
        );

        // Ensure system fonts are available for glyphon
        let mut font_system_runtime = FontSystem::new();
        font_system_runtime.db_mut().load_system_fonts();

        // Log any uncaptured WGPU errors to help diagnose backend issues
        device.on_uncaptured_error(Box::new(|e| {
            log::error!(target: "wgpu_renderer", "WGPU uncaptured error: {e:?}");
        }));

        RenderState {
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
            // Glyphon text state
            font_system: font_system_runtime,
            swash_cache: SwashCache::new(),
            text_atlas: text_atlas_local,
            text_renderer: text_renderer_local,
            #[allow(dead_code)]
            glyphon_cache: glyphon_cache_local,
            viewport: viewport_local,
            layers: Vec::new(),
            clear_color: [1.0, 1.0, 1.0, 1.0],
            offscreen_tex: None,
            readback_buf: None,
            readback_padded_bpr: 0,
            readback_size: 0,
            live_textures: Vec::new(),
            live_buffers: Vec::new(),
        }
    }

    /// Window getter for integrations that require it.
    pub fn get_window(&self) -> &Window {
        &self.window
    }

    /// Set the framebuffer clear color (canvas background). RGBA in [0,1].
    pub fn set_clear_color(&mut self, rgba: [f32; 4]) {
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
        if let Some(s) = &self.surface {
            s.configure(&self.device, &surface_config);
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

    /// Clear any compositor layers; subsequent render() will use the single retained list if set.
    pub fn clear_layers(&mut self) {
        self.layers.clear();
    }

    /// Reset rendering state for the next frame. Critical for test isolation and preventing
    /// state corruption when reusing RenderState across multiple renders.
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
        let _ = self.device.poll(wgpu::PollType::Wait);

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
            if let Err(e) = scope.check() {
                log::error!(target: "wgpu_renderer", "Glyphon text_atlas.trim() generated validation error: {e:?}");
            }
        }

        // Recreate text renderer to prevent glyphon state corruption after opacity compositing
        // This is critical because glyphon maintains internal GPU state that can become invalid
        {
            let scope = ErrorScopeGuard::push(&self.device, "glyphon-renderer-recreate");
            self.text_renderer = TextRenderer::new(
                &mut self.text_atlas,
                &self.device,
                wgpu::MultisampleState::default(),
                None,
            );
            if let Err(e) = scope.check() {
                log::error!(target: "wgpu_renderer", "Glyphon TextRenderer::new() generated validation error: {e:?}");
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
    /// When set, render() will prefer drawing directly from the retained list
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
    pub fn render(&mut self) -> Result<(), anyhow::Error> {
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
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.push_debug_group("onscreen-frame");
        let used_encoder = self.record_draw_passes(&texture_view, &mut encoder, false, true)?; // allow_split=true for swapchain
        if used_encoder {
            encoder.pop_debug_group();
            // Catch finish-time errors with proper error scope management
            let cb = {
                let scope = ErrorScopeGuard::push(&self.device, "encoder.finish(main)");
                let command_buffer = encoder.finish();
                scope.check()?;
                command_buffer
            };
            submit_with_validation(&self.device, &self.queue, [cb])?;
        }
        // If encoder wasn't used, split path already handled submission
        // After submission, it's safe to drop per-frame resources
        self.live_textures.clear();
        self.live_buffers.clear();
        self.window.pre_present_notify();
        surface_texture.present();
        Ok(())
    }

    /// Render a frame and return the framebuffer RGBA bytes.
    /// Uses split encoder pattern for D3D12 compatibility when opacity groups are present.
    pub fn render_to_rgba(&mut self) -> Result<Vec<u8>, anyhow::Error> {
        // Always render to an offscreen texture so we can COPY_SRC safely. Reuse across calls.
        // Ensure previous-frame resources are dropped before starting
        self.live_textures.clear();
        self.live_buffers.clear();

        // Verify device is in clean state
        {
            let validation_scope = ErrorScopeGuard::push(&self.device, "pre-render-validation");
            validation_scope.check()?;
        }

        // (Re)create offscreen texture/view if missing or size changed
        let fb_w = self.size.width.max(1);
        let fb_h = self.size.height.max(1);
        let need_offscreen = match &self.offscreen_tex {
            None => true,
            Some(tex) => {
                let s = tex.size();
                s.width != fb_w || s.height != fb_h || s.depth_or_array_layers != 1
            }
        };
        if need_offscreen {
            let fb_w = self.size.width.max(1);
            let fb_h = self.size.height.max(1);
            // Use non-sRGB base with sRGB view for rendering
            let base_format = match self.render_format {
                TextureFormat::Rgba8UnormSrgb => TextureFormat::Rgba8Unorm,
                TextureFormat::Bgra8UnormSrgb => TextureFormat::Bgra8Unorm,
                other => other,
            };
            let tex = self.device.create_texture(&TextureDescriptor {
                label: Some("offscreen-target"),
                size: Extent3d {
                    width: fb_w,
                    height: fb_h,
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
        // Create a transient view from the cached offscreen texture to avoid borrowing self immutably
        let tmp_view = self
            .offscreen_tex
            .as_ref()
            .expect("offscreen tex available")
            .create_view(&TextureViewDescriptor {
                format: Some(self.render_format),
                ..Default::default()
            });
        // Check if we need opacity compositing (requires split encoder for D3D12 resource transitions)
        let use_retained = true;
        let needs_opacity = self.has_opacity_groups(use_retained);

        if needs_opacity {
            // Use split encoder pattern: Phase 1 renders opacity groups, Phase 2 uses them
            // This allows D3D12 to transition resources from RENDER_TARGET to PIXEL_SHADER_RESOURCE
            log::debug!(target: "wgpu_renderer", "Opacity groups detected, using split encoder pattern");
            self.render_to_rgba_with_opacity(&tmp_view)?;
        } else {
            // No opacity groups: use simple single encoder path
            log::debug!(target: "wgpu_renderer", "No opacity groups, using single encoder");
            let mut encoder = self
                .device
                .create_command_encoder(&CommandEncoderDescriptor {
                    label: Some("render-to-rgba-encoder"),
                });
            self.record_draw_passes(&tmp_view, &mut encoder, true, false)?;
            let cb = encoder.finish();
            submit_with_validation(&self.device, &self.queue, [cb])?;
        }

        // Drop per-frame resources after rendering complete
        // Drop per-frame resources after submission
        self.live_textures.clear();
        self.live_buffers.clear();

        // Create a new encoder for the texture copy operation
        let mut copy_encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("texture-copy-encoder"),
            });

        // Read back with 256-byte aligned rows
        let width = self.size.width.max(1);
        let height = self.size.height.max(1);
        let bpp = 4u32;
        let row_bytes = width * bpp;
        let padded_bpr = row_bytes.div_ceil(256) * 256;
        let buffer_size = (padded_bpr as u64) * (height as u64);
        // (Re)create readback buffer if missing or bytes-per-row changed or too small
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
        copy_encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: self
                    .offscreen_tex
                    .as_ref()
                    .expect("offscreen tex available"),
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            TexelCopyBufferInfo {
                buffer: self
                    .readback_buf
                    .as_ref()
                    .expect("readback buffer available"),
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

        // Submit the copy command buffer with proper error scope management
        let copy_command_buffer = {
            let scope = ErrorScopeGuard::push(&self.device, "copy_encoder.finish");
            let cb = copy_encoder.finish();
            scope.check()?;
            cb
        };
        submit_with_validation(&self.device, &self.queue, [copy_command_buffer])?;

        // Map and wait for the copy to be available
        let readback = self
            .readback_buf
            .as_ref()
            .expect("readback buffer available");
        let slice = readback.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(MapMode::Read, move |res| {
            let _ = sender.send(res);
        });
        loop {
            let _ = self.device.poll(wgpu::PollType::Wait);
            if let Ok(res) = receiver.try_recv() {
                res?;
                break;
            }
        }
        let mapped = slice.get_mapped_range();
        // Ensure we create a buffer of exactly the expected size
        let expected_total_bytes = (width as usize) * (height as usize) * (bpp as usize);
        let mut out = vec![0u8; expected_total_bytes];
        for row in 0..height as usize {
            let src_off = row * (padded_bpr as usize);
            let dst_off = row * (row_bytes as usize);
            out[dst_off..dst_off + (row_bytes as usize)]
                .copy_from_slice(&mapped[src_off..src_off + (row_bytes as usize)]);
        }
        drop(mapped);
        readback.unmap();
        // If our render target format uses BGRA ordering, convert to RGBA for consumers
        match self.render_format {
            TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => {
                for px in out.chunks_exact_mut(4) {
                    let b = px[0];
                    let r = px[2];
                    px[0] = r;
                    px[2] = b;
                }
            }
            _ => {}
        }
        Ok(out)
    }

    /// Render to RGBA with opacity groups using split encoder pattern for D3D12 compatibility.
    /// Phase 1: Render opacity groups to offscreen textures and submit.
    /// Phase 2: Create bind groups and render main pass using those textures.
    fn render_to_rgba_with_opacity(&mut self, texture_view: &TextureView) -> AnyResult<()> {
        log::debug!(target: "wgpu_renderer", "=== PHASE 1: Rendering opacity groups ===");

        // Phase 1: Create encoder for offscreen opacity rendering
        let mut offscreen_encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("offscreen-opacity-encoder"),
            });

        // Collect and render opacity composites (WITHOUT creating bind groups yet)
        let comps_no_bindings = if let Some(dl) = self.retained_display_list.clone() {
            let items: Vec<DisplayItem> = dl.items;
            self.collect_opacity_composites(&mut offscreen_encoder, &items)?
        } else {
            vec![]
        };

        // Finish and submit Phase 1 encoder
        // This allows D3D12 to transition offscreen textures from RENDER_TARGET to PIXEL_SHADER_RESOURCE
        let offscreen_cb = offscreen_encoder.finish();
        submit_with_validation(&self.device, &self.queue, [offscreen_cb])?;
        log::debug!(target: "wgpu_renderer", "PHASE 1 complete: {} opacity groups rendered, submitting...", comps_no_bindings.len());
        log::debug!(target: "wgpu_renderer", "PHASE 1 submitted successfully");

        // Wait for Phase 1 GPU work to complete before starting Phase 2
        // This ensures D3D12 resource transitions are fully processed
        let _ = self.device.poll(wgpu::PollType::Wait);
        log::debug!(target: "wgpu_renderer", "PHASE 1 GPU work complete");

        // PHASE 2: Create bind groups and render main pass + text pass using offscreen textures
        log::debug!(target: "wgpu_renderer", "PHASE 2: Creating bind groups and rendering main pass");
        let mut main_encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("main-and-text-encoder"),
            });

        // Clear pass
        {
            debug!(target: "wgpu_renderer", "start clear-pass");
            let scope = ErrorScopeGuard::push(&self.device, "clear-pass");
            {
                let _clear_pass = main_encoder.begin_render_pass(&RenderPassDescriptor {
                    label: Some("clear-pass"),
                    color_attachments: &[Some(RenderPassColorAttachment {
                        view: texture_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: Operations {
                            load: LoadOp::Clear(Color {
                                r: self.clear_color[0] as f64,
                                g: self.clear_color[1] as f64,
                                b: self.clear_color[2] as f64,
                                a: self.clear_color[3] as f64,
                            }),
                            store: StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
            }
            scope.check()?;
            debug!(target: "wgpu_renderer", "end clear-pass");
        }

        // Main pass with opacity compositing
        if let Some(dl) = self.retained_display_list.clone() {
            let items: Vec<DisplayItem> = dl.items;
            // No offscreen rendering in single-encoder path
            {
                let scope = ErrorScopeGuard::push(&self.device, "main-pass-retained");
                self.render_retained_pass(&mut main_encoder, texture_view, LoadOp::Load, &items, comps_no_bindings)?;
                scope.check()?;
            }
        }

        // Text pass
        {
            let scope = ErrorScopeGuard::push(&self.device, "text-pass");
            {
                let mut pass = main_encoder.begin_render_pass(&RenderPassDescriptor {
                    label: Some("text-pass"),
                    color_attachments: &[Some(RenderPassColorAttachment {
                        view: texture_view,
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
                debug!(target: "wgpu_renderer", "start text-pass");
                pass.push_debug_group("text-pass");
                if let Some(dl) = &self.retained_display_list {
                    let batches = batch_texts_with_scissor(dl, self.size.width, self.size.height);
                    self.draw_text_batches(&mut pass, batches);
                }
                pass.pop_debug_group();
                debug!(target: "wgpu_renderer", "end text-pass");
            }
            scope.check()?;
        }

        let main_cb = {
            let scope = ErrorScopeGuard::push(&self.device, "main-encoder.finish");
            let cb = main_encoder.finish();
            scope.check()?;
            cb
        };

        log::debug!(target: "wgpu_renderer", "PHASE 2 complete, submitting...");
        submit_with_validation(&self.device, &self.queue, [main_cb])?;
        log::debug!(target: "wgpu_renderer", "PHASE 2 submitted successfully");

        Ok(())
    }
}

/// Pixel bounds (x, y, width, height)
pub(crate) type Bounds = (f32, f32, f32, f32);
