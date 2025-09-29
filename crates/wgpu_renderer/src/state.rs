use crate::display_list::{
    DisplayItem, DisplayList, Scissor, StackingContextBoundary, batch_display_list,
};
use crate::renderer::{DrawRect, DrawText};
use glyphon::{
    Attrs, Buffer as GlyphonBuffer, Cache, Color as GlyphonColor, FontSystem, Metrics, Resolution,
    Shaping, SwashCache, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use std::borrow::Cow;
use std::sync::Arc;
use tracing::info_span;
use wgpu::util::DeviceExt;
use wgpu::*;
use winit::dpi::PhysicalSize;
use winit::window::Window;

#[inline]
fn batch_texts_with_scissor(
    dl: &DisplayList,
    framebuffer_w: u32,
    framebuffer_h: u32,
) -> Vec<TextBatch> {
    use crate::display_list::DisplayItem;
    let mut out: Vec<TextBatch> = Vec::new();
    let mut stack: Vec<Scissor> = Vec::new();
    let mut current_scissor: Option<Scissor> = None;
    let mut current_texts: Vec<DrawText> = Vec::new();
    for item in &dl.items {
        match item {
            DisplayItem::BeginClip {
                x,
                y,
                width,
                height,
            } => {
                if !current_texts.is_empty() {
                    out.push((current_scissor, std::mem::take(&mut current_texts)));
                }
                let new_sc =
                    rect_to_scissor((framebuffer_w, framebuffer_h), *x, *y, *width, *height);
                let effective = match current_scissor {
                    Some(sc) => intersect_scissor(sc, new_sc),
                    None => new_sc,
                };
                stack.push(new_sc);
                current_scissor = Some(effective);
            }
            DisplayItem::EndClip => {
                if !current_texts.is_empty() {
                    out.push((current_scissor, std::mem::take(&mut current_texts)));
                }
                let _ = stack.pop();
                current_scissor = stack.iter().cloned().reduce(intersect_scissor);
            }
            DisplayItem::Text {
                x,
                y,
                text,
                color,
                font_size,
                bounds,
            } => {
                current_texts.push(DrawText {
                    x: *x,
                    y: *y,
                    text: text.clone(),
                    color: *color,
                    font_size: *font_size,
                    bounds: *bounds,
                });
            }
            _ => {}
        }
    }
    if !current_texts.is_empty() {
        out.push((current_scissor, current_texts));
    }
    out
}

type TextBatch = (Option<Scissor>, Vec<DrawText>);

#[inline]
fn rect_to_scissor(framebuffer: (u32, u32), x: f32, y: f32, w: f32, h: f32) -> Scissor {
    let framebuffer_w = framebuffer.0.max(1);
    let framebuffer_h = framebuffer.1.max(1);
    let mut sx = x.max(0.0).floor() as i32;
    let mut sy = y.max(0.0).floor() as i32;
    let mut sw = w.max(0.0).ceil() as i32;
    let mut sh = h.max(0.0).ceil() as i32;
    if sx < 0 {
        sw += sx;
        sx = 0;
    }
    if sy < 0 {
        sh += sy;
        sy = 0;
    }
    let max_w = framebuffer_w as i32 - sx;
    let max_h = framebuffer_h as i32 - sy;
    let sw = sw.clamp(0, max_w) as u32;
    let sh = sh.clamp(0, max_h) as u32;
    (sx as u32, sy as u32, sw, sh)
}

#[inline]
fn intersect_scissor(a: Scissor, b: Scissor) -> Scissor {
    let (ax, ay, aw, ah) = a;
    let (bx, by, bw, bh) = b;
    let x0 = ax.max(bx);
    let y0 = ay.max(by);
    let x1 = (ax + aw).min(bx + bw);
    let y1 = (ay + ah).min(by + bh);
    let w = x1.saturating_sub(x0);
    let h = y1.saturating_sub(y0);
    (x0, y0, w, h)
}

#[inline]
fn batch_layer_texts_with_scissor(
    layers: &[Layer],
    framebuffer_w: u32,
    framebuffer_h: u32,
) -> Vec<TextBatch> {
    use crate::display_list::DisplayItem;
    let mut out: Vec<TextBatch> = Vec::new();
    for layer in layers {
        let dl = match layer {
            Layer::Content(dl) | Layer::Chrome(dl) => dl,
            Layer::Background => continue,
        };
        let mut stack: Vec<Scissor> = Vec::new();
        let mut current_scissor: Option<Scissor> = None;
        let mut current_texts: Vec<DrawText> = Vec::new();
        for item in &dl.items {
            match item {
                DisplayItem::BeginClip {
                    x,
                    y,
                    width,
                    height,
                } => {
                    if !current_texts.is_empty() {
                        out.push((current_scissor, std::mem::take(&mut current_texts)));
                    }
                    let new_sc =
                        rect_to_scissor((framebuffer_w, framebuffer_h), *x, *y, *width, *height);
                    let effective = match current_scissor {
                        Some(sc) => intersect_scissor(sc, new_sc),
                        None => new_sc,
                    };
                    stack.push(new_sc);
                    current_scissor = Some(effective);
                }
                DisplayItem::EndClip => {
                    if !current_texts.is_empty() {
                        out.push((current_scissor, std::mem::take(&mut current_texts)));
                    }
                    let _ = stack.pop();
                    current_scissor = stack.iter().cloned().reduce(intersect_scissor);
                }
                DisplayItem::Text {
                    x,
                    y,
                    text,
                    color,
                    font_size,
                    bounds,
                } => {
                    // Map to DrawText, keeping bounds and color as-is
                    current_texts.push(DrawText {
                        x: *x,
                        y: *y,
                        text: text.clone(),
                        color: *color,
                        font_size: *font_size,
                        bounds: *bounds,
                    });
                }
                _ => {}
            }
        }
        if !current_texts.is_empty() {
            out.push((current_scissor, current_texts));
        }
    }
    out
}

#[inline]
fn map_text_item(item: &DisplayItem) -> Option<DrawText> {
    if let DisplayItem::Text {
        x,
        y,
        text,
        color,
        font_size,
        bounds,
    } = item
    {
        return Some(DrawText {
            x: *x,
            y: *y,
            text: text.clone(),
            color: *color,
            font_size: *font_size,
            bounds: *bounds,
        });
    }
    None
}

/// Layer types for the simple compositor: order determines z-position.
#[derive(Debug, Clone)]
pub enum Layer {
    Background,
    Content(DisplayList),
    Chrome(DisplayList),
}

// Reduce type complexity for cached batch entries
type BatchCacheEntry = (Option<(u32, u32, u32, u32)>, Buffer, u32);

/// Texture pool for efficient reuse of offscreen textures in opacity groups.
/// Spec: Performance optimization for stacking context rendering
#[derive(Debug)]
struct TexturePool {
    /// Available textures: (width, height, texture)
    available: Vec<(u32, u32, Texture)>,
}

impl TexturePool {
    /// Create a new texture pool
    fn new() -> Self {
        Self {
            available: Vec::new(),
        }
    }

    /// Get or create a texture with the specified dimensions and format
    /// Spec: Reuse textures to minimize GPU memory allocation overhead
    fn get_or_create(
        &mut self,
        device: &Device,
        width: u32,
        height: u32,
        format: TextureFormat,
    ) -> Texture {
        // Find suitable existing texture (allow up to 25% larger to improve reuse)
        let max_width = width + width / 4;
        let max_height = height + height / 4;

        if let Some(pos) = self.available.iter().position(|(w, h, _)| {
            *w >= width && *h >= height && *w <= max_width && *h <= max_height
        }) {
            let (_w, _h, texture) = self.available.remove(pos);
            return texture;
        }

        // Create new texture with tight bounds
        device.create_texture(&TextureDescriptor {
            label: Some("opacity-group-texture"),
            size: Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    }

    /// Return a texture to the pool for reuse
    fn return_texture(&mut self, texture: Texture, width: u32, height: u32) {
        self.available.push((width.max(1), height.max(1), texture));
    }

    /// Clear all textures from the pool (called on resize)
    fn clear(&mut self) {
        self.available.clear();
    }
}

/// RenderState owns the GPU device/surface and a minimal pipeline to draw rectangles from layout.
pub struct RenderState {
    window: Arc<Window>,
    device: Device,
    queue: Queue,
    size: PhysicalSize<u32>,
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
    text_list: Vec<crate::renderer::DrawText>,
    /// Retained display list for Phase 6. When set via set_retained_display_list,
    /// it becomes the source of truth and is flattened into the immediate lists.
    retained_display_list: Option<DisplayList>,
    // Glyphon text rendering state
    font_system: FontSystem,
    swash_cache: SwashCache,
    text_atlas: TextAtlas,
    text_renderer: TextRenderer,
    #[allow(dead_code)]
    glyphon_cache: Cache,
    viewport: Viewport,
    /// Cached GPU buffers per retained-DL batch; reused when the DL is unchanged between frames.
    cached_batches: Option<Vec<BatchCacheEntry>>,
    /// Last retained display list used to populate the cache, for equality-based no-op detection.
    last_retained_list: Option<DisplayList>,
    /// Number of times retained-DL batches were rebuilt this session.
    cache_builds: u64,
    /// Number of times we reused cached batches without rebuilding.
    cache_reuses: u64,
    /// Optional layers for multi-DL compositing; when non-empty, render() draws these instead of the single retained list.
    layers: Vec<Layer>,
    /// Clear color for the framebuffer (canvas background). RGBA in [0,1].
    clear_color: [f32; 4],
    /// Texture pool for efficient offscreen texture reuse
    texture_pool: TexturePool,
}

impl RenderState {
    #[inline]
    fn draw_text_batch(
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
            Some((x, y, w, h)) => pass.set_scissor_rect(x, y, w, h),
            None => pass.set_scissor_rect(0, 0, self.size.width.max(1), self.size.height.max(1)),
        }
        let _ = self
            .text_renderer
            .render(&self.text_atlas, &self.viewport, pass);
    }

    #[inline]
    fn draw_text_batches(&mut self, pass: &mut RenderPass<'_>, batches: Vec<TextBatch>) {
        for (scissor_opt, items) in batches.into_iter().filter(|(_, it)| !it.is_empty()) {
            self.draw_text_batch(pass, &items, scissor_opt);
        }
    }
    /// Record all render passes (rectangles + text) into the provided texture view.
    /// This uses the exact same code paths as `render()`.
    fn record_draw_passes(&mut self, texture_view: &TextureView, encoder: &mut CommandEncoder) {
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
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("main-pass"),
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

            pass.set_pipeline(&self.pipeline);

            if use_layers {
                for layer in self.layers.clone().iter() {
                    match layer {
                        Layer::Background => continue,
                        Layer::Content(dl) | Layer::Chrome(dl) => self.draw_layer(&mut pass, dl),
                    }
                }
            } else if use_retained {
                // Use (or build) cached GPU buffers for retained DL batches.
                let need_rebuild = if let (Some(prev), Some(cur)) =
                    (&self.last_retained_list, &self.retained_display_list)
                {
                    prev != cur
                } else {
                    true
                };
                if need_rebuild {
                    self.rebuild_cached_batches();
                }
                self.draw_cached_batches(&mut pass, need_rebuild);
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
                pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                if self.vertex_count > 0 {
                    pass.draw(0..self.vertex_count, 0..1);
                }
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
                        // Keep rects, draw text on top
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if use_layers {
                let batches =
                    batch_layer_texts_with_scissor(&self.layers, self.size.width, self.size.height);
                self.draw_text_batches(&mut pass, batches);
            } else if use_retained {
                if let Some(dl) = &self.retained_display_list {
                    let batches = batch_texts_with_scissor(dl, self.size.width, self.size.height);
                    self.draw_text_batches(&mut pass, batches);
                }
            } else {
                // Immediate path: use whatever self.text_list was set to externally
                self.glyphon_prepare();
                pass.set_viewport(
                    0.0,
                    0.0,
                    self.size.width as f32,
                    self.size.height as f32,
                    0.0,
                    1.0,
                );
                pass.set_scissor_rect(0, 0, self.size.width.max(1), self.size.height.max(1));
                let _ = self
                    .text_renderer
                    .render(&self.text_atlas, &self.viewport, &mut pass);
            }
        }
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

    fn draw_layer(&mut self, pass: &mut RenderPass<'_>, dl: &DisplayList) {
        self.draw_items_with_groups(pass, &dl.items);
    }

    /// Draw display items with proper stacking context handling
    /// Spec: CSS 2.2 §9.9.1 - Stacking contexts and paint order
    fn draw_items_with_groups(&mut self, pass: &mut RenderPass<'_>, items: &[DisplayItem]) {
        let mut i = 0usize;

        while i < items.len() {
            match &items[i] {
                DisplayItem::BeginStackingContext { boundary } => {
                    // Find the matching end boundary
                    let end = self.find_stacking_context_end(items, i + 1);
                    let group_items = &items[i + 1..end];

                    match boundary {
                        StackingContextBoundary::Opacity { alpha } if *alpha < 1.0 => {
                            // Render opacity group to offscreen texture with tight bounds
                            let bounds = self
                                .compute_items_bounds(group_items)
                                .unwrap_or((0.0, 0.0, 1.0, 1.0)); // Minimal fallback

                            let (tex, view, tex_w, tex_h) =
                                self.render_items_to_offscreen_bounded(group_items, bounds);
                            self.draw_texture_quad(pass, &view, *alpha, bounds);
                            // Return texture to pool for reuse
                            self.texture_pool.return_texture(tex, tex_w, tex_h);
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
        items: &[DisplayItem],
        bounds: (f32, f32, f32, f32),
    ) -> (Texture, TextureView, u32, u32) {
        let (x, y, width, height) = bounds;
        let tex_width = (width.ceil() as u32).max(1);
        let tex_height = (height.ceil() as u32).max(1);

        // Choose linear format for intermediate compositing to avoid sRGB round trips
        let offscreen_format = match self.render_format {
            TextureFormat::Rgba8UnormSrgb => TextureFormat::Rgba8Unorm,
            TextureFormat::Bgra8UnormSrgb => TextureFormat::Bgra8Unorm,
            other => other,
        };

        // Get texture from pool or create new one with tight bounds
        let texture =
            self.texture_pool
                .get_or_create(&self.device, tex_width, tex_height, offscreen_format);

        let view = texture.create_view(&TextureViewDescriptor {
            format: Some(offscreen_format),
            ..Default::default()
        });

        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("opacity-group-encoder"),
            });

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

            // Draw translated items (rectangles) using the same group-aware path
            self.draw_items_with_groups(&mut pass, &translated_items);
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
                self.draw_text_batch(&mut text_pass, &text_items, None);
            }
        }

        self.queue.submit([encoder.finish()]);

        // Caller will return the texture to the pool after compositing
        (texture, view, tex_width, tex_height)
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
        // Create a tiny uniform buffer for alpha
        let alpha_buf = self.device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("opacity-alpha"),
            contents: bytemuck::cast_slice(&[alpha]),
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

    fn rebuild_cached_batches(&mut self) {
        if let Some(dl) = &self.retained_display_list {
            let batches = batch_display_list(dl, self.size.width, self.size.height);
            let mut cache: Vec<BatchCacheEntry> = Vec::with_capacity(batches.len());
            for b in batches.into_iter() {
                let mut vertices: Vec<Vertex> = Vec::with_capacity(b.quads.len() * 6);
                for q in b.quads.iter() {
                    self.push_rect_vertices_ndc(
                        &mut vertices,
                        [q.x, q.y, q.width, q.height],
                        q.color,
                    );
                }
                if vertices.is_empty() {
                    cache.push((
                        b.scissor,
                        self.device.create_buffer(&BufferDescriptor {
                            label: Some("empty-batch"),
                            size: 4,
                            usage: BufferUsages::VERTEX,
                            mapped_at_creation: false,
                        }),
                        0,
                    ));
                } else {
                    let vertex_bytes = bytemuck::cast_slice(vertices.as_slice());
                    let vertex_buffer =
                        self.device.create_buffer_init(&util::BufferInitDescriptor {
                            label: Some("rect-batch"),
                            contents: vertex_bytes,
                            usage: BufferUsages::VERTEX,
                        });
                    cache.push((b.scissor, vertex_buffer, vertices.len() as u32));
                }
            }
            self.cached_batches = Some(cache);
            self.last_retained_list = self.retained_display_list.clone();
            self.cache_builds = self.cache_builds.wrapping_add(1);
        }
    }

    fn draw_cached_batches(&mut self, pass: &mut RenderPass<'_>, need_rebuild: bool) {
        if let Some(ref cache) = self.cached_batches {
            if !need_rebuild {
                self.cache_reuses = self.cache_reuses.wrapping_add(1);
            }
            for (scissor_opt, buffer, count) in cache.iter() {
                pass.set_vertex_buffer(0, buffer.slice(..));
                match scissor_opt {
                    Some((x, y, w, h)) => pass.set_scissor_rect(*x, *y, *w, *h),
                    None => {
                        pass.set_scissor_rect(0, 0, self.size.width.max(1), self.size.height.max(1))
                    }
                }
                if *count > 0 {
                    pass.draw(0..*count, 0..1);
                }
            }
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
                power_preference: Default::default(),
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
            cached_batches: None,
            last_retained_list: None,
            cache_builds: 0,
            cache_reuses: 0,
            layers: Vec::new(),
            clear_color: [1.0, 1.0, 1.0, 1.0],
            texture_pool: TexturePool::new(), // Pool initialized; reuse capacity managed internally
        }
    }

    /// Window getter for integrations that require it.
    pub fn get_window(&self) -> &Window {
        &self.window
    }

    /// Return (cache_builds, cache_reuses) for retained display list batches.
    pub fn cache_stats(&self) -> (u64, u64) {
        (self.cache_builds, self.cache_reuses)
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
        // Invalidate cached batches and clear pooled textures to avoid size/format mismatches
        self.cached_batches = None;
        self.last_retained_list = None;
        self.texture_pool.clear();
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
        // Invalidate cached batches only if the list has changed
        if self.last_retained_list.as_ref() != Some(&list) {
            self.cached_batches = None;
        }
        // Using a single retained display list implies no layered compositing this frame.
        self.layers.clear();
        self.retained_display_list = Some(list);
        // Clear immediate lists; they will be ignored when retained list is present.
        self.display_list.clear();
        self.text_list.clear();
    }

    /// Prepare glyphon buffers for the current text list and upload glyphs into the atlas.
    fn glyphon_prepare(&mut self) {
        let _span = info_span!("renderer.glyphon_prepare").entered();
        let start = std::time::Instant::now();
        let framebuffer_width = self.size.width;
        let framebuffer_height = self.size.height;
        let scale: f32 = self.window.scale_factor() as f32;
        // Build buffers first
        let mut buffers: Vec<GlyphonBuffer> = Vec::with_capacity(self.text_list.len());
        for item in &self.text_list {
            let mut buffer = GlyphonBuffer::new(
                &mut self.font_system,
                Metrics::new(item.font_size * scale, item.font_size * scale),
            );
            let attrs = Attrs::new();
            buffer.set_text(&mut self.font_system, &item.text, &attrs, Shaping::Advanced);
            buffers.push(buffer);
        }
        // Build areas referencing buffers
        let mut areas: Vec<TextArea> = Vec::with_capacity(self.text_list.len());
        for (index, item) in self.text_list.iter().enumerate() {
            // Visible on white: use opaque black (ARGB alpha-highest)
            let color = GlyphonColor(0xFF00_0000);
            let bounds = match item.bounds {
                Some((l, t, r, b)) => TextBounds {
                    left: (l as f32 * scale).round() as i32,
                    top: (t as f32 * scale).round() as i32,
                    right: (r as f32 * scale).round() as i32,
                    bottom: (b as f32 * scale).round() as i32,
                },
                None => TextBounds {
                    left: 0,
                    top: 0,
                    right: framebuffer_width as i32,
                    bottom: framebuffer_height as i32,
                },
            };
            let buffer_ref = &buffers[index];
            areas.push(TextArea {
                buffer: buffer_ref,
                left: item.x * scale,
                top: item.y * scale,
                scale: 1.0,
                bounds,
                default_color: color,
                custom_glyphs: &[],
            });
        }
        // Prepare text (atlas upload + layout)
        self.viewport.update(
            &self.queue,
            Resolution {
                width: framebuffer_width,
                height: framebuffer_height,
            },
        );
        let areas_count = areas.len();
        let prep_res = self.text_renderer.prepare(
            &self.device,
            &self.queue,
            &mut self.font_system,
            &mut self.text_atlas,
            &self.viewport,
            areas,
            &mut self.swash_cache,
        );
        if cfg!(debug_assertions) {
            eprintln!(
                "glyphon_prepare: areas={areas_count} viewport={framebuffer_width}x{framebuffer_height} result={prep_res:?}"
            );
        }
        let elapsed_ms = start.elapsed().as_millis() as u64;
        if cfg!(debug_assertions) {
            eprintln!(
                "glyphon_prepare: text_items={} time_ms={elapsed_ms}",
                self.text_list.len()
            );
        }
    }

    fn glyphon_prepare_for(&mut self, items: &[DrawText]) {
        let framebuffer_width = self.size.width;
        let framebuffer_height = self.size.height;
        let scale: f32 = self.window.scale_factor() as f32;
        let mut buffers: Vec<GlyphonBuffer> = Vec::with_capacity(items.len());
        for item in items.iter() {
            let mut buffer = GlyphonBuffer::new(
                &mut self.font_system,
                Metrics::new(item.font_size * scale, item.font_size * scale),
            );
            let attrs = Attrs::new();
            buffer.set_text(&mut self.font_system, &item.text, &attrs, Shaping::Advanced);
            buffers.push(buffer);
        }
        let mut areas: Vec<TextArea> = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            let color = GlyphonColor(0xFF00_0000);
            let bounds = match item.bounds {
                Some((l, t, r, b)) => TextBounds {
                    left: (l as f32 * scale).round() as i32,
                    top: (t as f32 * scale).round() as i32,
                    right: (r as f32 * scale).round() as i32,
                    bottom: (b as f32 * scale).round() as i32,
                },
                None => TextBounds {
                    left: 0,
                    top: 0,
                    right: framebuffer_width as i32,
                    bottom: framebuffer_height as i32,
                },
            };
            let buffer_ref = &buffers[index];
            areas.push(TextArea {
                buffer: buffer_ref,
                left: item.x * scale,
                top: item.y * scale,
                scale: 1.0,
                bounds,
                default_color: color,
                custom_glyphs: &[],
            });
        }
        self.viewport.update(
            &self.queue,
            Resolution {
                width: framebuffer_width,
                height: framebuffer_height,
            },
        );
        let areas_len = areas.len();
        let prep_res = self.text_renderer.prepare(
            &self.device,
            &self.queue,
            &mut self.font_system,
            &mut self.text_atlas,
            &self.viewport,
            areas,
            &mut self.swash_cache,
        );
        if cfg!(debug_assertions) {
            eprintln!(
                "glyphon_prepare_for: items={} areas={} viewport={}x{} result={:?}",
                items.len(),
                areas_len,
                framebuffer_width,
                framebuffer_height,
                prep_res
            );
        }
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
        self.record_draw_passes(&texture_view, &mut encoder);
        self.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        surface_texture.present();
        Ok(())
    }

    /// Render a frame and return the framebuffer RGBA bytes using the exact same drawing path.
    pub fn render_to_rgba(&mut self) -> Result<Vec<u8>, anyhow::Error> {
        // Always render to an offscreen texture so we can COPY_SRC safely.
        let mut encoder = self.device.create_command_encoder(&Default::default());

        let offscreen = self.device.create_texture(&TextureDescriptor {
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
        let view = offscreen.create_view(&TextureViewDescriptor {
            format: Some(self.render_format),
            ..Default::default()
        });
        self.record_draw_passes(&view, &mut encoder);

        // Read back with 256-byte aligned rows
        let width = self.size.width;
        let height = self.size.height;
        let bpp = 4u32;
        let row_bytes = width * bpp;
        let padded_bpr = row_bytes.div_ceil(256) * 256;
        let buffer_size = (padded_bpr as u64) * (height as u64);
        let readback = self.device.create_buffer(&BufferDescriptor {
            label: Some("render-readback"),
            size: buffer_size,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: &offscreen,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            TexelCopyBufferInfo {
                buffer: &readback,
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

        self.queue.submit([encoder.finish()]);

        // Map and repack into tightly packed bytes (in the texture's native ordering)
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
        let mut out = vec![0u8; (row_bytes as usize) * (height as usize)];
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

fn build_pipeline_and_buffers(
    device: &Device,
    render_format: TextureFormat,
) -> (RenderPipeline, Buffer, u32) {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("basic-shader"),
        source: ShaderSource::Wgsl(Cow::Borrowed(SHADER_WGSL)),
    });

    let vertex_buffers = [VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as BufferAddress,
        step_mode: VertexStepMode::Vertex,
        attributes: &[
            // position (vec2<f32>)
            VertexAttribute {
                format: VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            // color (vec4<f32>)
            VertexAttribute {
                format: VertexFormat::Float32x4,
                offset: 8,
                shader_location: 1,
            },
        ],
    }];

    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("pipeline-layout"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("basic-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &vertex_buffers,
            compilation_options: Default::default(),
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(ColorTargetState {
                format: render_format,
                blend: Some(BlendState {
                    color: BlendComponent {
                        src_factor: BlendFactor::SrcAlpha,
                        dst_factor: BlendFactor::OneMinusSrcAlpha,
                        operation: BlendOperation::Add,
                    },
                    alpha: BlendComponent {
                        src_factor: BlendFactor::One,
                        dst_factor: BlendFactor::OneMinusSrcAlpha,
                        operation: BlendOperation::Add,
                    },
                }),
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        multiview: None,
        cache: None,
    });

    let vertices: [Vertex; 3] = [
        Vertex {
            position: [-0.5, -0.5],
            color: [1.0, 0.2, 0.2, 1.0],
        },
        Vertex {
            position: [0.5, -0.5],
            color: [0.2, 1.0, 0.2, 1.0],
        },
        Vertex {
            position: [0.0, 0.5],
            color: [0.2, 0.4, 1.0, 1.0],
        },
    ];
    let vertex_bytes = bytemuck::cast_slice(&vertices);
    let vertex_buffer = device.create_buffer_init(&util::BufferInitDescriptor {
        label: Some("triangle-vertices"),
        contents: vertex_bytes,
        usage: BufferUsages::VERTEX,
    });

    (pipeline, vertex_buffer, vertices.len() as u32)
}

fn build_texture_pipeline(
    device: &Device,
    render_format: TextureFormat,
) -> (RenderPipeline, BindGroupLayout, Sampler) {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("texture-shader"),
        source: ShaderSource::Wgsl(Cow::Borrowed(TEX_SHADER_WGSL)),
    });
    let bind_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("tex-bind-layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 2,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("tex-pipeline-layout"),
        bind_group_layouts: &[&bind_layout],
        push_constant_ranges: &[],
    });
    let vbuf = [VertexBufferLayout {
        array_stride: (std::mem::size_of::<f32>() as BufferAddress) * 4,
        step_mode: VertexStepMode::Vertex,
        attributes: &[
            VertexAttribute {
                format: VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            VertexAttribute {
                format: VertexFormat::Float32x2,
                offset: 8,
                shader_location: 1,
            },
        ],
    }];
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("texture-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &vbuf,
            compilation_options: Default::default(),
        },
        primitive: PrimitiveState::default(),
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(ColorTargetState {
                format: render_format,
                blend: Some(BlendState {
                    color: BlendComponent {
                        src_factor: BlendFactor::SrcAlpha,
                        dst_factor: BlendFactor::OneMinusSrcAlpha,
                        operation: BlendOperation::Add,
                    },
                    alpha: BlendComponent {
                        src_factor: BlendFactor::One,
                        dst_factor: BlendFactor::OneMinusSrcAlpha,
                        operation: BlendOperation::Add,
                    },
                }),
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: Default::default(),
        }),
        multiview: None,
        cache: None,
    });
    let sampler = device.create_sampler(&SamplerDescriptor {
        label: Some("linear-sampler"),
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        mipmap_filter: FilterMode::Nearest,
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        address_mode_w: AddressMode::ClampToEdge,
        ..Default::default()
    });
    (pipeline, bind_layout, sampler)
}

/// Vertex data used by the simple pipeline.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
}

/// Pixel bounds (x, y, width, height)
type Bounds = (f32, f32, f32, f32);

/// Minimal WGSL shader that converts sRGB vertex colors to linear for correct blending into an sRGB target.
const SHADER_WGSL: &str = r#"
struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(@location(0) position: vec2<f32>, @location(1) color: vec4<f32>) -> VertexOut {
    var out: VertexOut;
    out.pos = vec4<f32>(position, 0.0, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // Convert sRGB -> linear for correct blending when render target is *Srgb
    let c = in.color;
    let rgb = c.xyz;
    let lo = rgb / 12.92;
    let hi = pow((rgb + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    let t = step(vec3<f32>(0.04045), rgb);
    let linear_rgb = mix(lo, hi, t);
    return vec4<f32>(linear_rgb, c.w);
}
"#;

/// WGSL for textured quad with external alpha multiplier.
const TEX_SHADER_WGSL: &str = r#"
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var t_color: texture_2d<f32>;
@group(0) @binding(1) var t_sampler: sampler;
@group(0) @binding(2) var<uniform> u_alpha: f32;

@vertex
fn vs_main(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>) -> VsOut {
    var out: VsOut;
    out.pos = vec4<f32>(pos, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(t_color, t_sampler, in.uv);
    // Apply group opacity by scaling the alpha; blending uses SrcAlpha so color is scaled during blend.
    return vec4<f32>(c.rgb, c.a * u_alpha);
}
"#;
