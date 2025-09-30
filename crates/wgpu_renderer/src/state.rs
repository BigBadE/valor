use crate::display_list::{DisplayItem, DisplayList, StackingContextBoundary, batch_display_list};
use crate::error::submit_with_validation;
use crate::pipelines::{Vertex, build_pipeline_and_buffers, build_texture_pipeline};
use crate::renderer::{DrawRect, DrawText};
use crate::text::{batch_layer_texts_with_scissor, batch_texts_with_scissor, map_text_item};
use anyhow::Result as AnyResult;
use glyphon::{Cache, FontSystem, Resolution, SwashCache, TextAtlas, TextRenderer, Viewport};
use log::debug;
use std::sync::Arc;
use tracing::info_span;
use wgpu::util::DeviceExt;
use wgpu::*;
use winit::dpi::PhysicalSize;
use winit::window::Window;

// pollster is used via crate::error helpers.

// Composite info for a pre-rendered opacity group.
// (start_index, end_index, texture, texture_view, tex_w, tex_h, alpha, bounds)
pub(crate) type OpacityComposite = (usize, usize, Texture, TextureView, u32, u32, f32, Bounds);

// Result payload for offscreen renders: (texture, view, width, height)
pub(crate) type OffscreenRender = (Texture, TextureView, u32, u32);

// Compact representation for preprocessed layer data: either no content
// or (items, composites, excluded ranges).
pub(crate) type LayerEntry = Option<(Vec<DisplayItem>, Vec<OpacityComposite>, Vec<(usize, usize)>)>;

#[derive(Debug, Clone)]
pub enum Layer {
    Background,
    Content(DisplayList),
    Chrome(DisplayList),
}

/// RenderState owns the GPU device/surface and a minimal pipeline to draw rectangles from layout.
pub struct RenderState {
    pub(crate) window: Arc<Window>,
    pub(crate) device: Device,
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
    pub(crate) text_list: Vec<crate::renderer::DrawText>,
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
    offscreen_depth: u32,
    /// Persistent offscreen render target for readback-based renders
    offscreen_tex: Option<Texture>,
    /// Persistent readback buffer sized for current framebuffer (padded bytes-per-row)
    readback_buf: Option<Buffer>,
    readback_padded_bpr: u32,
    readback_size: u64,
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
                let bounds = self
                    .compute_items_bounds(group_items)
                    .unwrap_or((0.0, 0.0, 1.0, 1.0));
                let (tex, view, tw, th) =
                    self.render_items_to_offscreen_bounded(encoder, group_items, bounds)?;
                let alpha = match boundary {
                    StackingContextBoundary::Opacity { alpha } => *alpha,
                    _ => 1.0,
                };
                out.push((i, end, tex, view, tw, th, alpha, bounds));
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
                self.draw_items_batched(pass, &items[i..next]);
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
    fn composite_groups(&mut self, pass: &mut RenderPass<'_>, comps: Vec<OpacityComposite>) {
        for (_s, _e, _tex, view, _tw, _th, alpha, bounds) in comps.into_iter() {
            self.draw_texture_quad(pass, &view, alpha, bounds);
        }
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
        self.draw_items_batched(pass, group_items);
        Ok(())
    }

    /// Record all render passes (rectangles + text) into the provided texture view.
    /// This uses the exact same code paths as `render()`.
    fn record_draw_passes(
        &mut self,
        texture_view: &TextureView,
        encoder: &mut CommandEncoder,
    ) -> AnyResult<()> {
        let use_layers = !self.layers.is_empty();
        let use_retained = self.retained_display_list.is_some() && !use_layers;

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
            // Always clear first to the background color, then drop pass to allow offscreen pre-renders.
            {
                debug!(target: "wgpu_renderer", "start clear-pass");
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
                debug!(target: "wgpu_renderer", "end clear-pass");
            }

            if use_layers {
                // Pre-collect opacity composites for each layer BEFORE opening the main pass
                // to avoid creating encoders while a render pass is in progress.
                let per_layer: Vec<LayerEntry> = self
                    .layers
                    .clone()
                    .iter()
                    .map(|l| self.preprocess_layer_with_encoder(encoder, l))
                    .collect::<AnyResult<Vec<_>>>()?;

                // Open the main pass and draw layers: non-group items first, then composite groups
                let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                    label: Some("main-pass"),
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
                debug!(target: "wgpu_renderer", "start main-pass");
                pass.set_pipeline(&self.pipeline);

                for (items, comps, ranges) in per_layer.into_iter().flatten() {
                    self.draw_items_excluding_ranges(&mut pass, &items, &ranges);
                    self.composite_groups(&mut pass, comps);
                }
                debug!(target: "wgpu_renderer", "end main-pass");
            } else if use_retained {
                if let Some(dl) = self.retained_display_list.clone() {
                    let items: Vec<DisplayItem> = dl.items;
                    // Pre-render opacity groups offscreen before opening the main pass, avoiding nested encoders.
                    let comps = self.collect_opacity_composites(encoder, &items)?;
                    // Open main pass and draw non-group items, then composite groups in order.
                    let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                        label: Some("main-pass"),
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
                    debug!(target: "wgpu_renderer", "start main-pass");
                    pass.set_pipeline(&self.pipeline);

                    let ranges = self.build_exclude_ranges(&comps);
                    self.draw_items_excluding_ranges(&mut pass, &items, &ranges);
                    self.composite_groups(&mut pass, comps);
                    debug!(target: "wgpu_renderer", "end main-pass");
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
                let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                    label: Some("main-pass"),
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
                debug!(target: "wgpu_renderer", "start main-pass");
                pass.set_pipeline(&self.pipeline);
                pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                if self.vertex_count > 0 {
                    pass.draw(0..self.vertex_count, 0..1);
                }
                debug!(target: "wgpu_renderer", "end main-pass");
            }
        }

        // Second pass: text rendering
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
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
            if use_layers {
                let batches =
                    batch_layer_texts_with_scissor(&self.layers, self.size.width, self.size.height);
                self.draw_text_batches(&mut pass, batches);
            } else if use_retained && let Some(dl) = &self.retained_display_list {
                let batches = batch_texts_with_scissor(dl, self.size.width, self.size.height);
                self.draw_text_batches(&mut pass, batches);
            }
            debug!(target: "wgpu_renderer", "end text-pass");
        }
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
                            // Other stacking contexts (transforms, filters, etc.) - render normally for now
                            // TODO: Implement transform matrices and filter effects
                            self.draw_items_batched(pass, group_items);
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
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            match b.scissor {
                Some((x, y, w, h)) => pass.set_scissor_rect(x, y, w, h),
                None => {
                    pass.set_scissor_rect(0, 0, self.size.width.max(1), self.size.height.max(1))
                }
            }
            if !vertices.is_empty() {
                pass.draw(0..(vertices.len() as u32), 0..1);
            }
        }
    }

    /// Render items to offscreen texture with tight bounds and texture pooling
    /// Spec: CSS Compositing Level 1 §3.1 - Stacking context rendering optimization
    fn render_items_to_offscreen_bounded(
        &mut self,
        encoder: &mut CommandEncoder,
        items: &[DisplayItem],
        bounds: (f32, f32, f32, f32),
    ) -> AnyResult<OffscreenRender> {
        let (x, y, width, height) = bounds;
        let tex_width = (width.ceil() as u32).max(1);
        let tex_height = (height.ceil() as u32).max(1);

        // Use the same render format as the main pipeline to ensure pipeline compatibility
        let offscreen_format = self.render_format;

        // Get texture from pool or create new one with tight bounds
        let texture = self.device.create_texture(&TextureDescriptor {
            label: Some("offscreen-target"),
            size: Extent3d {
                width: tex_width,
                height: tex_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: offscreen_format,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        // Use an explicit view format matching the render pipeline to avoid backend mismatches.
        let view = texture.create_view(&TextureViewDescriptor {
            format: Some(self.render_format),
            ..Default::default()
        });

        // Increase offscreen depth to disable nested offscreen compositing
        self.offscreen_depth = self.offscreen_depth.saturating_add(1);
        // Use the provided encoder to avoid creating a nested encoder while passes are active.

        {
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

            // Set viewport to match texture bounds
            pass.set_viewport(0.0, 0.0, tex_width as f32, tex_height as f32, 0.0, 1.0);
            pass.set_pipeline(&self.pipeline);

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

            // Draw translated items (rectangles) using the same group-aware path.
            // Temporarily override framebuffer size so scissor/viewport computations
            // inside draw paths are clamped to the offscreen texture.
            let old_size = self.size;
            self.size = PhysicalSize::new(tex_width, tex_height);
            self.draw_items_with_groups(&mut pass, &translated_items)?;
            self.size = old_size;
            drop(pass);

            // Prepare and draw text in a second pass using glyphon at texture-local coordinates
            let text_items: Vec<crate::renderer::DrawText> =
                translated_items.iter().filter_map(map_text_item).collect();
            if !text_items.is_empty() {
                self.glyphon_prepare_for(&text_items);
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
                text_pass.set_viewport(0.0, 0.0, tex_width as f32, tex_height as f32, 0.0, 1.0);
                let old_size2 = self.size;
                self.size = PhysicalSize::new(tex_width, tex_height);
                self.draw_text_batch(&mut text_pass, &text_items, None);
                self.size = old_size2;
            }
        }

        // Do not finish/submit here; caller will submit the shared encoder
        // Decrease offscreen depth after finishing this offscreen render
        self.offscreen_depth = self.offscreen_depth.saturating_sub(1);

        // Caller will return the texture to the pool after compositing
        Ok((texture, view, tex_width, tex_height))
    }

    fn draw_texture_quad(
        &mut self,
        pass: &mut RenderPass<'_>,
        view: &TextureView,
        alpha: f32,
        bounds: Bounds, // x, y, w, h in px
    ) {
        // Build a quad covering the group's bounds with UVs 0..1 over the offscreen texture
        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct TexVertex {
            pos: [f32; 2],
            uv: [f32; 2],
        }
        let (x, y, w, h) = bounds;
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
        // Create a tiny uniform buffer for alpha (std140-like padded to 16 bytes)
        let alpha_buf = self.device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("opacity-alpha"),
            contents: bytemuck::cast_slice(&[alpha, 0.0f32, 0.0f32, 0.0f32]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });
        // Bind group for texture + sampler + alpha
        let bind_group = self.device.create_bind_group(&BindGroupDescriptor {
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
        });
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
        pass.set_scissor_rect(rx, ry, rw, rh);
        pass.set_vertex_buffer(0, vb.slice(..));
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..6, 0..1);
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
        let instance = Instance::new(&InstanceDescriptor {
            backends: Backends::DX12 | Backends::GL,
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
        let (device, queue) = adapter
            .request_device(&DeviceDescriptor::default())
            .await
            .unwrap();

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
            glyphon_cache: glyphon_cache_local,
            viewport: viewport_local,
            layers: Vec::new(),
            clear_color: [1.0, 1.0, 1.0, 1.0],
            offscreen_depth: 0,
            offscreen_tex: None,
            readback_buf: None,
            readback_padded_bpr: 0,
            readback_size: 0,
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
        self.record_draw_passes(&texture_view, &mut encoder)?;
        let cb = encoder.finish();
        submit_with_validation(&self.device, &self.queue, [cb])?;
        self.window.pre_present_notify();
        surface_texture.present();
        Ok(())
    }

    /// Render a frame and return the framebuffer RGBA bytes using the exact same drawing path.
    pub fn render_to_rgba(&mut self) -> Result<Vec<u8>, anyhow::Error> {
        // Always render to an offscreen texture so we can COPY_SRC safely. Reuse across calls.
        let mut encoder = self.device.create_command_encoder(&Default::default());

        // (Re)create offscreen texture/view if missing
        let need_offscreen = self.offscreen_tex.is_none();
        if need_offscreen {
            let tex = self.device.create_texture(&TextureDescriptor {
                label: Some("offscreen-target"),
                size: Extent3d {
                    width: self.size.width,
                    height: self.size.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: self.render_format,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
                view_formats: &[],
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
        self.record_draw_passes(&tmp_view, &mut encoder)?;

        // Submit the command buffer with error scope to catch WGPU errors
        let command_buffer = encoder.finish();
        submit_with_validation(&self.device, &self.queue, [command_buffer])?;

        // Create a new encoder for the texture copy operation
        let mut copy_encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("texture-copy-encoder"),
            });

        // Read back with 256-byte aligned rows
        let width = self.size.width;
        let height = self.size.height;
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

        // Submit the copy command buffer with error scope
        let copy_command_buffer = copy_encoder.finish();
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
}

/// Pixel bounds (x, y, width, height)
pub(crate) type Bounds = (f32, f32, f32, f32);
