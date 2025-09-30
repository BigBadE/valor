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

pub mod display_list;
pub mod renderer;

pub use display_list::{
    Batch, DisplayItem, DisplayList, DisplayListDiff, Quad, Scissor, StackingContextBoundary,
    TextBoundsPx, batch_display_list,
};
pub use renderer::{DrawRect, DrawText, RenderNode, RenderNodeKind, Renderer, SnapshotEntry};
