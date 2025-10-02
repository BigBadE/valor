//! Backend-agnostic renderer crate exposing scene graph and display list.

pub mod backend;
pub mod compositor;
pub mod damage;
pub mod display_list;
pub mod paint;
pub mod render_graph;
pub mod renderer;
pub mod resource_pool;

pub use backend::{BackendMetrics, DebugMode, RenderBackend, RenderTarget};
pub use compositor::{OpacityCompositor, OpacityGroup, Rect};
pub use damage::{DamageRect, DamageTracker};
pub use display_list::{
    Batch, DisplayItem, DisplayList, DisplayListDiff, Quad, Scissor, StackingContextBoundary,
    TextBoundsPx, batch_display_list,
};
pub use paint::{DisplayListBuilder, StackingContext, StackingLevel, traverse_paint_tree};
pub use render_graph::{Dependency, OpacityComposite, PassId, RenderGraph, RenderPass, ResourceId};
pub use renderer::{DrawRect, DrawText, RenderNode, RenderNodeKind, Renderer, SnapshotEntry};
pub use resource_pool::{BindGroupHandle, BufferHandle, ResourcePool, TextureHandle};
