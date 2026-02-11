//! Texture and bind group management for opacity compositing.

use crate::state::gpu_context::GpuContext;
use crate::state::pipeline_cache::PipelineCache;
use crate::state::resource_tracker::ResourceTracker;
use bytemuck::cast_slice;
use wgpu::util::DeviceExt as _;
use wgpu::*;

/// Helper struct for texture and bind group management.
pub(super) struct TextureManager;

impl TextureManager {
    /// Create offscreen texture for opacity rendering.
    pub(super) fn create_offscreen_texture_static(
        gpu: &GpuContext,
        tex_width: u32,
        tex_height: u32,
    ) -> Texture {
        let offscreen_format = TextureFormat::Bgra8Unorm;
        gpu.device().create_texture(&TextureDescriptor {
            label: Some("offscreen-opacity-texture"),
            size: Extent3d {
                width: tex_width,
                height: tex_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: offscreen_format,
            usage: TextureUsages::RENDER_ATTACHMENT
                | TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    }

    /// Create bind group for opacity compositing with alpha blending.
    pub(super) fn create_opacity_bind_group_static(
        gpu: &GpuContext,
        pipelines: &PipelineCache,
        resources: &mut ResourceTracker,
        view: &TextureView,
        alpha: f32,
    ) -> BindGroup {
        let alpha_buf = gpu
            .device()
            .create_buffer_init(&util::BufferInitDescriptor {
                label: Some("opacity-alpha"),
                contents: cast_slice(&[
                    alpha, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32,
                ]),
                usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            });
        resources.track_buffer(alpha_buf.clone());

        gpu.device().create_bind_group(&BindGroupDescriptor {
            label: Some("opacity-tex-bind"),
            layout: pipelines.texture_bind_layout(),
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(pipelines.linear_sampler()),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: alpha_buf.as_entire_binding(),
                },
            ],
        })
    }
}
