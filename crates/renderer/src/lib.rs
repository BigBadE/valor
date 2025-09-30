//! Backend-agnostic renderer crate exposing scene graph and display list.
pub mod display_list;
pub mod renderer;
pub mod wgpu_backend;

pub use display_list::{
    Batch, DisplayItem, DisplayList, DisplayListDiff, Quad, Scissor, StackingContextBoundary,
    TextBoundsPx, batch_display_list,
};
pub use renderer::{DrawRect, DrawText, RenderNode, RenderNodeKind, Renderer, SnapshotEntry};
pub use wgpu_backend::{Layer, RenderState, render_display_list_to_rgba};
