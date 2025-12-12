//! Display item batching and vertex generation for opacity rendering.

use crate::pipelines::Vertex;
use crate::state::gpu_context::GpuContext;
use crate::state::resource_tracker::ResourceTracker;
use bytemuck::cast_slice;
use renderer::display_list::DisplayItem;
use renderer::display_list::{DisplayList, batch_display_list};
use wgpu::util::DeviceExt as _;
use wgpu::*;

/// Helper struct for batching display items.
pub(super) struct ItemBatcher<'state> {
    pub(super) gpu: &'state GpuContext,
    pub(super) resources: &'state mut ResourceTracker,
}

impl ItemBatcher<'_> {
    /// Draw display items in batches using specified viewport size.
    pub(super) fn draw_items_batched_with_size(
        &mut self,
        pass: &mut RenderPass<'_>,
        items: &[DisplayItem],
        width: u32,
        height: u32,
    ) {
        let sub = DisplayList::from_items(items.to_vec());
        let batches = batch_display_list(&sub, width, height);
        for batch in batches {
            let mut vertices: Vec<Vertex> = Vec::with_capacity(batch.quads.len() * 6);
            for quad in &batch.quads {
                Self::push_rect_vertices_ndc_with_size(
                    &mut vertices,
                    [quad.x, quad.y, quad.width, quad.height],
                    quad.color,
                    (width, height),
                );
            }
            let vertex_bytes = cast_slice(vertices.as_slice());
            let vertex_buffer = self
                .gpu
                .device()
                .create_buffer_init(&util::BufferInitDescriptor {
                    label: Some("layer-rect-batch"),
                    contents: vertex_bytes,
                    usage: BufferUsages::VERTEX,
                });
            self.resources.track_buffer(vertex_buffer.clone());
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            match batch.scissor {
                Some((scissor_x, scissor_y, scissor_width, scissor_height)) => {
                    let framebuffer_width = width.max(1);
                    let framebuffer_height = height.max(1);
                    let rect_x = scissor_x.min(framebuffer_width);
                    let rect_y = scissor_y.min(framebuffer_height);
                    let rect_width = scissor_width.min(framebuffer_width.saturating_sub(rect_x));
                    let rect_height = scissor_height.min(framebuffer_height.saturating_sub(rect_y));
                    if rect_width == 0 || rect_height == 0 {
                        continue;
                    }
                    pass.set_scissor_rect(rect_x, rect_y, rect_width, rect_height);
                }
                None => {
                    pass.set_scissor_rect(0, 0, width.max(1), height.max(1));
                }
            }
            if !vertices.is_empty() {
                pass.draw(0..(vertices.len() as u32), 0..1);
            }
        }
    }

    /// Push rectangle vertices in NDC coordinates with specified viewport size.
    fn push_rect_vertices_ndc_with_size(
        out: &mut Vec<Vertex>,
        rect_xywh: [f32; 4],
        color: [f32; 4],
        framebuffer_size: (u32, u32),
    ) {
        let (framebuffer_width, framebuffer_height) = framebuffer_size;
        let framebuffer_width = framebuffer_width.max(1) as f32;
        let framebuffer_height = framebuffer_height.max(1) as f32;
        let [rect_x, rect_y, rect_width, rect_height] = rect_xywh;
        if rect_width <= 0.0 || rect_height <= 0.0 {
            return;
        }
        let x0 = (rect_x / framebuffer_width).mul_add(2.0, -1.0);
        let x1 = ((rect_x + rect_width) / framebuffer_width).mul_add(2.0, -1.0);
        let y0 = (rect_y / framebuffer_height).mul_add(-2.0, 1.0);
        let y1 = ((rect_y + rect_height) / framebuffer_height).mul_add(-2.0, 1.0);
        out.extend_from_slice(&[
            Vertex {
                position: [x0, y0],
                color,
            },
            Vertex {
                position: [x1, y0],
                color,
            },
            Vertex {
                position: [x1, y1],
                color,
            },
            Vertex {
                position: [x0, y0],
                color,
            },
            Vertex {
                position: [x1, y1],
                color,
            },
            Vertex {
                position: [x0, y1],
                color,
            },
        ]);
    }
}
