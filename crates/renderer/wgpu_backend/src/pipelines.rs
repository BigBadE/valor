use std::borrow::Cow;
use wgpu::util::DeviceExt;
use wgpu::*;

/// Vertex data used by the simple pipeline.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

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
    // Input colors are in sRGB space. When rendering to an sRGB texture format,
    // the GPU automatically handles linear<->sRGB conversions:
    // - Shader outputs linear RGB
    // - GPU converts to sRGB when writing to texture
    // So we need to convert input sRGB to linear before outputting
    let c = in.color;
    let rgb = c.xyz;
    let lo = rgb / 12.92;
    let hi = pow((rgb + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    let t = step(vec3<f32>(0.04045), rgb);
    let linear_rgb = mix(lo, hi, t);
    // Premultiply RGB by alpha for correct blending
    return vec4<f32>(linear_rgb * c.w, c.w);
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
    // Texture already contains premultiplied RGB (rgb * alpha).
    // To apply additional opacity, multiply the entire premultiplied color by the opacity factor.
    // This correctly scales both the premultiplied RGB and alpha channels.
    return c * u_params.alpha;
}
"#;

pub fn build_pipeline_and_buffers(
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
                // Premultiplied alpha blending; shader outputs premultiplied color
                blend: Some(BlendState {
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

pub fn build_texture_pipeline(
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
                // Premultiplied alpha blending: shader outputs premultiplied RGBA
                blend: Some(BlendState {
                    color: BlendComponent {
                        src_factor: BlendFactor::One, // RGB already multiplied by alpha in shader
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
