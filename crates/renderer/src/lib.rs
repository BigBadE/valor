//! Backend-agnostic renderer crate exposing scene graph and display list.
#![allow(
    clippy::missing_docs_in_private_items,
    clippy::missing_inline_in_public_items,
    clippy::min_ident_chars,
    clippy::missing_panics_doc,
    clippy::doc_markdown,
    clippy::module_name_repetitions,
    clippy::self_named_module_files,
    reason = "Renderer crate uses legacy file structure and simplified documentation"
)]

pub mod compositor;
pub mod display_list;
pub mod render_graph;
pub mod renderer;
pub mod resource_pool;

pub use compositor::{OpacityCompositor, OpacityGroup, Rect};
pub use display_list::{
    Batch, DisplayItem, DisplayList, DisplayListDiff, Quad, Scissor, StackingContextBoundary,
    TextBoundsPx, batch_display_list,
};
pub use render_graph::{Dependency, OpacityComposite, PassId, RenderGraph, RenderPass, ResourceId};
pub use renderer::{DrawRect, DrawText, RenderNode, RenderNodeKind, Renderer, SnapshotEntry};
pub use resource_pool::{BindGroupHandle, BufferHandle, ResourcePool, TextureHandle};
