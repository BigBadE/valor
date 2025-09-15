use crate::display_list::{DisplayItem, DisplayList, batch_display_list};
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
fn map_text_item(item: &DisplayItem) -> Option<DrawText> {
    if let DisplayItem::Text {
        x,
        y,
        text,
        color,
        font_size,
    } = item
    {
        return Some(DrawText {
            x: *x,
            y: *y,
            text: text.clone(),
            color: *color,
            font_size: *font_size,
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

/// RenderState owns the GPU device/surface and a minimal pipeline to draw rectangles from layout.
pub struct RenderState {
    window: Arc<Window>,
    device: Device,
    queue: Queue,
    size: PhysicalSize<u32>,
    surface: Surface<'static>,
    surface_format: TextureFormat,
    render_format: TextureFormat,
    pipeline: RenderPipeline,
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
}

impl RenderState {
    /// Create the GPU device/surface and initialize a simple render pipeline.
    pub async fn new(window: Arc<Window>) -> RenderState {
        let instance = Instance::new(&InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&RequestAdapterOptions::default())
            .await
            .unwrap();
        let (device, queue) = adapter
            .request_device(&DeviceDescriptor::default())
            .await
            .unwrap();

        let size = window.inner_size();

        let surface = instance.create_surface(window.clone()).unwrap();
        let capabilities = surface.get_capabilities(&adapter);
        let surface_format = capabilities.formats[0];
        let render_format = surface_format.add_srgb_suffix();

        // Configure the surface before creating the pipeline
        let surface_config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            view_formats: vec![render_format],
            alpha_mode: CompositeAlphaMode::Auto,
            width: size.width,
            height: size.height,
            desired_maximum_frame_latency: 2,
            present_mode: PresentMode::AutoVsync,
        };
        surface.configure(&device, &surface_config);

        // Build pipeline and buffers now that formats are known
        let (pipeline, vertex_buffer, vertex_count) =
            build_pipeline_and_buffers(&device, render_format);

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

        RenderState {
            window,
            device,
            queue,
            size,
            surface,
            surface_format,
            render_format,
            pipeline,
            vertex_buffer,
            vertex_count,
            display_list: Vec::new(),
            text_list: Vec::new(),
            retained_display_list: None,
            // Glyphon text state
            font_system: FontSystem::new(),
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
        self.surface.configure(&self.device, &surface_config);
    }

    /// Handle window resize and reconfigure the surface.
    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.size = new_size;
        self.configure_surface();
        // Invalidate cached batches since framebuffer dimensions have changed
        self.cached_batches = None;
        self.last_retained_list = None;
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
        // Build buffers first
        let mut buffers: Vec<GlyphonBuffer> = Vec::with_capacity(self.text_list.len());
        for item in &self.text_list {
            let mut buffer = GlyphonBuffer::new(
                &mut self.font_system,
                Metrics::new(item.font_size, item.font_size),
            );
            let attrs = Attrs::new();
            buffer.set_text(&mut self.font_system, &item.text, &attrs, Shaping::Advanced);
            buffers.push(buffer);
        }
        // Build areas referencing buffers
        let mut areas: Vec<TextArea> = Vec::with_capacity(self.text_list.len());
        for (index, item) in self.text_list.iter().enumerate() {
            let red = (item.color[0].clamp(0.0, 1.0) * 255.0).round() as u8;
            let green = (item.color[1].clamp(0.0, 1.0) * 255.0).round() as u8;
            let blue = (item.color[2].clamp(0.0, 1.0) * 255.0).round() as u8;
            let color = GlyphonColor(
                ((255u32) << 24) | ((red as u32) << 16) | ((green as u32) << 8) | (blue as u32),
            );
            let bounds = TextBounds {
                left: 0,
                top: 0,
                right: framebuffer_width as i32,
                bottom: framebuffer_height as i32,
            };
            let buffer_ref = &buffers[index];
            areas.push(TextArea {
                buffer: buffer_ref,
                left: item.x,
                top: item.y,
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
        let _ = self.text_renderer.prepare(
            &self.device,
            &self.queue,
            &mut self.font_system,
            &mut self.text_atlas,
            &self.viewport,
            areas,
            &mut self.swash_cache,
        );
        let elapsed_ms = start.elapsed().as_millis() as u64;
        if cfg!(debug_assertions) {
            eprintln!(
                "glyphon_prepare: text_items={} time_ms={}",
                self.text_list.len(),
                elapsed_ms
            );
        }
    }

    /// Prepare glyphon for an arbitrary list of text items (used for per-layer text rendering).
    fn glyphon_prepare_for(&mut self, items: &[DrawText]) {
        let framebuffer_width = self.size.width;
        let framebuffer_height = self.size.height;
        let mut buffers: Vec<GlyphonBuffer> = Vec::with_capacity(items.len());
        for item in items.iter() {
            let mut buffer = GlyphonBuffer::new(
                &mut self.font_system,
                Metrics::new(item.font_size, item.font_size),
            );
            let attrs = Attrs::new();
            buffer.set_text(&mut self.font_system, &item.text, &attrs, Shaping::Advanced);
            buffers.push(buffer);
        }
        let mut areas: Vec<TextArea> = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            let red = (item.color[0].clamp(0.0, 1.0) * 255.0).round() as u8;
            let green = (item.color[1].clamp(0.0, 1.0) * 255.0).round() as u8;
            let blue = (item.color[2].clamp(0.0, 1.0) * 255.0).round() as u8;
            let color = GlyphonColor(
                ((255u32) << 24) | ((red as u32) << 16) | ((green as u32) << 8) | (blue as u32),
            );
            let bounds = TextBounds {
                left: 0,
                top: 0,
                right: framebuffer_width as i32,
                bottom: framebuffer_height as i32,
            };
            let buffer_ref = &buffers[index];
            areas.push(TextArea {
                buffer: buffer_ref,
                left: item.x,
                top: item.y,
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
        let _ = self.text_renderer.prepare(
            &self.device,
            &self.queue,
            &mut self.font_system,
            &mut self.text_atlas,
            &self.viewport,
            areas,
            &mut self.swash_cache,
        );
    }

    /// Render a frame by clearing and drawing quads from the current display list.
    pub fn render(&mut self) -> Result<(), anyhow::Error> {
        let _span = info_span!("renderer.render").entered();
        let use_layers = !self.layers.is_empty();
        let use_retained = self.retained_display_list.is_some() && !use_layers;
        let surface_texture = self.surface.get_current_texture()?;
        let texture_view = surface_texture.texture.create_view(&TextureViewDescriptor {
            format: Some(self.render_format),
            ..Default::default()
        });

        // Build vertex data depending on retained vs immediate path
        let fw = self.size.width.max(1) as f32;
        let fh = self.size.height.max(1) as f32;

        // Helper to convert a rect into two triangles worth of vertices (NDC space)
        let push_rect_vertices =
            |out: &mut Vec<Vertex>, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]| {
                if w <= 0.0 || h <= 0.0 {
                    return;
                }
                let x0 = (x / fw) * 2.0 - 1.0;
                let x1 = ((x + w) / fw) * 2.0 - 1.0;
                let y0 = 1.0 - (y / fh) * 2.0;
                let y1 = 1.0 - ((y + h) / fh) * 2.0;
                let c = color;
                out.push(Vertex {
                    position: [x0, y0],
                    color: c,
                });
                out.push(Vertex {
                    position: [x1, y0],
                    color: c,
                });
                out.push(Vertex {
                    position: [x0, y1],
                    color: c,
                });
                out.push(Vertex {
                    position: [x1, y0],
                    color: c,
                });
                out.push(Vertex {
                    position: [x1, y1],
                    color: c,
                });
                out.push(Vertex {
                    position: [x0, y1],
                    color: c,
                });
            };

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

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("main-pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &texture_view,
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
                        Layer::Background => { /* TODO: optional background */ }
                        Layer::Content(dl) | Layer::Chrome(dl) => {
                            // Draw rectangles for this layer using DL batching (no cache for simplicity)
                            let batches = batch_display_list(dl, self.size.width, self.size.height);
                            for b in batches.into_iter() {
                                let mut vertices: Vec<Vertex> =
                                    Vec::with_capacity(b.quads.len() * 6);
                                for q in b.quads.iter() {
                                    push_rect_vertices(
                                        &mut vertices,
                                        q.x,
                                        q.y,
                                        q.width,
                                        q.height,
                                        q.color,
                                    );
                                }
                                let vertex_bytes = bytemuck::cast_slice(vertices.as_slice());
                                let vertex_buffer =
                                    self.device.create_buffer_init(&util::BufferInitDescriptor {
                                        label: Some("layer-rect-batch"),
                                        contents: vertex_bytes,
                                        usage: BufferUsages::VERTEX,
                                    });
                                pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                                match b.scissor {
                                    Some((x, y, w, h)) => pass.set_scissor_rect(x, y, w, h),
                                    None => pass.set_scissor_rect(
                                        0,
                                        0,
                                        self.size.width.max(1),
                                        self.size.height.max(1),
                                    ),
                                }
                                if !vertices.is_empty() {
                                    pass.draw(0..(vertices.len() as u32), 0..1);
                                }
                            }
                            // Draw text for this layer on top of its rects
                            let layer_text: Vec<DrawText> = dl
                                .items
                                .iter()
                                .filter_map(|item| {
                                    if let DisplayItem::Text {
                                        x,
                                        y,
                                        text,
                                        color,
                                        font_size,
                                    } = item
                                    {
                                        Some(DrawText {
                                            x: *x,
                                            y: *y,
                                            text: text.clone(),
                                            color: *color,
                                            font_size: *font_size,
                                        })
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            if !layer_text.is_empty() {
                                self.glyphon_prepare_for(&layer_text);
                                let _ = self.text_renderer.render(
                                    &self.text_atlas,
                                    &self.viewport,
                                    &mut pass,
                                );
                            }
                        }
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
                if need_rebuild && let Some(dl) = &self.retained_display_list {
                    let batches = batch_display_list(dl, self.size.width, self.size.height);
                    let mut cache: Vec<BatchCacheEntry> = Vec::with_capacity(batches.len());
                    for b in batches.into_iter() {
                        let mut vertices: Vec<Vertex> = Vec::with_capacity(b.quads.len() * 6);
                        for q in b.quads.iter() {
                            push_rect_vertices(&mut vertices, q.x, q.y, q.width, q.height, q.color);
                        }
                        if vertices.is_empty() {
                            // Store an empty draw to preserve batch/scissor alignment
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
                            continue;
                        }
                        let vertex_bytes = bytemuck::cast_slice(vertices.as_slice());
                        let vertex_buffer =
                            self.device.create_buffer_init(&util::BufferInitDescriptor {
                                label: Some("rect-batch"),
                                contents: vertex_bytes,
                                usage: BufferUsages::VERTEX,
                            });
                        cache.push((b.scissor, vertex_buffer, vertices.len() as u32));
                    }
                    self.cached_batches = Some(cache);
                    self.last_retained_list = self.retained_display_list.clone();
                    // Stats: count cache builds
                    self.cache_builds = self.cache_builds.wrapping_add(1);
                }
                if let Some(ref cache) = self.cached_batches {
                    if !need_rebuild {
                        self.cache_reuses = self.cache_reuses.wrapping_add(1);
                    }
                    for (scissor_opt, buffer, count) in cache.iter() {
                        pass.set_vertex_buffer(0, buffer.slice(..));
                        match scissor_opt {
                            Some((x, y, w, h)) => pass.set_scissor_rect(*x, *y, *w, *h),
                            None => pass.set_scissor_rect(
                                0,
                                0,
                                self.size.width.max(1),
                                self.size.height.max(1),
                            ),
                        }
                        if *count > 0 {
                            pass.draw(0..*count, 0..1);
                        }
                    }
                }
                // Render prepared glyphon text for retained path
                let _ = self
                    .text_renderer
                    .render(&self.text_atlas, &self.viewport, &mut pass);
            } else {
                // Immediate path: batch all rects into one draw call as before
                let mut vertices: Vec<Vertex> = Vec::with_capacity(self.display_list.len() * 6);
                for rect in &self.display_list {
                    let rgba = [rect.color[0], rect.color[1], rect.color[2], 1.0];
                    push_rect_vertices(
                        &mut vertices,
                        rect.x,
                        rect.y,
                        rect.width,
                        rect.height,
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
                // Render prepared glyphon text on top
                let _ = self
                    .text_renderer
                    .render(&self.text_atlas, &self.viewport, &mut pass);
            }
        }

        self.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        surface_texture.present();
        Ok(())
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
                blend: Some(BlendState::ALPHA_BLENDING),
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

/// Vertex data used by the simple pipeline.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
}

/// Minimal WGSL shader that passes through a colored triangle with alpha.
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
    return in.color;
}
"#;
