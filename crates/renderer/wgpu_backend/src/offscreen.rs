use anyhow::Result as AnyhowResult;
use bytemuck::cast_slice;
use core::mem::size_of;
use glyphon::{
    Attrs as GlyphonAttrs, Buffer as GlyphonBuffer, Cache as GlyphonCache, Color as GlyphonColor,
    FontSystem, Metrics as GlyphonMetrics, Resolution, Shaping as GlyphonShaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use pollster::block_on;
use renderer::display_list::{DisplayItem, DisplayList, batch_display_list};
use renderer::renderer::DrawText;

/// Map a display item to text if it's a text item, otherwise return None.
fn map_text_item(item: &DisplayItem) -> Option<DrawText> {
    if let DisplayItem::Text {
        x,
        y,
        text,
        color,
        font_size,
        font_weight,
        font_family,
        line_height,
        bounds,
    } = item
    {
        Some(DrawText {
            x: *x,
            y: *y,
            text: text.clone(),
            color: *color,
            font_size: *font_size,
            font_weight: *font_weight,
            font_family: font_family.clone(),
            line_height: *line_height,
            bounds: *bounds,
        })
    } else {
        None
    }
}
use std::borrow::Cow;
use std::sync::mpsc::channel;
use wgpu::util::DeviceExt as _;
use wgpu::*;

/// Vertex structure for offscreen rendering.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    /// Position in NDC coordinates.
    position: [f32; 2],
    /// RGBA color.
    color: [f32; 4],
}

/// WGSL shader source for offscreen rendering.
const SHADER_WGSL: &str = "
struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(@location(0) pos: vec2<f32>, @location(1) color: vec4<f32>) -> VertexOut {
    var out: VertexOut;
    out.pos = vec4<f32>(pos, 0.0, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> { return in.color; }
";

/// Build the rendering pipeline for offscreen rendering.
fn build_pipeline(device: &Device, render_format: TextureFormat) -> RenderPipeline {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("offscreen-shader"),
        source: ShaderSource::Wgsl(Cow::Borrowed(SHADER_WGSL)),
    });
    let vertex_buffers = [VertexBufferLayout {
        array_stride: size_of::<Vertex>() as BufferAddress,
        step_mode: VertexStepMode::Vertex,
        attributes: &[
            VertexAttribute {
                format: VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            VertexAttribute {
                format: VertexFormat::Float32x4,
                offset: 8,
                shader_location: 1,
            },
        ],
    }];
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("offscreen-pipeline-layout"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("offscreen-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &vertex_buffers,
            compilation_options: PipelineCompilationOptions::default(),
        },
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(ColorTargetState {
                format: render_format,
                blend: Some(BlendState::ALPHA_BLENDING),
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: PipelineCompilationOptions::default(),
        }),
        primitive: PrimitiveState::default(),
        depth_stencil: None,
        multisample: MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

/// Push rectangle vertices in NDC coordinates to the vertex buffer.
fn push_rect_vertices_ndc(
    out: &mut Vec<Vertex>,
    framebuffer_width: u32,
    framebuffer_height: u32,
    rect_xywh: [f32; 4],
    color: [f32; 4],
) {
    let frame_width = framebuffer_width.max(1) as f32;
    let frame_height = framebuffer_height.max(1) as f32;
    let [rect_x, rect_y, rect_width, rect_height] = rect_xywh;
    if rect_width <= 0.0 || rect_height <= 0.0 {
        return;
    }
    let x0 = (rect_x / frame_width).mul_add(2.0, -1.0);
    let x1 = ((rect_x + rect_width) / frame_width).mul_add(2.0, -1.0);
    let y0 = (rect_y / frame_height).mul_add(-2.0, 1.0);
    let y1 = ((rect_y + rect_height) / frame_height).mul_add(-2.0, 1.0);
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

/// GPU context for offscreen rendering.
struct OffscreenGpuContext {
    /// WGPU device for creating GPU resources.
    device: Device,
    /// Command queue for submitting GPU work.
    queue: Queue,
}

/// Initialize GPU device and queue for offscreen rendering.
///
/// # Errors
/// Returns an error if GPU adapter or device initialization fails.
fn initialize_gpu() -> AnyhowResult<OffscreenGpuContext> {
    let instance = Instance::new(&InstanceDescriptor::default());
    let adapter = block_on(instance.request_adapter(&RequestAdapterOptions::default()))
        .map_err(|err| anyhow::anyhow!("wgpu adapter not found: {err}"))?;
    let (device, queue) = block_on(adapter.request_device(&DeviceDescriptor::default()))?;
    Ok(OffscreenGpuContext { device, queue })
}

/// Create an offscreen render texture.
fn create_render_texture(
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
struct GlyphonState {
    /// Font system for loading and managing fonts.
    font_system: FontSystem,
    /// Texture atlas for glyph caching.
    text_atlas: TextAtlas,
    /// Text renderer for drawing glyphs.
    text_renderer: TextRenderer,
    /// Viewport for coordinate transformation.
    viewport: Viewport,
    /// Swash cache for rasterizing glyphs.
    swash_cache: SwashCache,
}

/// Parameters for initializing Glyphon.
struct GlyphonInitParams<'init> {
    /// GPU device reference.
    device: &'init Device,
    /// Command queue reference.
    queue: &'init Queue,
    /// Glyph cache reference.
    glyphon_cache: &'init GlyphonCache,
    /// Render texture format.
    render_format: TextureFormat,
    /// Viewport width in pixels.
    width: u32,
    /// Viewport height in pixels.
    height: u32,
}

/// Initialize Glyphon text rendering state.
fn initialize_glyphon(params: &GlyphonInitParams<'_>) -> GlyphonState {
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

/// Parameters for preparing text items.
struct PrepareTextParams<'prepare> {
    /// Display list containing text items.
    display_list: &'prepare DisplayList,
    /// Glyphon state for text rendering.
    glyphon_state: &'prepare mut GlyphonState,
    /// GPU device reference.
    device: &'prepare Device,
    /// Command queue reference.
    queue: &'prepare Queue,
    /// Viewport width in pixels.
    width: u32,
    /// Viewport height in pixels.
    height: u32,
}

/// Prepare text items for rendering.
///
/// # Errors
/// Returns an error if text preparation fails.
fn prepare_text_items(params: &mut PrepareTextParams<'_>) -> AnyhowResult<(Vec<DrawText>, Vec<GlyphonBuffer>)> {
    let texts: Vec<DrawText> = params
        .display_list
        .items
        .iter()
        .filter_map(map_text_item)
        .collect();
    let mut buffers: Vec<GlyphonBuffer> = Vec::with_capacity(texts.len());
    for item in &texts {
        let mut buffer = GlyphonBuffer::new(
            &mut params.glyphon_state.font_system,
            GlyphonMetrics::new(item.font_size, item.font_size),
        );
        let attrs = GlyphonAttrs::new().cache_key_flags(glyphon::CacheKeyFlags::SUBPIXEL_RENDERING);
        buffer.set_text(
            &mut params.glyphon_state.font_system,
            &item.text,
            &attrs,
            GlyphonShaping::Advanced,
            None,
        );
        buffers.push(buffer);
    }
    let mut areas: Vec<TextArea> = Vec::with_capacity(texts.len());
    for (index, item) in texts.iter().enumerate() {
        let color = GlyphonColor(0xFF00_0000);
        let bounds = match item.bounds {
            Some((left, top, right, bottom)) => TextBounds {
                left,
                top,
                right,
                bottom,
            },
            None => TextBounds {
                left: 0,
                top: 0,
                right: i32::try_from(params.width).unwrap_or(i32::MAX),
                bottom: i32::try_from(params.height).unwrap_or(i32::MAX),
            },
        };
        areas.push(TextArea {
            buffer: &buffers[index],
            left: item.x,
            top: item.y,
            scale: 1.0,
            bounds,
            default_color: color,
            custom_glyphs: &[],
        });
    }
    params.glyphon_state.text_renderer.prepare(
        params.device,
        params.queue,
        &mut params.glyphon_state.font_system,
        &mut params.glyphon_state.text_atlas,
        &params.glyphon_state.viewport,
        areas,
        &mut params.glyphon_state.swash_cache,
    )?;
    Ok((texts, buffers))
}

/// Parameters for rendering rectangles.
struct RenderRectsParams<'render> {
    /// Command encoder for recording commands.
    encoder: &'render mut CommandEncoder,
    /// Texture view to render into.
    texture_view: &'render TextureView,
    /// Render pipeline for rectangles.
    pipeline: &'render RenderPipeline,
    /// Display list containing rectangles.
    display_list: &'render DisplayList,
    /// GPU device for creating buffers.
    device: &'render Device,
    /// Viewport width in pixels.
    width: u32,
    /// Viewport height in pixels.
    height: u32,
}

/// Render rectangles from the display list.
fn render_rectangles_pass(params: &mut RenderRectsParams<'_>) {
    let mut pass = params.encoder.begin_render_pass(&RenderPassDescriptor {
        label: Some("offscreen-rects"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: params.texture_view,
            depth_slice: None,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Clear(Color {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                }),
                store: StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    });
    pass.set_pipeline(params.pipeline);
    let batches = batch_display_list(params.display_list, params.width, params.height);
    for batch in batches {
        let mut vertices: Vec<Vertex> = Vec::with_capacity(batch.quads.len() * 6);
        for quad in &batch.quads {
            let rgba = [quad.color[0], quad.color[1], quad.color[2], quad.color[3]];
            push_rect_vertices_ndc(
                &mut vertices,
                params.width,
                params.height,
                [quad.x, quad.y, quad.width, quad.height],
                rgba,
            );
        }
        if vertices.is_empty() {
            continue;
        }
        let vertex_bytes = cast_slice(vertices.as_slice());
        let vertex_buffer = params
            .device
            .create_buffer_init(&util::BufferInitDescriptor {
                label: Some("offscreen-rect-vertices"),
                contents: vertex_bytes,
                usage: BufferUsages::VERTEX,
            });
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..(vertices.len() as u32), 0..1);
    }
}

/// Render text using Glyphon.
///
/// # Errors
/// Returns an error if text rendering fails.
fn render_text_pass(
    encoder: &mut CommandEncoder,
    texture_view: &TextureView,
    glyphon_state: &GlyphonState,
    width: u32,
    height: u32,
) -> AnyhowResult<()> {
    let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
        label: Some("offscreen-text"),
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
    pass.set_viewport(0.0, 0.0, width as f32, height as f32, 0.0, 1.0);
    pass.set_scissor_rect(0, 0, width.max(1), height.max(1));
    glyphon_state.text_renderer.render(
        &glyphon_state.text_atlas,
        &glyphon_state.viewport,
        &mut pass,
    )?;
    Ok(())
}

/// Parameters for texture readback.
struct ReadbackParams<'readback> {
    /// Command encoder (consumed).
    encoder: CommandEncoder,
    /// Texture to read back.
    texture: &'readback Texture,
    /// GPU device for creating buffers.
    device: &'readback Device,
    /// Command queue for submission.
    queue: &'readback Queue,
    /// Texture width in pixels.
    width: u32,
    /// Texture height in pixels.
    height: u32,
}

/// Read back texture from GPU to CPU buffer.
///
/// # Errors
/// Returns an error if buffer mapping or readback fails.
fn readback_texture(params: ReadbackParams<'_>) -> AnyhowResult<Vec<u8>> {
    let bytes_per_pixel: u32 = 4;
    let row_bytes: u32 = params.width * bytes_per_pixel;
    let align: u32 = 256;
    let padded_bpr: u32 = row_bytes.div_ceil(align) * align;
    let buffer_size = u64::from(padded_bpr) * u64::from(params.height);
    let readback = params.device.create_buffer(&BufferDescriptor {
        label: Some("offscreen-readback"),
        size: buffer_size,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = params.encoder;
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: params.texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        TexelCopyBufferInfo {
            buffer: &readback,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bpr),
                rows_per_image: Some(params.height),
            },
        },
        Extent3d {
            width: params.width,
            height: params.height,
            depth_or_array_layers: 1,
        },
    );
    params.queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    let (sender, receiver) = channel();
    slice.map_async(MapMode::Read, move |res| {
        drop(sender.send(res));
    });
    loop {
        drop(params.device.poll(PollType::Wait));
        if let Ok(res) = receiver.try_recv() {
            res?;
            break;
        }
    }
    let mapped = slice.get_mapped_range();
    let mut data = vec![0u8; (row_bytes as usize) * (params.height as usize)];
    for row in 0..params.height as usize {
        let src_offset = row * (padded_bpr as usize);
        let dst_offset = row * (row_bytes as usize);
        let src = &mapped[src_offset..src_offset + (row_bytes as usize)];
        let dst = &mut data[dst_offset..dst_offset + (row_bytes as usize)];
        dst.copy_from_slice(src);
    }
    drop(mapped);
    readback.unmap();
    Ok(data)
}

/// Render a display list to an RGBA buffer using offscreen rendering.
///
/// # Errors
/// Returns an error if GPU initialization, rendering, or buffer readback fails.
pub fn render_display_list_to_rgba(
    display_list: &DisplayList,
    width: u32,
    height: u32,
) -> AnyhowResult<Vec<u8>> {
    let OffscreenGpuContext { device, queue } = initialize_gpu()?;
    let render_format = TextureFormat::Rgba8UnormSrgb;
    let (texture, texture_view) = create_render_texture(&device, width, height, render_format);
    let pipeline = build_pipeline(&device, render_format);
    let glyphon_cache = GlyphonCache::new(&device);
    let mut glyphon_state = initialize_glyphon(&GlyphonInitParams {
        device: &device,
        queue: &queue,
        glyphon_cache: &glyphon_cache,
        render_format,
        width,
        height,
    });
    let (_texts, _buffers) = prepare_text_items(&mut PrepareTextParams {
        display_list,
        glyphon_state: &mut glyphon_state,
        device: &device,
        queue: &queue,
        width,
        height,
    })?;
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    render_rectangles_pass(&mut RenderRectsParams {
        encoder: &mut encoder,
        texture_view: &texture_view,
        pipeline: &pipeline,
        display_list,
        device: &device,
        width,
        height,
    });
    render_text_pass(&mut encoder, &texture_view, &glyphon_state, width, height)?;
    readback_texture(ReadbackParams {
        encoder,
        texture: &texture,
        device: &device,
        queue: &queue,
        width,
        height,
    })
}
