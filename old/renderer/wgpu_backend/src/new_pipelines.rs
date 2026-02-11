//! Pipeline builders for new display item types (Border, BoxShadow, Image, Gradients).
//!
//! This module provides pipeline creation functions for rendering the extended
//! DisplayItem variants added to the display list.

use core::mem::size_of;
use std::borrow::Cow;
use wgpu::*;

// Shader sources
const BORDER_SHADER: &str = include_str!("shaders/border.wgsl");
const BOX_SHADOW_SHADER: &str = include_str!("shaders/box_shadow.wgsl");
const IMAGE_SHADER: &str = include_str!("shaders/image.wgsl");
const GRADIENT_SHADER: &str = include_str!("shaders/gradient.wgsl");

/// Vertex format for border rendering.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BorderVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub border_width: f32,
    pub border_radius: f32,
    pub rect_size: [f32; 2],
}

/// Vertex format for box shadow rendering.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BoxShadowVertex {
    pub position: [f32; 2],
    pub shadow_center: [f32; 2],
    pub shadow_size: [f32; 2],
    pub blur_radius: f32,
    pub spread_radius: f32,
    pub color: [f32; 4],
}

/// Vertex format for image rendering.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ImageVertex {
    pub position: [f32; 2],
    pub texture_coords: [f32; 2],
}

/// Vertex format for gradient rendering.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GradientVertex {
    pub position: [f32; 2],
    pub texture_coords: [f32; 2],
    pub gradient_type: u32,
    pub angle: f32,
}

/// Gradient stop data for uniform buffer.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GradientStop {
    pub offset: f32,
    pub color: [f32; 4],
    padding: [f32; 3],
}

/// Gradient uniforms for shader.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GradientUniforms {
    pub stops: [GradientStop; 16],
    pub stop_count: u32,
    padding: [f32; 3],
}

/// Build border rendering pipeline.
pub fn build_border_pipeline(device: &Device, render_format: TextureFormat) -> RenderPipeline {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("border-shader"),
        source: ShaderSource::Wgsl(Cow::Borrowed(BORDER_SHADER)),
    });

    let vertex_buffer_layout = VertexBufferLayout {
        array_stride: size_of::<BorderVertex>() as BufferAddress,
        step_mode: VertexStepMode::Vertex,
        attributes: &[
            // position
            VertexAttribute {
                format: VertexFormat::Float32x2,
                offset: 0,
                shader_location: 0,
            },
            // color
            VertexAttribute {
                format: VertexFormat::Float32x4,
                offset: 8,
                shader_location: 1,
            },
            // border_width
            VertexAttribute {
                format: VertexFormat::Float32,
                offset: 24,
                shader_location: 2,
            },
            // border_radius
            VertexAttribute {
                format: VertexFormat::Float32,
                offset: 28,
                shader_location: 3,
            },
            // rect_size
            VertexAttribute {
                format: VertexFormat::Float32x2,
                offset: 32,
                shader_location: 4,
            },
        ],
    };

    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("border-pipeline-layout"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });

    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("border-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[vertex_buffer_layout],
            compilation_options: PipelineCompilationOptions::default(),
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
                blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: PipelineCompilationOptions::default(),
        }),
        multiview: None,
        cache: None,
    })
}

/// Build box shadow rendering pipeline.
pub fn build_box_shadow_pipeline(device: &Device, render_format: TextureFormat) -> RenderPipeline {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("box-shadow-shader"),
        source: ShaderSource::Wgsl(Cow::Borrowed(BOX_SHADOW_SHADER)),
    });

    let vertex_buffer_layout = VertexBufferLayout {
        array_stride: size_of::<BoxShadowVertex>() as BufferAddress,
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
            VertexAttribute {
                format: VertexFormat::Float32x2,
                offset: 16,
                shader_location: 2,
            },
            VertexAttribute {
                format: VertexFormat::Float32,
                offset: 24,
                shader_location: 3,
            },
            VertexAttribute {
                format: VertexFormat::Float32,
                offset: 28,
                shader_location: 4,
            },
            VertexAttribute {
                format: VertexFormat::Float32x4,
                offset: 32,
                shader_location: 5,
            },
        ],
    };

    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("box-shadow-pipeline-layout"),
        bind_group_layouts: &[],
        push_constant_ranges: &[],
    });

    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("box-shadow-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[vertex_buffer_layout],
            compilation_options: PipelineCompilationOptions::default(),
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
                blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: PipelineCompilationOptions::default(),
        }),
        multiview: None,
        cache: None,
    })
}

/// Build image rendering pipeline with bind group layout.
pub fn build_image_pipeline(
    device: &Device,
    render_format: TextureFormat,
) -> (RenderPipeline, BindGroupLayout) {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("image-shader"),
        source: ShaderSource::Wgsl(Cow::Borrowed(IMAGE_SHADER)),
    });

    let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("image-bind-layout"),
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
        ],
    });

    let vertex_buffer_layout = VertexBufferLayout {
        array_stride: size_of::<ImageVertex>() as BufferAddress,
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
    };

    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("image-pipeline-layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("image-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[vertex_buffer_layout],
            compilation_options: PipelineCompilationOptions::default(),
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
                blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: PipelineCompilationOptions::default(),
        }),
        multiview: None,
        cache: None,
    });

    (pipeline, bind_group_layout)
}

/// Create vertex buffer layout for gradient vertices.
fn gradient_vertex_layout() -> VertexBufferLayout<'static> {
    VertexBufferLayout {
        array_stride: size_of::<GradientVertex>() as BufferAddress,
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
            VertexAttribute {
                format: VertexFormat::Uint32,
                offset: 16,
                shader_location: 2,
            },
            VertexAttribute {
                format: VertexFormat::Float32,
                offset: 20,
                shader_location: 3,
            },
        ],
    }
}

/// Build gradient rendering pipeline with bind group layout.
pub fn build_gradient_pipeline(
    device: &Device,
    render_format: TextureFormat,
) -> (RenderPipeline, BindGroupLayout) {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("gradient-shader"),
        source: ShaderSource::Wgsl(Cow::Borrowed(GRADIENT_SHADER)),
    });

    let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("gradient-bind-layout"),
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });

    let vertex_buffer_layout = gradient_vertex_layout();

    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("gradient-pipeline-layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("gradient-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[vertex_buffer_layout],
            compilation_options: PipelineCompilationOptions::default(),
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
                blend: Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: PipelineCompilationOptions::default(),
        }),
        multiview: None,
        cache: None,
    });

    (pipeline, bind_group_layout)
}
