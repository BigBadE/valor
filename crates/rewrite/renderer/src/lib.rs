//! Backend-agnostic renderer for the rewrite architecture.
//!
//! This crate provides display list types and rendering abstractions.

pub mod backend;
pub mod builder;
pub mod display_list;
pub mod renderer;
pub mod tile;

pub use backend::{BackendCapabilities, BackendError, RenderBackend};
pub use builder::{DisplayListBuilder, build_display_list};
pub use display_list::DisplayList;
pub use renderer::Renderer;
pub use tile::RenderTile;

/// Main rendering coordinator.
pub struct RenderHandler {
    tiles: Vec<RenderTile>,
}

impl RenderHandler {
    pub fn new() -> Self {
        Self { tiles: Vec::new() }
    }

    pub fn tiles(&self) -> &[RenderTile] {
        &self.tiles
    }

    pub fn tiles_mut(&mut self) -> &mut Vec<RenderTile> {
        &mut self.tiles
    }
}

impl Default for RenderHandler {
    fn default() -> Self {
        Self::new()
    }
}
