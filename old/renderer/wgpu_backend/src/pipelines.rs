use bytemuck::cast_slice;
use core::mem::size_of;
use std::borrow::Cow;
use wgpu::util::DeviceExt as _;
use wgpu::*;

/// Vertex data used by the simple pipeline.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

/// Minimal WGSL shader for CSS-compliant sRGB-space rendering.
/// CSS specifies that all blending occurs in sRGB space, not linear space.
const SHADER_WGSL: &str = "
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
    // CSS spec requires blending in sRGB space (not linear).
    // Input colors are already in sRGB, output them directly.
    // Premultiply RGB by alpha for correct alpha blending.
    let c = in.color;
    return vec4<f32>(c.xyz * c.w, c.w);
}
";

/// WGSL for textured quad with external alpha multiplier.
const TEX_SHADER_WGSL: &str = "
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var t_color: texture_2d<f32>;
@group(0) @binding(1) var t_sampler: sampler;
struct Params { alpha: f32, _pad0: vec3<f32> };
@group(0) @binding(2) var<uniform> u_params: Params;

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
    // CSS spec requires opacity blending in sRGB space.
    // The offscreen texture contains sRGB-space premultiplied colors.
    // We multiply by opacity in sRGB space (treating values as linear arithmetic).
    // This matches CSS/Chrome behavior exactly.
    return c * u_params.alpha;
}
";

/// Create vertex buffer layout for basic rendering pipeline.
const fn create_vertex_buffer_layout() -> VertexBufferLayout<'static> {
    VertexBufferLayout {
        array_stride: size_of::<Vertex>() as BufferAddress,
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
    }
}

/// Create blend state for premultiplied alpha blending.
const fn create_basic_blend_state() -> BlendState {
    BlendState {
        color: BlendComponent {
            src_factor: BlendFactor::One,
            dst_factor: BlendFactor::OneMinusSrcAlpha,
            operation: BlendOperation::Add,
        },
        alpha: BlendComponent {
            src_factor: BlendFactor::One,
            dst_factor: BlendFactor::OneMinusSrcAlpha,
            operation: BlendOperation::Add,
        },
    }
}

/// Create blend state for opaque rendering (no blending, just replace).
const fn create_opaque_blend_state() -> BlendState {
    BlendState {
        color: BlendComponent {
            src_factor: BlendFactor::One,
            dst_factor: BlendFactor::Zero,
            operation: BlendOperation::Add,
        },
        alpha: BlendComponent {
            src_factor: BlendFactor::One,
            dst_factor: BlendFactor::Zero,
            operation: BlendOperation::Add,
        },
    }
}

pub fn build_pipeline_and_buffers(
    device: &Device,
    render_format: TextureFormat,
) -> (RenderPipeline, Buffer, u32) {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("basic-shader"),
        source: ShaderSource::Wgsl(Cow::Borrowed(SHADER_WGSL)),
    });

    let vertex_buffers = [create_vertex_buffer_layout()];

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
                blend: Some(create_basic_blend_state()),
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: PipelineCompilationOptions::default(),
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
    let vertex_bytes = cast_slice(&vertices);
    let vertex_buffer = device.create_buffer_init(&util::BufferInitDescriptor {
        label: Some("triangle-vertices"),
        contents: vertex_bytes,
        usage: BufferUsages::VERTEX,
    });

    (pipeline, vertex_buffer, vertices.len() as u32)
}

/// Create bind group layout for texture rendering pipeline.
fn create_texture_bind_group_layout(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
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
    })
}

/// Create vertex buffer layout for texture rendering pipeline.
const fn create_texture_vertex_buffer_layout() -> VertexBufferLayout<'static> {
    VertexBufferLayout {
        array_stride: (size_of::<f32>() as BufferAddress) * 4,
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
    }
}

pub fn build_texture_pipeline(
    device: &Device,
    render_format: TextureFormat,
) -> (RenderPipeline, BindGroupLayout, Sampler) {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("texture-shader"),
        source: ShaderSource::Wgsl(Cow::Borrowed(TEX_SHADER_WGSL)),
    });
    let bind_layout = create_texture_bind_group_layout(device);
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("tex-pipeline-layout"),
        bind_group_layouts: &[&bind_layout],
        push_constant_ranges: &[],
    });
    let vbuf = [create_texture_vertex_buffer_layout()];
    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("texture-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &vbuf,
            compilation_options: PipelineCompilationOptions::default(),
        },
        primitive: PrimitiveState::default(),
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(ColorTargetState {
                format: render_format,
                blend: Some(create_basic_blend_state()),
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: PipelineCompilationOptions::default(),
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

/// Build a pipeline for offscreen opacity group rendering without blending.
///
/// This pipeline outputs premultiplied alpha but doesn't blend with the destination, allowing
/// proper opacity compositing in a separate pass.
pub fn build_offscreen_pipeline(device: &Device, render_format: TextureFormat) -> RenderPipeline {
    let shader = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("offscreen-shader"),
        source: ShaderSource::Wgsl(Cow::Borrowed(SHADER_WGSL)),
    });

    let vertex_buffers = [create_vertex_buffer_layout()];

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
                // Use REPLACE blending (ONE, ZERO) for offscreen rendering
                // This prevents double-blending when opacity is applied later
                blend: Some(create_opaque_blend_state()),
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: PipelineCompilationOptions::default(),
        }),
        multiview: None,
        cache: None,
    })
}
