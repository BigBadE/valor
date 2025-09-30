//! WGPU backend implementation module.
//! This module contains all WGPU-specific rendering code.

mod error;
mod offscreen;
mod pipelines;
pub mod state;
mod text;
mod texture_pool;

pub use error::{submit_with_validation, with_validation_scope};
pub use offscreen::render_display_list_to_rgba;
pub use pipelines::{Vertex, build_pipeline_and_buffers, build_texture_pipeline};
pub use state::{Layer, RenderState};
pub use texture_pool::TexturePool;
