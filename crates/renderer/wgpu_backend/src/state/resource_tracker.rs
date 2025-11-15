//! Resource lifetime tracking for WGPU backend.
//!
//! This module contains the `ResourceTracker` struct which manages GPU resource
//! lifetimes to ensure resources remain valid through command buffer submission.
//! This is a focused component with a single responsibility: tracking GPU resource lifetimes.

use wgpu::*;

/// Resource tracker managing GPU resource lifetimes.
/// This struct has a single responsibility: keeping GPU resources alive through submission.
pub struct ResourceTracker {
    /// Textures that must remain alive until after submission.
    live_textures: Vec<Texture>,
    /// Buffers that must remain alive until after submission.
    live_buffers: Vec<Buffer>,
}

impl ResourceTracker {
    /// Create a new resource tracker with empty resource lists.
    pub const fn new() -> Self {
        Self {
            live_textures: Vec::new(),
            live_buffers: Vec::new(),
        }
    }

    /// Track a texture to keep it alive through submission.
    pub fn track_texture(&mut self, texture: Texture) {
        self.live_textures.push(texture);
    }

    /// Track a buffer to keep it alive through submission.
    pub fn track_buffer(&mut self, buffer: Buffer) {
        self.live_buffers.push(buffer);
    }

    /// Clear all tracked resources after submission completes.
    pub fn clear(&mut self) {
        self.live_textures.clear();
        self.live_buffers.clear();
    }
}

impl Default for ResourceTracker {
    fn default() -> Self {
        Self::new()
    }
}
