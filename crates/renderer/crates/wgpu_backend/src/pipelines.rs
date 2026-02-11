//! Rendering pipelines for different primitive types.

use bytemuck::{Pod, Zeroable};
use rewrite_core::Color;

use crate::shaders;

/// Collection of rendering pipelines.
pub struct Pipelines {
    rect_pipeline: wgpu::RenderPipeline,
    rect_bind_group_layout: wgpu::BindGroupLayout,
}

impl Pipelines {
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let rect_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Rect Bind Group Layout"),
                entries: &[],
            });

        let rect_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Rect Pipeline Layout"),
            bind_group_layouts: &[&rect_bind_group_layout],
            push_constant_ranges: &[],
        });

        let rect_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Rect Shader"),
            source: wgpu::ShaderSource::Wgsl(shaders::RECT_SHADER.into()),
        });

        let rect_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Rect Pipeline"),
            layout: Some(&rect_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &rect_shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<RectVertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        // position
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                        // color
                        wgpu::VertexAttribute {
                            offset: 8,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                    ],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &rect_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        Self {
            rect_pipeline,
            rect_bind_group_layout,
        }
    }

    pub fn draw_rect(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
    ) {
        let vertices = create_rect_vertices(x, y, width, height, color);

        // For now, we'll skip the actual rendering since we need a device reference
        // to create buffers. In a real implementation, we'd have a vertex buffer pool.
        // This is a simplified version showing the structure.
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct RectVertex {
    position: [f32; 2],
    color: [f32; 4],
}

fn create_rect_vertices(x: f32, y: f32, width: f32, height: f32, color: Color) -> [RectVertex; 6] {
    let color_f32 = [
        color.r as f32 / 255.0,
        color.g as f32 / 255.0,
        color.b as f32 / 255.0,
        color.a as f32 / 255.0,
    ];

    [
        // First triangle
        RectVertex {
            position: [x, y],
            color: color_f32,
        },
        RectVertex {
            position: [x + width, y],
            color: color_f32,
        },
        RectVertex {
            position: [x, y + height],
            color: color_f32,
        },
        // Second triangle
        RectVertex {
            position: [x + width, y],
            color: color_f32,
        },
        RectVertex {
            position: [x + width, y + height],
            color: color_f32,
        },
        RectVertex {
            position: [x, y + height],
            color: color_f32,
        },
    ]
}
