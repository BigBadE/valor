//! WGPU backend implementation as a standalone crate within the renderer module.
//! This crate contains all WGPU-specific rendering code and depends on the renderer crate
//! for backend-agnostic types like `DisplayList` and `DrawText`.

/// Error handling utilities for WGPU operations.
mod error;
/// Logical encoder for command recording.
mod logical_encoder;
/// New pipeline builders for extended display items (Border, BoxShadow, Image, Gradients).
mod new_pipelines;
/// Offscreen rendering utilities.
pub mod offscreen;
/// Pipeline creation and vertex buffer management.
mod pipelines;
/// Main rendering state and implementation.
pub mod state;

/// Text rendering utilities for batching and scissoring.
mod text;

/// Texture pool for efficient reuse of offscreen textures.
mod texture_pool;

pub use error::{submit_with_validation, with_validation_scope};
pub use logical_encoder::LogicalEncoder;
pub use new_pipelines::{
    BorderVertex, BoxShadowVertex, GradientStop, GradientUniforms, GradientVertex, ImageVertex,
    build_border_pipeline, build_box_shadow_pipeline, build_gradient_pipeline,
    build_image_pipeline,
};
pub use offscreen::{
    PersistentGpuContext, initialize_persistent_context, render_display_list_to_rgba,
    render_display_list_with_context,
};
pub use pipelines::{Vertex, build_pipeline_and_buffers, build_texture_pipeline};
pub use state::{GlyphBounds, Layer, RenderState};
pub use texture_pool::TexturePool;
