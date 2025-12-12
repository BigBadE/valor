//! Offscreen rendering passes.

use crate::offscreen::initialization::GlyphonState;
use crate::pipelines::Vertex;
use anyhow::Result as AnyhowResult;
use bytemuck::cast_slice;
use core::mem::size_of;
use renderer::display_list::{DisplayList, batch_display_list};
use std::borrow::Cow;
use wgpu::util::DeviceExt as _;
use wgpu::*;

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
pub fn build_pipeline(device: &Device, render_format: TextureFormat) -> RenderPipeline {
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
pub fn push_rect_vertices_ndc(
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

/// Parameters for rendering rectangles.
pub struct RenderRectsParams<'render> {
    pub encoder: &'render mut CommandEncoder,
    pub texture_view: &'render TextureView,
    pub pipeline: &'render RenderPipeline,
    pub display_list: &'render DisplayList,
    pub device: &'render Device,
    pub width: u32,
    pub height: u32,
}

/// Render rectangles from the display list.
pub fn render_rectangles_pass(params: &mut RenderRectsParams<'_>) {
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
pub fn render_text_pass(
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
