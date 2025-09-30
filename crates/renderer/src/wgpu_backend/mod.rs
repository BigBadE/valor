//! WGPU backend implementation for the renderer crate.
//! This module contains all WGPU-specific rendering code.

pub mod error;
pub mod offscreen;
pub mod pipelines;
pub mod state;
pub(crate) mod text;
pub mod texture_pool;

pub use error::{submit_with_validation, with_validation_scope};
pub use offscreen::render_display_list_to_rgba;
pub use pipelines::{Vertex, build_pipeline_and_buffers, build_texture_pipeline};
pub use state::{Layer, RenderState};
pub use texture_pool::TexturePool;
