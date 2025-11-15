//! Pipeline cache for WGPU backend.
//!
//! This module contains the `PipelineCache` struct which manages all rendering pipelines.
//! This is a focused component with a single responsibility: pipeline storage and retrieval.

use crate::pipelines::{
    build_offscreen_pipeline, build_pipeline_and_buffers, build_texture_pipeline,
};
use std::sync::Arc;
use wgpu::*;

/// Pipeline cache managing all rendering pipelines.
/// This struct has a single responsibility: storing and providing access to pipelines.
pub struct PipelineCache {
    /// Main rendering pipeline for rectangles with blending.
    main_pipeline: RenderPipeline,
    /// Offscreen rendering pipeline (no blending, for opacity groups).
    offscreen_pipeline: RenderPipeline,
    /// Textured quad rendering pipeline for compositing.
    texture_pipeline: RenderPipeline,
    /// Bind group layout for textured quads.
    texture_bind_layout: BindGroupLayout,
    /// Linear sampler for texture sampling.
    linear_sampler: Sampler,
    /// Initial vertex buffer (may be replaced during rendering).
    initial_vertex_buffer: Buffer,
    /// Initial vertex count.
    initial_vertex_count: u32,
}

impl PipelineCache {
    /// Create a new pipeline cache with all pipelines initialized.
    pub fn new(device: &Arc<Device>, render_format: TextureFormat) -> Self {
        let (main_pipeline, initial_vertex_buffer, initial_vertex_count) =
            build_pipeline_and_buffers(device, render_format);
        let offscreen_pipeline = build_offscreen_pipeline(device, render_format);
        let (texture_pipeline, texture_bind_layout, linear_sampler) =
            build_texture_pipeline(device, render_format);

        Self {
            main_pipeline,
            offscreen_pipeline,
            texture_pipeline,
            texture_bind_layout,
            linear_sampler,
            initial_vertex_buffer,
            initial_vertex_count,
        }
    }

    /// Get the main rendering pipeline.
    pub const fn main_pipeline(&self) -> &RenderPipeline {
        &self.main_pipeline
    }

    /// Get the offscreen rendering pipeline.
    pub const fn offscreen_pipeline(&self) -> &RenderPipeline {
        &self.offscreen_pipeline
    }

    /// Get the texture rendering pipeline.
    pub const fn texture_pipeline(&self) -> &RenderPipeline {
        &self.texture_pipeline
    }

    /// Get the texture bind group layout.
    pub const fn texture_bind_layout(&self) -> &BindGroupLayout {
        &self.texture_bind_layout
    }

    /// Get the linear sampler.
    pub const fn linear_sampler(&self) -> &Sampler {
        &self.linear_sampler
    }

    /// Get the initial vertex buffer.
    pub const fn initial_vertex_buffer(&self) -> &Buffer {
        &self.initial_vertex_buffer
    }

    /// Get the initial vertex count.
    pub const fn initial_vertex_count(&self) -> u32 {
        self.initial_vertex_count
    }
}
