//! Opacity compositing and offscreen rendering.
//!
//! This component handles all opacity-related rendering operations including:
//! - Collecting opacity composites from display items
//! - Rendering items to offscreen textures with alpha blending
//! - Two-phase opacity rendering (extract + composite)
//! - Managing offscreen texture lifecycle

use super::error_scope::ErrorScopeGuard;
use super::gpu_context::GpuContext;
use super::offscreen_target::OffscreenTarget;
use super::pipeline_cache::PipelineCache;
use super::rectangle_renderer::RectangleRenderer;
use super::resource_tracker::ResourceTracker;
use super::text_renderer_state::TextRendererState;
use super::{
    Bounds, OffscreenRenderParams, OpacityComposite, RenderContext, ScissorRect, TexVertex,
};
use crate::error::submit_with_validation;
use crate::pipelines::Vertex;
use crate::text::map_text_item;
use anyhow::Result as AnyResult;
use bytemuck::cast_slice;
use log::{debug, error};
use renderer::display_list::{
    DisplayItem, DisplayList, StackingContextBoundary, batch_display_list,
};
use renderer::renderer::DrawText;
use wgpu::util::DeviceExt as _;
use wgpu::*;
use winit::dpi::PhysicalSize;

/// Pre-rendered opacity layer for two-phase compositing.
pub struct OpacityLayer {
    pub(crate) bounds: Bounds,
    pub(crate) texture: Texture,
    pub(crate) bind_group: BindGroup,
}

/// Result of extracting opacity layers from a display list.
pub struct OpacityExtraction {
    pub(crate) layers: Vec<OpacityLayer>,
    pub(crate) clean_items: Vec<DisplayItem>,
}

/// Component responsible for opacity compositing and offscreen rendering.
///
/// This component extracts opacity groups from display lists, renders them
/// to offscreen textures with alpha blending, and composites them back into
/// the main framebuffer.
pub struct OpacityCompositor<'state> {
    pub(super) gpu: &'state GpuContext,
    pub(super) pipelines: &'state PipelineCache,
    pub(super) text: &'state mut TextRendererState,
    pub(super) _rectangles: &'state mut RectangleRenderer,
    pub(super) _offscreen: &'state OffscreenTarget,
    pub(super) resources: &'state mut ResourceTracker,
}

impl OpacityCompositor<'_> {
    /// Find the matching end marker for a stacking context.
    pub(crate) fn find_stacking_context_end(items: &[DisplayItem], start: usize) -> usize {
        let mut depth = 1usize;
        for (index, item) in items.iter().enumerate().skip(start) {
            match item {
                DisplayItem::BeginStackingContext { .. } => depth += 1,
                DisplayItem::EndStackingContext => {
                    depth -= 1;
                    if depth == 0 {
                        return index;
                    }
                }
                _ => {}
            }
        }
        items.len()
    }

    /// Compute the bounding box of a set of display items.
    pub(crate) fn compute_items_bounds(items: &[DisplayItem]) -> Option<Bounds> {
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
                    ..
                } => {
                    min_x = min_x.min(*x);
                    min_y = min_y.min(*y);
                    max_x = max_x.max(x + width);
                    max_y = max_y.max(y + height);
                }
                DisplayItem::Text { x, y, bounds, .. } => {
                    if let Some((left, top, right, bottom)) = bounds {
                        min_x = min_x.min(*left as f32);
                        min_y = min_y.min(*top as f32);
                        max_x = max_x.max(*right as f32);
                        max_y = max_y.max(*bottom as f32);
                    } else {
                        min_x = min_x.min(*x);
                        min_y = min_y.min(*y);
                        max_x = max_x.max(x + 100.0);
                        max_y = max_y.max(y + 20.0);
                    }
                }
                _ => {}
            }
        }

        (min_x.is_finite() && max_x.is_finite() && min_y.is_finite() && max_y.is_finite()).then(
            || {
                let width = (max_x - min_x).max(1.0);
                let height = (max_y - min_y).max(1.0);
                (min_x, min_y, width, height)
            },
        )
    }

    /// Collect opacity composites from display items for offscreen rendering.
    pub(crate) fn collect_opacity_composites(
        &mut self,
        encoder: &mut CommandEncoder,
        items: &[DisplayItem],
    ) -> Vec<OpacityComposite> {
        let mut out: Vec<OpacityComposite> = Vec::new();
        let mut index = 0usize;
        while index < items.len() {
            if let DisplayItem::BeginStackingContext { boundary } = &items[index]
                && matches!(boundary, StackingContextBoundary::Opacity { alpha } if *alpha < 1.0)
            {
                let end = Self::find_stacking_context_end(items, index + 1);
                let group_items = &items[index + 1..end];
                let alpha = match boundary {
                    StackingContextBoundary::Opacity { alpha } => *alpha,
                    _ => 1.0,
                };
                let bounds =
                    Self::compute_items_bounds(group_items).unwrap_or((0.0, 0.0, 1.0, 1.0));
                let (tex, view, tex_width, tex_height, bind_group) = self
                    .render_items_to_offscreen_bounded_with_bind_group(
                        encoder,
                        group_items,
                        bounds,
                        alpha,
                    );
                out.push((
                    index, end, tex, view, tex_width, tex_height, alpha, bounds, bind_group,
                ));
                index = end + 1;
                continue;
            }
            index += 1;
        }
        out
    }

    /// Extract and render opacity layers for two-phase compositing.
    ///
    /// # Errors
    /// Returns an error if opacity layer rendering or submission fails.
    pub(crate) fn extract_and_render_opacity_layers(
        &mut self,
        _main_encoder: &mut CommandEncoder,
        items: &[DisplayItem],
    ) -> AnyResult<OpacityExtraction> {
        let mut layers = Vec::new();
        let mut clean_items = Vec::new();
        let mut index = 0usize;

        debug!(target: "wgpu_renderer", "=== PHASE 1: Extracting and pre-rendering opacity layers ===");
        debug!(target: "wgpu_renderer", "    Using dedicated encoder per opacity group with immediate submission");

        while index < items.len() {
            match &items[index] {
                DisplayItem::BeginStackingContext { boundary } => {
                    if let StackingContextBoundary::Opacity { alpha } = boundary
                        && *alpha < 1.0
                    {
                        let (layer, next_index) =
                            self.process_opacity_group(items, index, *alpha)?;

                        let bounds = layer.bounds;
                        clean_items.push(DisplayItem::Rect {
                            x: bounds.0,
                            y: bounds.1,
                            width: 0.0,
                            height: 0.0,
                            color: [0.0, 0.0, 0.0, 0.0],
                        });

                        layers.push(layer);
                        index = next_index;
                        continue;
                    }

                    index += 1;
                }
                DisplayItem::EndStackingContext => {
                    index += 1;
                }
                other_item => {
                    clean_items.push(other_item.clone());
                    index += 1;
                }
            }
        }

        debug!(target: "wgpu_renderer", "=== PHASE 1 COMPLETE: {} layers extracted ===", layers.len());
        Ok(OpacityExtraction {
            layers,
            clean_items,
        })
    }

    /// Process a single opacity group: render it offscreen and return the layer data.
    ///
    /// # Errors
    /// Returns an error if offscreen rendering or submission fails.
    fn process_opacity_group(
        &mut self,
        items: &[DisplayItem],
        index: usize,
        alpha: f32,
    ) -> AnyResult<(OpacityLayer, usize)> {
        let start_index = index;
        let end = Self::find_stacking_context_end(items, index + 1);
        let group_items_raw = &items[index + 1..end];

        let group_items_clean = Self::remove_stacking_markers(group_items_raw);

        debug!(target: "wgpu_renderer", "  Found opacity group: index={}, alpha={}, raw_items={}, clean_items={}",
               start_index, alpha, group_items_raw.len(), group_items_clean.len());

        let bounds = Self::compute_items_bounds(&group_items_clean).unwrap_or((0.0, 0.0, 1.0, 1.0));

        let mut offscreen_encoder =
            self.gpu
                .device()
                .create_command_encoder(&CommandEncoderDescriptor {
                    label: Some("opacity-group-encoder"),
                });

        let (texture, _view, _tex_width, _tex_height, bind_group) = self
            .render_items_to_offscreen_bounded_with_bind_group(
                &mut offscreen_encoder,
                &group_items_clean,
                bounds,
                alpha,
            );

        let offscreen_cmd_buf = offscreen_encoder.finish();
        submit_with_validation(self.gpu.device(), self.gpu.queue(), [offscreen_cmd_buf])?;

        Ok((
            OpacityLayer {
                bounds,
                texture,
                bind_group,
            },
            end + 1,
        ))
    }

    /// Remove all stacking context markers from a list of display items.
    fn remove_stacking_markers(items: &[DisplayItem]) -> Vec<DisplayItem> {
        items
            .iter()
            .filter(|item| {
                !matches!(
                    item,
                    DisplayItem::BeginStackingContext { .. } | DisplayItem::EndStackingContext
                )
            })
            .cloned()
            .collect()
    }

    /// Render items to offscreen texture with bind group for opacity compositing.
    fn render_items_to_offscreen_bounded_with_bind_group(
        &mut self,
        encoder: &mut CommandEncoder,
        items: &[DisplayItem],
        bounds: (f32, f32, f32, f32),
        alpha: f32,
    ) -> (Texture, TextureView, u32, u32, BindGroup) {
        let (x, y, width, height) = bounds;
        let tex_width = (width.ceil() as u32).max(1);
        let tex_height = (height.ceil() as u32).max(1);

        debug!(target: "wgpu_renderer", "render_items_to_offscreen_bounded: bounds=({}, {}, {}, {}), tex_size={}x{}, items={}",
            x, y, width, height, tex_width, tex_height, items.len());

        let texture = self.create_offscreen_texture(tex_width, tex_height);
        let view = texture.create_view(&TextureViewDescriptor {
            label: Some("offscreen-opacity-view"),
            format: Some(TextureFormat::Rgba8Unorm),
            ..Default::default()
        });

        let ctx = RenderContext::new(PhysicalSize::new(tex_width, tex_height));
        let translated_items = Self::translate_items_to_local(items, x, y);

        let text_items: Vec<DrawText> = translated_items.iter().filter_map(map_text_item).collect();
        if !text_items.is_empty() {
            debug!(target: "wgpu_renderer", "Pre-preparing glyphon for {} text items before encoder operations", text_items.len());
            self.glyphon_prepare_for(&text_items);
        }

        self.render_offscreen_rects_pass(&mut OffscreenRenderParams {
            encoder,
            view: &view,
            translated_items: &translated_items,
            tex_width,
            tex_height,
            ctx,
        });
        self.render_offscreen_text_pass(&mut OffscreenRenderParams {
            encoder,
            view: &view,
            translated_items: &translated_items,
            tex_width,
            tex_height,
            ctx,
        });

        debug!(target: "wgpu_renderer", "Offscreen render passes complete, creating bind group");
        let bind_group = self.create_opacity_bind_group(&view, alpha);
        debug!(target: "wgpu_renderer", "Bind group created, texture ready for compositing");

        (texture, view, tex_width, tex_height, bind_group)
    }

    /// Create offscreen texture for opacity rendering.
    fn create_offscreen_texture(&self, tex_width: u32, tex_height: u32) -> Texture {
        let offscreen_format = TextureFormat::Rgba8Unorm;
        self.gpu.device().create_texture(&TextureDescriptor {
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
                    font_weight,
                    font_family,
                    line_height,
                    bounds: text_bounds,
                } => DisplayItem::Text {
                    x: text_x - offset_x,
                    y: text_y - offset_y,
                    text: text.clone(),
                    color: *color,
                    font_size: *font_size,
                    font_weight: *font_weight,
                    font_family: font_family.clone(),
                    line_height: *line_height,
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

    /// Render rectangles to offscreen texture.
    fn render_offscreen_rects_pass(&mut self, params: &mut OffscreenRenderParams<'_>) {
        debug!(target: "wgpu_renderer", ">>> CREATING offscreen rects pass");
        {
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
            debug!(target: "wgpu_renderer", "    Pass created, setting viewport and pipeline");
            pass.set_viewport(
                0.0,
                0.0,
                params.tex_width as f32,
                params.tex_height as f32,
                0.0,
                1.0,
            );
            pass.set_pipeline(self.pipelines.offscreen_pipeline());
            debug!(target: "wgpu_renderer", "    Drawing items (batched, no nested stacking contexts)");

            self.draw_items_batched_with_size(
                &mut pass,
                params.translated_items,
                params.tex_width,
                params.tex_height,
            );
            debug!(target: "wgpu_renderer", "    Pass ending...");
        };
    }

    /// Render text to offscreen texture.
    fn render_offscreen_text_pass(&self, params: &mut OffscreenRenderParams<'_>) {
        let text_items: Vec<DrawText> = params
            .translated_items
            .iter()
            .filter_map(map_text_item)
            .collect();
        if text_items.is_empty() {
            return;
        }

        debug!(target: "wgpu_renderer", ">>> CREATING offscreen text pass");
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
        debug!(target: "wgpu_renderer", "    Pass created, drawing text");
        text_pass.set_viewport(
            0.0,
            0.0,
            params.tex_width as f32,
            params.tex_height as f32,
            0.0,
            1.0,
        );
        self.draw_text_batch_ctx(&mut text_pass, text_items.as_slice(), None, params.ctx);
    }

    /// Context-aware version of `draw_text_batch`.
    fn draw_text_batch_ctx(
        &self,
        pass: &mut RenderPass<'_>,
        _text_items: &[DrawText],
        scissor: Option<ScissorRect>,
        ctx: RenderContext,
    ) {
        pass.set_viewport(0.0, 0.0, ctx.width() as f32, ctx.height() as f32, 0.0, 1.0);
        match scissor {
            Some((x, y, width, height)) => pass.set_scissor_rect(x, y, width, height),
            None => pass.set_scissor_rect(0, 0, ctx.width().max(1), ctx.height().max(1)),
        }
        {
            let scope = ErrorScopeGuard::push(self.gpu.device(), "glyphon-text-render");
            if let Err(error) = self.text.render(self.gpu.device(), pass) {
                error!(target: "wgpu_renderer", "Glyphon text_renderer.render() failed: {error:?}");
            }
            if let Err(error) = scope.check() {
                error!(target: "wgpu_renderer", "Glyphon text_renderer.render() generated validation error: {error:?}");
            }
        }
    }

    /// Create bind group for opacity compositing with alpha blending.
    fn create_opacity_bind_group(&mut self, view: &TextureView, alpha: f32) -> BindGroup {
        let alpha_buf = self
            .gpu
            .device()
            .create_buffer_init(&util::BufferInitDescriptor {
                label: Some("opacity-alpha"),
                contents: cast_slice(&[
                    alpha, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32,
                ]),
                usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            });
        self.resources.track_buffer(alpha_buf.clone());

        self.gpu.device().create_bind_group(&BindGroupDescriptor {
            label: Some("opacity-tex-bind"),
            layout: self.pipelines.texture_bind_layout(),
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(self.pipelines.linear_sampler()),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: alpha_buf.as_entire_binding(),
                },
            ],
        })
    }

    /// Draw a textured quad using a pre-created bind group.
    pub(crate) fn draw_texture_quad_with_bind_group(
        &mut self,
        pass: &mut RenderPass<'_>,
        bind_group: &BindGroup,
        bounds: Bounds,
    ) {
        let (rect_x, rect_y, rect_width, rect_height) = bounds;
        debug!(target: "wgpu_renderer", ">>> draw_texture_quad_with_bind_group: bounds=({rect_x}, {rect_y}, {rect_width}, {rect_height})");

        let framebuffer_width = self.gpu.size().width.max(1) as f32;
        let framebuffer_height = self.gpu.size().height.max(1) as f32;
        let x0 = (rect_x / framebuffer_width).mul_add(2.0, -1.0);
        let x1 = ((rect_x + rect_width) / framebuffer_width).mul_add(2.0, -1.0);
        let y0 = (rect_y / framebuffer_height).mul_add(-2.0, 1.0);
        let y1 = ((rect_y + rect_height) / framebuffer_height).mul_add(-2.0, 1.0);
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
        let vertex_buffer = self
            .gpu
            .device()
            .create_buffer_init(&util::BufferInitDescriptor {
                label: Some("opacity-quad-vertices"),
                contents: cast_slice(&verts),
                usage: BufferUsages::VERTEX,
            });
        self.resources.track_buffer(vertex_buffer.clone());

        pass.set_pipeline(self.pipelines.texture_pipeline());

        let scissor_x = rect_x.max(0.0).floor() as u32;
        let scissor_y = rect_y.max(0.0).floor() as u32;
        let scissor_width = rect_width.max(0.0).ceil() as u32;
        let scissor_height = rect_height.max(0.0).ceil() as u32;

        let clipped_x = scissor_x.min(self.gpu.size().width);
        let clipped_y = scissor_y.min(self.gpu.size().height);
        let clipped_width = scissor_width.min(self.gpu.size().width.saturating_sub(clipped_x));
        let clipped_height = scissor_height.min(self.gpu.size().height.saturating_sub(clipped_y));

        if clipped_width == 0 || clipped_height == 0 {
            debug!(target: "wgpu_renderer", ">>> Skipping draw: scissor rect is empty after clipping");
            return;
        }

        pass.set_scissor_rect(clipped_x, clipped_y, clipped_width, clipped_height);
        pass.set_bind_group(0, bind_group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..6, 0..1);
        debug!(target: "wgpu_renderer", ">>> Textured quad drawn");
    }

    /// Prepare glyphon buffers for a specific set of text items.
    fn glyphon_prepare_for(&mut self, items: &[DrawText]) {
        let scale = self.gpu.window().scale_factor() as f32;
        self.text.prepare(
            self.gpu.device(),
            self.gpu.queue(),
            items,
            (self.gpu.size(), scale),
        );
    }

    /// Draw display items in batches using specified viewport size.
    #[inline]
    fn draw_items_batched_with_size(
        &mut self,
        pass: &mut RenderPass<'_>,
        items: &[DisplayItem],
        width: u32,
        height: u32,
    ) {
        let sub = DisplayList::from_items(items.to_vec());
        let batches = batch_display_list(&sub, width, height);
        for batch in batches {
            let mut vertices: Vec<Vertex> = Vec::with_capacity(batch.quads.len() * 6);
            for quad in &batch.quads {
                Self::push_rect_vertices_ndc_with_size(
                    &mut vertices,
                    [quad.x, quad.y, quad.width, quad.height],
                    quad.color,
                    (width, height),
                );
            }
            let vertex_bytes = cast_slice(vertices.as_slice());
            let vertex_buffer = self
                .gpu
                .device()
                .create_buffer_init(&util::BufferInitDescriptor {
                    label: Some("layer-rect-batch"),
                    contents: vertex_bytes,
                    usage: BufferUsages::VERTEX,
                });
            self.resources.track_buffer(vertex_buffer.clone());
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            match batch.scissor {
                Some((scissor_x, scissor_y, scissor_width, scissor_height)) => {
                    let framebuffer_width = width.max(1);
                    let framebuffer_height = height.max(1);
                    let rect_x = scissor_x.min(framebuffer_width);
                    let rect_y = scissor_y.min(framebuffer_height);
                    let rect_width = scissor_width.min(framebuffer_width.saturating_sub(rect_x));
                    let rect_height = scissor_height.min(framebuffer_height.saturating_sub(rect_y));
                    if rect_width == 0 || rect_height == 0 {
                        continue;
                    }
                    pass.set_scissor_rect(rect_x, rect_y, rect_width, rect_height);
                }
                None => {
                    pass.set_scissor_rect(0, 0, width.max(1), height.max(1));
                }
            }
            if !vertices.is_empty() {
                pass.draw(0..(vertices.len() as u32), 0..1);
            }
        }
    }

    /// Push rectangle vertices in NDC coordinates with specified viewport size.
    #[inline]
    fn push_rect_vertices_ndc_with_size(
        out: &mut Vec<Vertex>,
        rect_xywh: [f32; 4],
        color: [f32; 4],
        framebuffer_size: (u32, u32),
    ) {
        let (framebuffer_width, framebuffer_height) = framebuffer_size;
        let framebuffer_width = framebuffer_width.max(1) as f32;
        let framebuffer_height = framebuffer_height.max(1) as f32;
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
