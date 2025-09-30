//! WGPU renderer backend (legacy crate). Kept temporarily for compatibility.
//! Exposes modules used by dependents and tests.

// Use the renderer crate's display list types to keep a single definition across the workspace.
pub use ::renderer::display_list;
pub mod error;
pub mod offscreen;
pub mod pipelines;
pub mod renderer;
pub mod state;
pub mod text;
pub mod texture_pool;
