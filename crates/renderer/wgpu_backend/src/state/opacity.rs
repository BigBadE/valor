//! Opacity compositing and offscreen rendering.
//!
//! This module contains extracted API functions for opacity compositing.
//! Many functions are intentionally unused as they provide the API for the refactored module.

use super::error_scope::ErrorScopeGuard;
use super::rectangles::{DrawBatchedParams, draw_items_batched};
use super::text::{GlyphonPrepareParams, GlyphonState, glyphon_prepare_for};
use crate::text::map_text_item;
use bytemuck::cast_slice;
use log::debug;
use renderer::compositor::OpacityCompositor;
use renderer::display_list::{DisplayItem, StackingContextBoundary};
use renderer::renderer::DrawText;
use std::sync::Arc;
use wgpu::util::DeviceExt as _;
use wgpu::*;
use winit::dpi::PhysicalSize;
use winit::window::Window;

/// Result type for offscreen rendering operations.
pub type OffscreenRenderResult = (Texture, TextureView, u32, u32, BindGroup);

/// Composite info for a pre-rendered opacity group.
/// Contains (`start_index`, `end_index`, `texture`, `texture_view`, `tex_w`, `tex_h`, `alpha`, `bounds`, `bind_group`).
pub type OpacityComposite = (
    usize,
    usize,
    Texture,
    TextureView,
    u32,
    u32,
    f32,
    Bounds,
    BindGroup,
);

/// Pixel bounds (x, y, width, height)
pub type Bounds = (f32, f32, f32, f32);

/// Vertex structure for texture quad rendering.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct TexVertex {
    /// Position in NDC coordinates.
    pos: [f32; 2],
    /// UV texture coordinates.
    tex_coords: [f32; 2],
}

/// Rendering context that encapsulates viewport and size information.
/// This is passed as a parameter instead of mutating shared state.
#[allow(dead_code, reason = "API type for extracted opacity module")]
#[derive(Debug, Copy, Clone)]
pub struct RenderContext {
    /// The viewport size for rendering.
    viewport_size: PhysicalSize<u32>,
}

impl RenderContext {
    /// Create a new render context with the given size.
    pub(crate) const fn new(size: PhysicalSize<u32>) -> Self {
        Self {
            viewport_size: size,
        }
    }

    /// Get the viewport width (minimum 1).
    pub(crate) fn width(self) -> u32 {
        self.viewport_size.width.max(1)
    }

    /// Get the viewport height (minimum 1).
    pub(crate) fn height(self) -> u32 {
        self.viewport_size.height.max(1)
    }
}

/// Parameters for offscreen rendering passes.
#[allow(dead_code, reason = "API type for extracted opacity module")]
pub struct OffscreenRenderParams<'render> {
    /// Command encoder for recording render commands.
    pub encoder: &'render mut CommandEncoder,
    /// Texture view to render into.
    pub view: &'render TextureView,
    /// Display items translated to local coordinates.
    pub translated_items: &'render [DisplayItem],
    /// Texture width in pixels.
    pub tex_width: u32,
    /// Texture height in pixels.
    pub tex_height: u32,
    /// Render context with viewport information.
    pub ctx: RenderContext,
}

/// Translate display items to texture-local coordinates.
#[allow(dead_code, reason = "API function for extracted opacity module")]
pub fn translate_items_to_local(
    items: &[DisplayItem],
    offset_x: f32,
    offset_y: f32,
) -> Vec<DisplayItem> {
    items
        .iter()
        .map(|item| match item {
            DisplayItem::Rect {
                x: rect_x,
                y: rect_y,
                width: rect_width,
                height: rect_height,
                color,
            } => DisplayItem::Rect {
                x: rect_x - offset_x,
                y: rect_y - offset_y,
                width: *rect_width,
                height: *rect_height,
                color: *color,
            },
            DisplayItem::Text {
                x: text_x,
                y: text_y,
                text,
                color,
                font_size,
                bounds: text_bounds,
            } => DisplayItem::Text {
                x: text_x - offset_x,
                y: text_y - offset_y,
                text: text.clone(),
                color: *color,
                font_size: *font_size,
                bounds: text_bounds.map(|(left, top, right, bottom)| {
                    (
                        (left as f32 - offset_x) as i32,
                        (top as f32 - offset_y) as i32,
                        (right as f32 - offset_x) as i32,
                        (bottom as f32 - offset_y) as i32,
                    )
                }),
            },
            other => other.clone(),
        })
        .collect()
}

/// Parameters for opacity composite collection.
#[allow(dead_code, reason = "API type for extracted opacity module")]
pub struct CollectOpacityParams<'collect> {
    /// Command encoder for offscreen rendering.
    pub encoder: &'collect mut CommandEncoder,
    /// Display items to process.
    pub items: &'collect [DisplayItem],
}

/// Parameters for offscreen rendering creation.
#[allow(dead_code, reason = "API type for extracted opacity module")]
pub struct OffscreenCreationParams<'offscreen> {
    /// GPU device.
    pub device: &'offscreen Arc<Device>,
    /// Render format.
    pub render_format: TextureFormat,
    /// Rectangle pipeline.
    pub pipeline: &'offscreen RenderPipeline,
    /// Live buffers for resource management.
    pub live_buffers: &'offscreen mut Vec<Buffer>,
}

/// Parameters for bind group creation.
#[allow(dead_code, reason = "API type for extracted opacity module")]
pub struct BindGroupParams<'bind> {
    /// GPU device.
    pub device: &'bind Arc<Device>,
    /// Texture bind layout.
    pub tex_bind_layout: &'bind BindGroupLayout,
    /// Linear sampler.
    pub linear_sampler: &'bind Sampler,
    /// Live buffers for resource management.
    pub live_buffers: &'bind mut Vec<Buffer>,
}

/// Render rectangles to offscreen texture.
///
/// # Errors
/// Returns an error if rendering fails.
fn render_offscreen_rects_pass(
    pass_params: &mut OffscreenRenderParams<'_>,
    creation_params: &mut OffscreenCreationParams<'_>,
    current_size: PhysicalSize<u32>,
) {
    debug!(target: "wgpu_renderer", ">>> CREATING offscreen rects pass");
    let mut pass = pass_params
        .encoder
        .begin_render_pass(&RenderPassDescriptor {
            label: Some("opacity-offscreen-pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: pass_params.view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::TRANSPARENT),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
    debug!(target: "wgpu_renderer", "    Pass created, setting viewport and pipeline");
    pass.set_viewport(
        0.0,
        0.0,
        pass_params.tex_width as f32,
        pass_params.tex_height as f32,
        0.0,
        1.0,
    );
    pass.set_pipeline(creation_params.pipeline);
    debug!(target: "wgpu_renderer", "    Drawing items");

    // Use temporary size for rendering context
    let old_size = current_size;
    let new_size = PhysicalSize::new(pass_params.ctx.width(), pass_params.ctx.height());

    // Draw items with batching - simplified without stacking context recursion
    draw_items_batched(
        &mut pass,
        &mut DrawBatchedParams {
            device: creation_params.device,
            items: pass_params.translated_items,
            size: new_size,
            pipeline: creation_params.pipeline,
            live_buffers: creation_params.live_buffers,
        },
    );

    let _: PhysicalSize<u32> = old_size; // Acknowledge old_size
    debug!(target: "wgpu_renderer", "<<< Pass DROPPED");
}

/// Render text to offscreen texture.
fn render_offscreen_text_pass(
    params: &mut OffscreenRenderParams<'_>,
    glyphon_state: &mut GlyphonState,
    window: &Arc<Window>,
    device: &Arc<Device>,
    queue: &Queue,
) {
    let text_items: Vec<DrawText> = params
        .translated_items
        .iter()
        .filter_map(map_text_item)
        .collect();
    if text_items.is_empty() {
        return;
    }

    debug!(target: "wgpu_renderer", ">>> CREATING offscreen text pass");
    glyphon_prepare_for(
        glyphon_state,
        &GlyphonPrepareParams {
            device,
            queue,
            window,
            size: PhysicalSize::new(params.tex_width, params.tex_height),
            items: text_items.as_slice(),
        },
    );
    let mut text_pass = params.encoder.begin_render_pass(&RenderPassDescriptor {
        label: Some("opacity-offscreen-text-pass"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: params.view,
            depth_slice: None,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Load,
                store: StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    });
    debug!(target: "wgpu_renderer", "    Pass created, drawing text");
    text_pass.set_viewport(
        0.0,
        0.0,
        params.tex_width as f32,
        params.tex_height as f32,
        0.0,
        1.0,
    );

    // Simplified text rendering without context switching
    text_pass.set_viewport(
        0.0,
        0.0,
        params.tex_width as f32,
        params.tex_height as f32,
        0.0,
        1.0,
    );
    text_pass.set_scissor_rect(0, 0, params.tex_width.max(1), params.tex_height.max(1));
    {
        let scope = ErrorScopeGuard::push(device, "glyphon-text-render");
        if let Err(error) = glyphon_state.text_renderer.render(
            &glyphon_state.text_atlas,
            &glyphon_state.viewport,
            &mut text_pass,
        ) {
            log::error!(target: "wgpu_renderer", "Glyphon text_renderer.render() failed: {error:?}");
        }
        if let Err(error) = scope.check() {
            log::error!(target: "wgpu_renderer", "Glyphon text_renderer.render() generated validation error: {error:?}");
        }
    }

    debug!(target: "wgpu_renderer", "<<< Text pass DROPPED");
}

/// Create offscreen texture for opacity compositing.
#[allow(dead_code, reason = "API function for extracted opacity module")]
pub fn create_offscreen_texture(
    device: &Device,
    render_format: TextureFormat,
    tex_width: u32,
    tex_height: u32,
) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: Some("offscreen-opacity-texture"),
        size: Extent3d {
            width: tex_width,
            height: tex_height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: render_format,
        usage: TextureUsages::RENDER_ATTACHMENT
            | TextureUsages::TEXTURE_BINDING
            | TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// Create bind group for opacity compositing with alpha blending.
#[allow(dead_code, reason = "API function for extracted opacity module")]
pub fn create_opacity_bind_group(
    params: &mut BindGroupParams<'_>,
    view: &TextureView,
    alpha: f32,
) -> BindGroup {
    let alpha_buf = params
        .device
        .create_buffer_init(&util::BufferInitDescriptor {
            label: Some("opacity-alpha"),
            contents: cast_slice(&[alpha, 0.0f32, 0.0f32, 0.0f32]),
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        });
    params.live_buffers.push(alpha_buf.clone());

    params.device.create_bind_group(&BindGroupDescriptor {
        label: Some("opacity-tex-bind"),
        layout: params.tex_bind_layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(view),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::Sampler(params.linear_sampler),
            },
            BindGroupEntry {
                binding: 2,
                resource: alpha_buf.as_entire_binding(),
            },
        ],
    })
}

/// Full parameters bundle for offscreen rendering.
#[allow(dead_code, reason = "API type for extracted opacity module")]
pub struct FullOffscreenParams<'full> {
    /// Creation parameters.
    pub creation: OffscreenCreationParams<'full>,
    /// Bind group parameters.
    pub bind_group: BindGroupParams<'full>,
    /// Glyphon state.
    pub glyphon_state: &'full mut GlyphonState,
    /// Window reference.
    pub window: &'full Arc<Window>,
    /// GPU queue.
    pub queue: &'full Queue,
    /// Current size.
    pub current_size: PhysicalSize<u32>,
}

/// Render items to offscreen texture with bind group for opacity compositing.
///
/// # Errors
/// Returns an error if rendering fails.
#[allow(dead_code, reason = "API function for extracted opacity module")]
pub fn render_items_to_offscreen_bounded_with_bind_group(
    encoder: &mut CommandEncoder,
    items: &[DisplayItem],
    bounds: Bounds,
    alpha: f32,
    params: &mut FullOffscreenParams<'_>,
) -> OffscreenRenderResult {
    let (x, y, width, height) = bounds;
    let tex_width = (width.ceil() as u32).max(1);
    let tex_height = (height.ceil() as u32).max(1);

    debug!(target: "wgpu_renderer", "render_items_to_offscreen_bounded: bounds=({}, {}, {}, {}), tex_size={}x{}, items={}",
        x, y, width, height, tex_width, tex_height, items.len());

    let texture = create_offscreen_texture(
        params.creation.device,
        params.creation.render_format,
        tex_width,
        tex_height,
    );
    let view = texture.create_view(&TextureViewDescriptor {
        label: Some("offscreen-opacity-view"),
        format: Some(params.creation.render_format),
        ..Default::default()
    });

    let ctx = RenderContext::new(PhysicalSize::new(tex_width, tex_height));
    let translated_items = translate_items_to_local(items, x, y);

    render_offscreen_rects_pass(
        &mut OffscreenRenderParams {
            encoder,
            view: &view,
            translated_items: &translated_items,
            tex_width,
            tex_height,
            ctx,
        },
        &mut OffscreenCreationParams {
            device: params.creation.device,
            render_format: params.creation.render_format,
            pipeline: params.creation.pipeline,
            live_buffers: params.creation.live_buffers,
        },
        params.current_size,
    );
    render_offscreen_text_pass(
        &mut OffscreenRenderParams {
            encoder,
            view: &view,
            translated_items: &translated_items,
            tex_width,
            tex_height,
            ctx,
        },
        params.glyphon_state,
        params.window,
        params.bind_group.device,
        params.queue,
    );

    debug!(target: "wgpu_renderer", "Offscreen render passes complete, creating bind group");
    let bind_group = create_opacity_bind_group(
        &mut BindGroupParams {
            device: params.bind_group.device,
            tex_bind_layout: params.bind_group.tex_bind_layout,
            linear_sampler: params.bind_group.linear_sampler,
            live_buffers: params.bind_group.live_buffers,
        },
        &view,
        alpha,
    );
    debug!(target: "wgpu_renderer", "Bind group created, texture ready for compositing");

    (texture, view, tex_width, tex_height, bind_group)
}

/// Collect opacity composites from display items for offscreen rendering.
#[allow(dead_code, reason = "API function for extracted opacity module")]
pub fn collect_opacity_composites(
    params: &mut CollectOpacityParams<'_>,
    mut full_params: FullOffscreenParams<'_>,
) -> Vec<OpacityComposite> {
    let mut out: Vec<OpacityComposite> = Vec::new();
    let mut index = 0usize;
    while index < params.items.len() {
        if let DisplayItem::BeginStackingContext { boundary } = &params.items[index]
            && matches!(boundary, StackingContextBoundary::Opacity { alpha } if *alpha < 1.0)
        {
            let end = OpacityCompositor::find_stacking_context_end(params.items, index + 1);
            let group_items = &params.items[index + 1..end];
            let alpha = match boundary {
                StackingContextBoundary::Opacity { alpha } => *alpha,
                _ => 1.0,
            };
            let bounds = OpacityCompositor::compute_items_bounds(group_items)
                .unwrap_or((0.0, 0.0, 1.0, 1.0));
            let (tex, view, tex_width, tex_height, bind_group) =
                render_items_to_offscreen_bounded_with_bind_group(
                    params.encoder,
                    group_items,
                    bounds,
                    alpha,
                    &mut full_params,
                );
            // Offscreen render pass is complete. Texture will be ready to sample after
            // the encoder is submitted (done by caller after all opacity groups are collected).
            out.push((
                index, end, tex, view, tex_width, tex_height, alpha, bounds, bind_group,
            ));
            index = end + 1;
            continue;
        }
        index += 1;
    }
    out
}

/// Build exclude ranges from opacity composites for rendering.
#[allow(dead_code, reason = "API function for extracted opacity module")]
#[inline]
pub fn build_exclude_ranges(comps: &[OpacityComposite]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::with_capacity(comps.len());
    for (start, end, ..) in comps {
        ranges.push((*start, *end));
    }
    ranges
}

/// Parameters for texture quad drawing.
#[allow(dead_code, reason = "API type for extracted opacity module")]
pub struct DrawTextureQuadParams<'quad> {
    /// GPU device.
    pub device: &'quad Arc<Device>,
    /// Texture pipeline.
    pub tex_pipeline: &'quad RenderPipeline,
    /// Current framebuffer size.
    pub size: PhysicalSize<u32>,
    /// Live buffers for resource management.
    pub live_buffers: &'quad mut Vec<Buffer>,
}

/// Draw a textured quad using a pre-created bind group (called from within render pass).
#[allow(dead_code, reason = "API function for extracted opacity module")]
pub fn draw_texture_quad_with_bind_group(
    pass: &mut RenderPass<'_>,
    params: &mut DrawTextureQuadParams<'_>,
    bind_group: &BindGroup,
    bounds: Bounds,
) {
    let (rect_x, rect_y, rect_width, rect_height) = bounds;
    debug!(target: "wgpu_renderer", ">>> draw_texture_quad_with_bind_group: bounds=({rect_x}, {rect_y}, {rect_width}, {rect_height})");

    // Build a quad covering the group's bounds with UVs 0..1 over the offscreen texture
    let framebuffer_width = params.size.width.max(1) as f32;
    let framebuffer_height = params.size.height.max(1) as f32;
    let x0 = (rect_x / framebuffer_width).mul_add(2.0, -1.0);
    let x1 = ((rect_x + rect_width) / framebuffer_width).mul_add(2.0, -1.0);
    let y0 = (rect_y / framebuffer_height).mul_add(-2.0, 1.0);
    let y1 = ((rect_y + rect_height) / framebuffer_height).mul_add(-2.0, 1.0);
    // UVs cover the full offscreen texture [0,1]
    let uv_left = 0.0;
    let uv_top = 0.0;
    let uv_right = 1.0;
    let uv_bottom = 1.0;
    let verts = [
        TexVertex {
            pos: [x0, y0],
            tex_coords: [uv_left, uv_bottom],
        },
        TexVertex {
            pos: [x1, y0],
            tex_coords: [uv_right, uv_bottom],
        },
        TexVertex {
            pos: [x1, y1],
            tex_coords: [uv_right, uv_top],
        },
        TexVertex {
            pos: [x0, y0],
            tex_coords: [uv_left, uv_bottom],
        },
        TexVertex {
            pos: [x1, y1],
            tex_coords: [uv_right, uv_top],
        },
        TexVertex {
            pos: [x0, y1],
            tex_coords: [uv_left, uv_top],
        },
    ];
    let vertex_buffer = params
        .device
        .create_buffer_init(&util::BufferInitDescriptor {
            label: Some("opacity-quad-vertices"),
            contents: cast_slice(&verts),
            usage: BufferUsages::VERTEX,
        });
    params.live_buffers.push(vertex_buffer.clone());

    // Constrain drawing to the group's bounds to avoid edge bleed and match compositing region.
    pass.set_pipeline(params.tex_pipeline);

    let scissor_x = rect_x.max(0.0).floor() as u32;
    let scissor_y = rect_y.max(0.0).floor() as u32;
    let scissor_width = rect_width.max(0.0).ceil() as u32;
    let scissor_height = rect_height.max(0.0).ceil() as u32;

    let framebuffer_width_u32 = params.size.width.max(1);
    let framebuffer_height_u32 = params.size.height.max(1);
    let final_x = scissor_x.min(framebuffer_width_u32);
    let final_y = scissor_y.min(framebuffer_height_u32);
    let final_width = scissor_width.min(framebuffer_width_u32.saturating_sub(final_x));
    let final_height = scissor_height.min(framebuffer_height_u32.saturating_sub(final_y));
    if final_width == 0 || final_height == 0 {
        // Nothing visible; skip draw for this batch
        return;
    }
    pass.set_scissor_rect(final_x, final_y, final_width, final_height);
    pass.set_vertex_buffer(0, vertex_buffer.slice(..));
    pass.set_bind_group(0, bind_group, &[]);
    pass.draw(0..6, 0..1);
}
