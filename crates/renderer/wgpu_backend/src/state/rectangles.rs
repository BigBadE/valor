//! Rectangle rendering for the WGPU backend.

use crate::pipelines::Vertex;
use bytemuck::cast_slice;
use renderer::display_list::{DisplayItem, DisplayList, batch_display_list};
use renderer::renderer::DrawRect;
use std::sync::Arc;
use wgpu::util::DeviceExt as _;
use wgpu::*;
use winit::dpi::PhysicalSize;

/// Push rectangle vertices in NDC coordinates to the vertex buffer.
#[allow(dead_code, reason = "API function for extracted rectangles module")]
#[inline]
pub fn push_rect_vertices_ndc(
    out: &mut Vec<Vertex>,
    rect_xywh: [f32; 4],
    color: [f32; 4],
    framebuffer_width: f32,
    framebuffer_height: f32,
) {
    let [rect_x, rect_y, rect_width, rect_height] = rect_xywh;
    if rect_width <= 0.0 || rect_height <= 0.0 {
        return;
    }
    let x0 = (rect_x / framebuffer_width).mul_add(2.0, -1.0);
    let x1 = ((rect_x + rect_width) / framebuffer_width).mul_add(2.0, -1.0);
    let y0 = (rect_y / framebuffer_height).mul_add(-2.0, 1.0);
    let y1 = ((rect_y + rect_height) / framebuffer_height).mul_add(-2.0, 1.0);
    // Pass through color; shader handles sRGB->linear conversion for blending.
    let vertex_color = color;
    out.extend_from_slice(&[
        Vertex {
            position: [x0, y0],
            color: vertex_color,
        },
        Vertex {
            position: [x1, y0],
            color: vertex_color,
        },
        Vertex {
            position: [x1, y1],
            color: vertex_color,
        },
        Vertex {
            position: [x0, y0],
            color: vertex_color,
        },
        Vertex {
            position: [x1, y1],
            color: vertex_color,
        },
        Vertex {
            position: [x0, y1],
            color: vertex_color,
        },
    ]);
}

/// Parameters for batched rectangle drawing.
#[allow(dead_code, reason = "API type for extracted rectangles module")]
pub struct DrawBatchedParams<'params> {
    /// GPU device for creating resources.
    pub device: &'params Arc<Device>,
    /// Display items to batch.
    pub items: &'params [DisplayItem],
    /// Current framebuffer size.
    pub size: PhysicalSize<u32>,
    /// Rectangle rendering pipeline.
    pub pipeline: &'params RenderPipeline,
    /// Live buffers to keep alive through submission.
    pub live_buffers: &'params mut Vec<Buffer>,
}

/// Draw display items in batches for efficient rendering.
#[allow(dead_code, reason = "API function for extracted rectangles module")]
#[inline]
pub fn draw_items_batched(pass: &mut RenderPass<'_>, params: &mut DrawBatchedParams<'_>) {
    let DrawBatchedParams {
        device,
        items,
        size,
        pipeline: _pipeline,
        live_buffers,
    } = params;

    let sub = DisplayList::from_items(items.to_vec());
    let batches = batch_display_list(&sub, size.width, size.height);
    for batch in batches {
        let mut vertices: Vec<Vertex> = Vec::with_capacity(batch.quads.len() * 6);
        let framebuffer_width = size.width.max(1) as f32;
        let framebuffer_height = size.height.max(1) as f32;
        for quad in &batch.quads {
            push_rect_vertices_ndc(
                &mut vertices,
                [quad.x, quad.y, quad.width, quad.height],
                quad.color,
                framebuffer_width,
                framebuffer_height,
            );
        }
        let vertex_bytes = cast_slice(vertices.as_slice());
        let vertex_buffer = device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("layer-rect-batch"),
            contents: vertex_bytes,
            usage: BufferUsages::VERTEX,
        });
        // Keep buffer alive until submission to avoid backend lifetime edge-cases
        live_buffers.push(vertex_buffer.clone());
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        match batch.scissor {
            Some((scissor_x, scissor_y, scissor_width, scissor_height)) => {
                let fb_width = size.width.max(1);
                let fb_height = size.height.max(1);
                let rect_x = scissor_x.min(fb_width);
                let rect_y = scissor_y.min(fb_height);
                let rect_width = scissor_width.min(fb_width.saturating_sub(rect_x));
                let rect_height = scissor_height.min(fb_height.saturating_sub(rect_y));
                if rect_width == 0 || rect_height == 0 {
                    // Nothing visible; skip draw for this batch
                    continue;
                }
                pass.set_scissor_rect(rect_x, rect_y, rect_width, rect_height);
            }
            None => {
                pass.set_scissor_rect(0, 0, size.width.max(1), size.height.max(1));
            }
        }
        if !vertices.is_empty() {
            // Draw if we generated any geometry
            pass.draw(0..(vertices.len() as u32), 0..1);
        }
    }
}

/// Create immediate mode vertices from rectangle list.
#[allow(dead_code, reason = "API function for extracted rectangles module")]
pub fn create_immediate_vertices(
    display_list: &[DrawRect],
    size: PhysicalSize<u32>,
) -> Vec<Vertex> {
    let mut vertices: Vec<Vertex> = Vec::with_capacity(display_list.len() * 6);
    let framebuffer_width = size.width.max(1) as f32;
    let framebuffer_height = size.height.max(1) as f32;
    for rect in display_list {
        let rgba = [rect.color[0], rect.color[1], rect.color[2], 1.0];
        push_rect_vertices_ndc(
            &mut vertices,
            [rect.x, rect.y, rect.width, rect.height],
            rgba,
            framebuffer_width,
            framebuffer_height,
        );
    }
    vertices
}
