use glyphon::{
    Attrs as GlyphonAttrs, Buffer as GlyphonBuffer, Cache as GlyphonCache, Color as GlyphonColor,
    Metrics as GlyphonMetrics, Resolution, Shaping as GlyphonShaping, SwashCache, TextArea,
    TextAtlas, TextRenderer, Viewport,
};
use renderer::display_list::{DisplayItem, DisplayList, batch_display_list};
use renderer::renderer::DrawText;
use std::borrow::Cow;
use std::num::NonZeroU32;
use wgpu::util::DeviceExt;
use wgpu::*;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 4],
}

const SHADER_WGSL: &str = r#"
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
"#;

fn build_pipeline(device: &Device, render_format: TextureFormat) -> RenderPipeline {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("offscreen-shader"),
        source: ShaderSource::Wgsl(Cow::Borrowed(SHADER_WGSL)),
    });
    let vertex_buffers = [VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as BufferAddress,
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
            compilation_options: Default::default(),
        },
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
        primitive: PrimitiveState::default(),
        depth_stencil: None,
        multisample: MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

fn push_rect_vertices_ndc(
    out: &mut Vec<Vertex>,
    fw: u32,
    fh: u32,
    rect_xywh: [f32; 4],
    color: [f32; 4],
) {
    let fw = fw.max(1) as f32;
    let fh = fh.max(1) as f32;
    let [x, y, w, h] = rect_xywh;
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let x0 = (x / fw) * 2.0 - 1.0;
    let x1 = ((x + w) / fw) * 2.0 - 1.0;
    let y0 = 1.0 - (y / fh) * 2.0;
    let y1 = 1.0 - ((y + h) / fh) * 2.0;
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

pub fn render_display_list_to_rgba(
    dl: &DisplayList,
    width: u32,
    height: u32,
) -> anyhow::Result<Vec<u8>> {
    // Initialize wgpu device and queue
    let instance = Instance::new(&InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions::default()))
        .expect("wgpu adapter not found");
    let (device, queue) = pollster::block_on(adapter.request_device(&DeviceDescriptor::default()))?;

    // Offscreen color target
    let render_format = TextureFormat::Rgba8UnormSrgb;
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

    // Pipeline for rectangles
    let pipeline = build_pipeline(&device, render_format);

    // Glyphon text state
    let mut font_system = glyphon::FontSystem::new();
    font_system.db_mut().load_system_fonts();
    let glyphon_cache = GlyphonCache::new(&device);
    let mut text_atlas = TextAtlas::new(&device, &queue, &glyphon_cache, render_format);
    let mut text_renderer =
        TextRenderer::new(&mut text_atlas, &device, MultisampleState::default(), None);
    let mut viewport = Viewport::new(&device, &glyphon_cache);
    viewport.update(&queue, Resolution { width, height });
    let mut swash_cache = SwashCache::new();

    // Prepare text from DL
    let texts: Vec<DrawText> = dl
        .items
        .iter()
        .filter_map(|item| {
            if let DisplayItem::Text {
                x,
                y,
                text,
                color,
                font_size,
                bounds,
            } = item
            {
                Some(DrawText {
                    x: *x,
                    y: *y,
                    text: text.clone(),
                    color: *color,
                    font_size: *font_size,
                    bounds: *bounds,
                })
            } else {
                None
            }
        })
        .collect();
    // Build glyphon buffers
    let mut buffers: Vec<GlyphonBuffer> = Vec::with_capacity(texts.len());
    for item in &texts {
        let mut buffer = GlyphonBuffer::new(
            &mut font_system,
            GlyphonMetrics::new(item.font_size, item.font_size),
        );
        let attrs = GlyphonAttrs::new();
        buffer.set_text(
            &mut font_system,
            &item.text,
            &attrs,
            GlyphonShaping::Advanced,
        );
        buffers.push(buffer);
    }
    // Build text areas
    let mut areas: Vec<TextArea> = Vec::with_capacity(texts.len());
    for (index, item) in texts.iter().enumerate() {
        let color = GlyphonColor(0xFF00_0000); // opaque black
        let bounds = match item.bounds {
            Some((l, t, r, b)) => glyphon::TextBounds {
                left: l,
                top: t,
                right: r,
                bottom: b,
            },
            None => glyphon::TextBounds {
                left: 0,
                top: 0,
                right: width as i32,
                bottom: height as i32,
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
    // Prepare text
    let _ = text_renderer.prepare(
        &device,
        &queue,
        &mut font_system,
        &mut text_atlas,
        &viewport,
        areas,
        &mut swash_cache,
    );

    // Encode rendering passes
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        // Rectangles pass
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("offscreen-rects"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &texture_view,
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
        pass.set_pipeline(&pipeline);
        let batches = batch_display_list(dl, width, height);
        for b in batches.into_iter() {
            let mut vertices: Vec<Vertex> = Vec::with_capacity(b.quads.len() * 6);
            for q in b.quads.iter() {
                let rgba = [q.color[0], q.color[1], q.color[2], q.color[3]];
                push_rect_vertices_ndc(
                    &mut vertices,
                    width,
                    height,
                    [q.x, q.y, q.width, q.height],
                    rgba,
                );
            }
            if vertices.is_empty() {
                continue;
            }
            let vertex_bytes = bytemuck::cast_slice(vertices.as_slice());
            let vertex_buffer = device.create_buffer_init(&util::BufferInitDescriptor {
                label: Some("offscreen-rect-vertices"),
                contents: vertex_bytes,
                usage: BufferUsages::VERTEX,
            });
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.draw(0..(vertices.len() as u32), 0..1);
        }
    }
    {
        // Text pass
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("offscreen-text"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &texture_view,
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
        let _ = text_renderer.render(&text_atlas, &viewport, &mut pass);
    }

    // Read back texture to CPU buffer using 256-byte aligned rows
    let bytes_per_pixel: u32 = 4;
    let row_bytes: u32 = width * bytes_per_pixel;
    let align: u32 = 256;
    let padded_bpr: u32 = row_bytes.div_ceil(align) * align; // ceil to 256
    let buffer_size = (padded_bpr as u64) * (height as u64);
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("offscreen-readback"),
        size: buffer_size,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        TexelCopyBufferInfo {
            buffer: &readback,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(NonZeroU32::new(padded_bpr).unwrap().into()),
                rows_per_image: Some(NonZeroU32::new(height).unwrap().into()),
            },
        },
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);

    // Map and read
    let slice = readback.slice(..);
    // Map asynchronously with a callback, then block the device until mapping completes.
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(MapMode::Read, move |res| {
        let _ = sender.send(res);
    });
    // Drive the device until the mapping completes
    loop {
        let _ = device.poll(wgpu::PollType::Wait);
        if let Ok(res) = receiver.try_recv() {
            res?;
            break;
        }
    }
    // Copy each padded row into a tightly packed RGBA buffer
    let mapped = slice.get_mapped_range();
    let mut data = vec![0u8; (row_bytes as usize) * (height as usize)];
    for row in 0..height as usize {
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
