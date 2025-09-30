//! Public API for the wgpu-based renderer backend and its DOM-mirroring interface.
pub mod display_list;
pub mod error;
pub mod pipelines;
pub mod renderer;
pub mod state;
pub mod text;
pub mod texture_pool;

pub use display_list::{DisplayItem, DisplayList, DisplayListDiff};
pub use renderer::{DrawRect, DrawText, RenderNode, RenderNodeKind, Renderer};
