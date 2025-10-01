//! WGPU backend implementation as a standalone crate within the renderer module.
//! This crate contains all WGPU-specific rendering code and depends on the renderer crate
//! for backend-agnostic types like DisplayList and DrawText.

mod error;
mod logical_encoder;
mod offscreen;
mod pipelines;
pub mod state;
mod text;
mod texture_pool;

pub use error::{submit_with_validation, with_validation_scope};
pub use logical_encoder::LogicalEncoder;
pub use offscreen::render_display_list_to_rgba;
pub use pipelines::{Vertex, build_pipeline_and_buffers, build_texture_pipeline};
pub use state::{Layer, RenderState};
pub use texture_pool::TexturePool;
