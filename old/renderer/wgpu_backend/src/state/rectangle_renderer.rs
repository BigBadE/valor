//! Rectangle rendering for WGPU backend.
//!
//! This module contains the `RectangleRenderer` struct which manages vertex buffers
//! and rectangle rendering. This is a focused component with a single responsibility:
//! rendering rectangles.

use wgpu::*;

/// Rectangle renderer managing vertex buffers and rendering.
/// This struct has a single responsibility: rendering rectangles via vertex buffers.
pub struct RectangleRenderer {
    /// Current vertex buffer for rendering.
    vertex_buffer: Buffer,
    /// Number of vertices in the vertex buffer.
    vertex_count: u32,
}

impl RectangleRenderer {
    /// Create a new rectangle renderer with an initial vertex buffer.
    pub const fn new(initial_buffer: Buffer, initial_count: u32) -> Self {
        Self {
            vertex_buffer: initial_buffer,
            vertex_count: initial_count,
        }
    }

    /// Set the vertex buffer.
    pub fn set_vertex_buffer(&mut self, buffer: Buffer) {
        self.vertex_buffer = buffer;
    }

    /// Set the vertex count.
    pub fn set_vertex_count(&mut self, count: u32) {
        self.vertex_count = count;
    }

    /// Render rectangles using the current vertex buffer.
    pub fn render(&self, pass: &mut RenderPass<'_>) {
        if self.vertex_count > 0 {
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.draw(0..self.vertex_count, 0..1);
        }
    }
}
