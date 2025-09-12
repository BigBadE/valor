//! Public API for the wgpu-based renderer backend and its DOM-mirroring interface.
pub mod state;
pub mod renderer;
pub mod display_list;

pub use renderer::{Renderer, RenderNode, RenderNodeKind, DrawRect, DrawText};
pub use display_list::{DisplayList, DisplayItem, DisplayListDiff};