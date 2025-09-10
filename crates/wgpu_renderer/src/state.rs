use std::sync::Arc;
use std::borrow::Cow;
use wgpu::*;
use wgpu::util::DeviceExt;
use winit::dpi::PhysicalSize;
use winit::window::Window;
use crate::renderer::{DrawRect, DrawText};
use glyphon::{FontSystem, SwashCache, TextAtlas, TextRenderer, Buffer as GlyphonBuffer, Metrics, Attrs, Shaping, TextArea, TextBounds, Color as GlyphonColor, Viewport, Cache, Resolution};


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
    // Glyphon text rendering state
    font_system: FontSystem,
    swash_cache: SwashCache,
    text_atlas: TextAtlas,
    text_renderer: TextRenderer,
    #[allow(dead_code)]
    glyphon_cache: Cache,
    viewport: Viewport,
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
        let (pipeline, vertex_buffer, vertex_count) = build_pipeline_and_buffers(&device, render_format);

        // Initialize glyphon text subsystem
        let glyphon_cache_local = Cache::new(&device);
        let mut text_atlas_local = TextAtlas::new(&device, &queue, &glyphon_cache_local, render_format);
        let text_renderer_local = TextRenderer::new(&mut text_atlas_local, &device, MultisampleState::default(), None);
        let mut viewport_local = Viewport::new(&device, &glyphon_cache_local);
        viewport_local.update(&queue, Resolution { width: size.width, height: size.height });

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
            // Glyphon text state
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            text_atlas: text_atlas_local,
            text_renderer: text_renderer_local,
            glyphon_cache: glyphon_cache_local,
            viewport: viewport_local,
        }
    }

    /// Window getter for integrations that require it.
    pub fn get_window(&self) -> &Window { &self.window }

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

    /// Prepare glyphon buffers for the current text list and upload glyphs into the atlas.
    fn glyphon_prepare(&mut self) {
        let framebuffer_width = self.size.width;
        let framebuffer_height = self.size.height;
        // Build buffers first
        let mut buffers: Vec<GlyphonBuffer> = Vec::with_capacity(self.text_list.len());
        for item in &self.text_list {
            let mut buffer = GlyphonBuffer::new(&mut self.font_system, Metrics::new(item.font_size, item.font_size));
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
            let color = GlyphonColor(((255u32) << 24) | ((red as u32) << 16) | ((green as u32) << 8) | (blue as u32));
            let bounds = TextBounds { left: 0, top: 0, right: framebuffer_width as i32, bottom: framebuffer_height as i32 };
            let buffer_ref = &buffers[index];
            areas.push(TextArea { buffer: buffer_ref, left: item.x, top: item.y, scale: 1.0, bounds, default_color: color, custom_glyphs: &[] });
        }
        // Prepare text (atlas upload + layout)
        self.viewport.update(&self.queue, Resolution { width: framebuffer_width, height: framebuffer_height });
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
        let surface_texture = self.surface.get_current_texture()?;
        let texture_view = surface_texture.texture.create_view(&TextureViewDescriptor {
            format: Some(self.render_format),
            ..Default::default()
        });

        // Build vertex data from rects (two triangles per rect)
        let fw = self.size.width.max(1) as f32;
        let fh = self.size.height.max(1) as f32;

        let mut vertices: Vec<Vertex> = Vec::with_capacity(self.display_list.len() * 6);
        for rect in &self.display_list {
            if rect.width <= 0.0 || rect.height <= 0.0 { continue; }
            let x0 = (rect.x / fw) * 2.0 - 1.0;
            let x1 = ((rect.x + rect.width) / fw) * 2.0 - 1.0;
            let y0 = 1.0 - (rect.y / fh) * 2.0;
            let y1 = 1.0 - ((rect.y + rect.height) / fh) * 2.0;
            let c = rect.color;
            // Triangle 1
            vertices.push(Vertex { position: [x0, y0], color: c });
            vertices.push(Vertex { position: [x1, y0], color: c });
            vertices.push(Vertex { position: [x0, y1], color: c });
            // Triangle 2
            vertices.push(Vertex { position: [x1, y0], color: c });
            vertices.push(Vertex { position: [x1, y1], color: c });
            vertices.push(Vertex { position: [x0, y1], color: c });
        }

        // Prepare text via glyphon (atlas + GPU buffers)
        self.glyphon_prepare();

        // Upload/replace vertex buffer each frame (rectangles only for now)
        let vertex_bytes = bytemuck::cast_slice(vertices.as_slice());
        let vertex_buffer = self.device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("rect-vertices"),
            contents: vertex_bytes,
            usage: BufferUsages::VERTEX,
        });
        self.vertex_buffer = vertex_buffer;
        self.vertex_count = vertices.len() as u32;

        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("main-pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &texture_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 }),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            if self.vertex_count > 0 {
                pass.draw(0..self.vertex_count, 0..1);
            }
            // Render prepared glyphon text on top of rectangles
            let _ = self.text_renderer.render(&self.text_atlas, &self.viewport, &mut pass);
        }

        self.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        surface_texture.present();
        Ok(())
    }

}

fn build_pipeline_and_buffers(device: &Device, render_format: TextureFormat) -> (RenderPipeline, Buffer, u32) {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("basic-shader"),
        source: ShaderSource::Wgsl(Cow::Borrowed(SHADER_WGSL)),
    });

    let vertex_buffers = [VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as BufferAddress,
        step_mode: VertexStepMode::Vertex,
        attributes: &[
            // position (vec2<f32>)
            VertexAttribute { format: VertexFormat::Float32x2, offset: 0, shader_location: 0 },
            // color (vec3<f32>)
            VertexAttribute { format: VertexFormat::Float32x3, offset: 8, shader_location: 1 },
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
        vertex: VertexState { module: &shader, entry_point: Some("vs_main"), buffers: &vertex_buffers, compilation_options: Default::default() },
        primitive: PrimitiveState { topology: PrimitiveTopology::TriangleList, ..Default::default() },
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(ColorTargetState { format: render_format, blend: Some(BlendState::ALPHA_BLENDING), write_mask: ColorWrites::ALL })],
            compilation_options: Default::default(),
        }),
        multiview: None,
        cache: None,
    });

    let vertices: [Vertex; 3] = [
        Vertex { position: [-0.5, -0.5], color: [1.0, 0.2, 0.2] },
        Vertex { position: [0.5, -0.5], color: [0.2, 1.0, 0.2] },
        Vertex { position: [0.0, 0.5], color: [0.2, 0.4, 1.0] },
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
    color: [f32; 3],
}

/// Minimal WGSL shader that passes through a colored triangle.
const SHADER_WGSL: &str = r#"
struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(@location(0) position: vec2<f32>, @location(1) color: vec3<f32>) -> VertexOut {
    var out: VertexOut;
    out.pos = vec4<f32>(position, 0.0, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
"#;
