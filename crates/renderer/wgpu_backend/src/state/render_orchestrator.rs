//! Render orchestration for display list rendering.
//!
//! This module provides standalone functions that orchestrate high-level rendering operations:
//! - Coordinating rectangle and text rendering passes
//! - Managing layers and retained display lists
//! - Handling stacking contexts and opacity groups
//! - Batching and drawing display items

/// Bundle of rendering components passed to orchestration functions.
pub(super) struct RenderComponents<'render> {
    pub(super) gpu: &'render GpuContext,
    pub(super) pipelines: &'render PipelineCache,
    pub(super) text: &'render mut TextRendererState,
    pub(super) rectangles: &'render mut RectangleRenderer,
    pub(super) offscreen: &'render OffscreenTarget,
    pub(super) resources: &'render mut ResourceTracker,
}

impl RenderComponents<'_> {
    /// Create an opacity compositor from these components.
    pub(super) fn create_opacity_compositor(&mut self) -> OpacityCompositor<'_> {
        OpacityCompositor {
            gpu: self.gpu,
            pipelines: self.pipelines,
            text: self.text,
            _rectangles: self.rectangles,
            _offscreen: self.offscreen,
            resources: self.resources,
        }
    }
}

use super::error_scope::ErrorScopeGuard;
use super::gpu_context::GpuContext;
use super::offscreen_target::OffscreenTarget;
use super::opacity_compositor::{OpacityCompositor, OpacityExtraction};
use super::pipeline_cache::PipelineCache;
use super::rectangle_renderer::RectangleRenderer;
use super::resource_tracker::ResourceTracker;
use super::text_renderer_state::TextRendererState;
use super::{Layer, LayerEntry, OpacityComposite, ScissorRect};
use crate::pipelines::Vertex;
use crate::text::TextBatch;
use anyhow::Result as AnyResult;
use bytemuck::cast_slice;
use log::{debug, error};
use renderer::display_list::{
    DisplayItem, DisplayList, StackingContextBoundary, batch_display_list,
};
use renderer::renderer::DrawText;
use wgpu::util::DeviceExt as _;
use wgpu::*;

/// Render layers rectangles with opacity compositing.
pub(super) fn render_layers_rectangles(
    encoder: &mut CommandEncoder,
    texture_view: &TextureView,
    main_load: LoadOp<Color>,
    layers: &[Layer],
    components: &mut RenderComponents<'_>,
) {
    let per_layer: Vec<LayerEntry> = layers
        .iter()
        .map(|layer: &Layer| preprocess_layer_with_encoder(encoder, layer, components))
        .collect();

    let has_opacity = per_layer
        .iter()
        .any(|entry| matches!(entry, Some((_, comps, _)) if !comps.is_empty()));
    if has_opacity {
        debug!(target: "wgpu_renderer", ">>> Collected layer opacity groups (no mid-frame submission)");
    }

    render_layers_pass(encoder, texture_view, main_load, per_layer, components);
}

/// Render retained display list rectangles with opacity compositing.
///
/// # Errors
/// Returns an error if rendering or opacity composite collection fails.
pub(super) fn render_retained_rectangles(
    encoder: &mut CommandEncoder,
    texture_view: &TextureView,
    main_load: LoadOp<Color>,
    retained_display_list: Option<&DisplayList>,
    components: &mut RenderComponents<'_>,
) -> AnyResult<()> {
    let Some(display_list) = retained_display_list else {
        return Ok(());
    };
    let items: Vec<DisplayItem> = display_list.items.clone();

    // Two-Phase Rendering for Opacity:
    // Phase 1: Extract and pre-render all opacity layers to offscreen textures
    let phase1_scope = ErrorScopeGuard::push(components.gpu.device(), "phase1-extract-render");
    let extraction = {
        let mut compositor = components.create_opacity_compositor();
        compositor.extract_and_render_opacity_layers(encoder, &items)?
    };
    if let Err(err) = phase1_scope.check() {
        error!(target: "wgpu_renderer", "Phase 1 error scope caught error: {err:?}");
        return Err(err);
    }

    debug!(target: "wgpu_renderer", ">>> Phase 1 complete: {} opacity layers, {} clean items",
           extraction.layers.len(), extraction.clean_items.len());

    // Phase 2: Render clean display list (no stacking markers) + composite layers
    let phase2_scope = ErrorScopeGuard::push(components.gpu.device(), "phase2-render-composite");
    render_retained_pass_two_phase(encoder, texture_view, main_load, extraction, components);
    if let Err(err) = phase2_scope.check() {
        error!(target: "wgpu_renderer", "Phase 2 error scope caught error: {err:?}");
        return Err(err);
    }
    Ok(())
}

/// Render layers pass with opacity compositing.
fn render_layers_pass(
    encoder: &mut CommandEncoder,
    texture_view: &TextureView,
    main_load: LoadOp<Color>,
    per_layer: Vec<LayerEntry>,
    components: &mut RenderComponents<'_>,
) {
    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("main-pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: texture_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: main_load,
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        debug!(target: "wgpu_renderer", "start main-pass");
        pass.push_debug_group("main-pass(layers)");
        pass.set_pipeline(components.pipelines.main_pipeline());

        for (items, comps, ranges) in per_layer.into_iter().flatten() {
            draw_items_excluding_ranges(&mut pass, &items, &ranges, components);
            composite_groups(&mut pass, comps, components);
        }
        pass.pop_debug_group();
        debug!(target: "wgpu_renderer", "end main-pass");
    };
}

/// Draw items excluding specified ranges for opacity groups.
fn draw_items_excluding_ranges(
    pass: &mut RenderPass<'_>,
    items: &[DisplayItem],
    exclude: &[(usize, usize)],
    components: &mut RenderComponents<'_>,
) {
    let mut index = 0usize;
    let mut ex_idx = 0usize;
    while index < items.len() {
        if ex_idx < exclude.len() && index == exclude[ex_idx].0 {
            index = exclude[ex_idx].1 + 1;
            ex_idx += 1;
            continue;
        }
        let next = exclude.get(ex_idx).map_or(items.len(), |range| range.0);
        if index < next {
            drop(draw_items_with_groups(
                pass,
                &items[index..next],
                components,
            ));
            index = next;
        }
    }
}

/// Composite opacity groups by drawing textured quads with bind groups.
#[inline]
fn composite_groups(
    pass: &mut RenderPass<'_>,
    comps: Vec<OpacityComposite>,
    components: &mut RenderComponents<'_>,
) {
    for (_s, _e, tex, _view, _tw, _th, _alpha, bounds, bind_group) in comps {
        components.resources.track_texture(tex);
        let mut compositor = components.create_opacity_compositor();
        compositor.draw_texture_quad_with_bind_group(pass, &bind_group, bounds);
    }
}

/// Draw items with proper handling of stacking contexts.
///
/// # Errors
/// Returns an error if rendering nested stacking contexts fails.
pub(super) fn draw_items_with_groups(
    pass: &mut RenderPass<'_>,
    items: &[DisplayItem],
    components: &mut RenderComponents<'_>,
) -> AnyResult<()> {
    let mut index = 0usize;

    while index < items.len() {
        match &items[index] {
            DisplayItem::BeginStackingContext { boundary } => {
                let end = OpacityCompositor::find_stacking_context_end(items, index + 1);
                let group_items = &items[index + 1..end];

                match boundary {
                    StackingContextBoundary::Opacity { alpha } if *alpha < 1.0 => {
                        draw_opacity_group(pass, group_items, *alpha, components)?;
                    }
                    _ => {
                        draw_items_with_groups(pass, group_items, components)?;
                    }
                }

                index = end + 1;
            }
            DisplayItem::EndStackingContext => {
                index += 1;
            }
            _ => {
                let start = index;
                let mut end = index;
                while end < items.len() {
                    match &items[end] {
                        DisplayItem::BeginStackingContext { .. } => break,
                        _ => end += 1,
                    }
                }

                if start < end {
                    draw_items_batched(
                        pass,
                        &items[start..end],
                        components.gpu,
                        components.pipelines,
                        components.resources,
                    );
                }
                index = end;
            }
        }
    }
    Ok(())
}

/// Draw an opacity group with the specified alpha value.
///
/// # Errors
/// Returns an error if rendering fails.
#[inline]
fn draw_opacity_group(
    pass: &mut RenderPass<'_>,
    group_items: &[DisplayItem],
    alpha: f32,
    components: &mut RenderComponents<'_>,
) -> AnyResult<()> {
    let _: f32 = alpha;
    draw_items_with_groups(pass, group_items, components)
}

/// Render retained pass using two-phase opacity approach.
fn render_retained_pass_two_phase(
    encoder: &mut CommandEncoder,
    texture_view: &TextureView,
    main_load: LoadOp<Color>,
    extraction: OpacityExtraction,
    components: &mut RenderComponents<'_>,
) {
    let clean_items = &extraction.clean_items;
    let layers = extraction.layers;
    debug!(target: "wgpu_renderer", "=== PHASE 2: Rendering clean display list + compositing layers ===");
    debug!(target: "wgpu_renderer", "    Layers to composite: {}", layers.len());
    debug!(target: "wgpu_renderer", "    Clean items: {}", clean_items.len());

    {
        let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("main-pass-two-phase"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: texture_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: main_load,
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        debug!(target: "wgpu_renderer", "start main-pass-two-phase");
        pass.push_debug_group("main-pass(two-phase)");
        pass.set_pipeline(components.pipelines.main_pipeline());

        draw_items_batched(
            &mut pass,
            clean_items,
            components.gpu,
            components.pipelines,
            components.resources,
        );

        for layer in layers {
            components.resources.track_texture(layer.texture);
            let mut compositor = components.create_opacity_compositor();
            compositor.draw_texture_quad_with_bind_group(
                &mut pass,
                &layer.bind_group,
                layer.bounds,
            );
        }

        pass.pop_debug_group();
        debug!(target: "wgpu_renderer", "end main-pass-two-phase");
    };

    debug!(target: "wgpu_renderer", "=== PHASE 2 COMPLETE ===");
}

/// Preprocess a layer with the given encoder to collect opacity composites.
#[inline]
fn preprocess_layer_with_encoder(
    encoder: &mut CommandEncoder,
    layer: &Layer,
    components: &mut RenderComponents<'_>,
) -> LayerEntry {
    match layer {
        Layer::Background => None,
        Layer::Content(display_list) | Layer::Chrome(display_list) => {
            let items: Vec<DisplayItem> = display_list.items.clone();
            let mut compositor = components.create_opacity_compositor();
            let comps = compositor.collect_opacity_composites(encoder, &items);
            let ranges = build_exclude_ranges(&comps);
            Some((items, comps, ranges))
        }
    }
}

/// Build exclude ranges from opacity composites for rendering.
#[inline]
fn build_exclude_ranges(comps: &[OpacityComposite]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::with_capacity(comps.len());
    for (start, end, ..) in comps {
        ranges.push((*start, *end));
    }
    ranges
}

/// Draw display items in batches for efficient rendering.
#[inline]
pub(super) fn draw_items_batched(
    pass: &mut RenderPass<'_>,
    items: &[DisplayItem],
    gpu: &GpuContext,
    _pipelines: &PipelineCache,
    resources: &mut ResourceTracker,
) {
    let sub = DisplayList::from_items(items.to_vec());
    let batches = batch_display_list(&sub, gpu.size().width, gpu.size().height);
    for batch in batches {
        let mut vertices: Vec<Vertex> = Vec::with_capacity(batch.quads.len() * 6);
        for quad in &batch.quads {
            push_rect_vertices_ndc(
                &mut vertices,
                [quad.x, quad.y, quad.width, quad.height],
                quad.color,
                gpu,
            );
        }
        let vertex_bytes = cast_slice(vertices.as_slice());
        let vertex_buffer = gpu
            .device()
            .create_buffer_init(&util::BufferInitDescriptor {
                label: Some("layer-rect-batch"),
                contents: vertex_bytes,
                usage: BufferUsages::VERTEX,
            });
        resources.track_buffer(vertex_buffer.clone());
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        match batch.scissor {
            Some((scissor_x, scissor_y, scissor_width, scissor_height)) => {
                let framebuffer_width = gpu.size().width.max(1);
                let framebuffer_height = gpu.size().height.max(1);
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
                pass.set_scissor_rect(0, 0, gpu.size().width.max(1), gpu.size().height.max(1));
            }
        }
        if !vertices.is_empty() {
            pass.draw(0..(vertices.len() as u32), 0..1);
        }
    }
}

/// Push rectangle vertices in NDC coordinates.
#[inline]
fn push_rect_vertices_ndc(
    out: &mut Vec<Vertex>,
    rect_xywh: [f32; 4],
    color: [f32; 4],
    gpu: &GpuContext,
) {
    let framebuffer_width = gpu.size().width.max(1) as f32;
    let framebuffer_height = gpu.size().height.max(1) as f32;
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

/// Draw a batch of text items with optional scissor rect.
#[inline]
pub(super) fn draw_text_batch(
    pass: &mut RenderPass<'_>,
    _items: &[DrawText],
    scissor_opt: Option<ScissorRect>,
    gpu: &GpuContext,
    text: &TextRendererState,
) {
    pass.set_viewport(
        0.0,
        0.0,
        gpu.size().width as f32,
        gpu.size().height as f32,
        0.0,
        1.0,
    );
    match scissor_opt {
        Some((x, y, width, height)) => pass.set_scissor_rect(x, y, width, height),
        None => pass.set_scissor_rect(0, 0, gpu.size().width.max(1), gpu.size().height.max(1)),
    }
    {
        let scope = ErrorScopeGuard::push(gpu.device(), "glyphon-text-render");
        if let Err(error) = text.render(gpu.device(), pass) {
            error!(target: "wgpu_renderer", "Glyphon text_renderer.render() failed: {error:?}");
        }
        if let Err(error) = scope.check() {
            error!(target: "wgpu_renderer", "Glyphon text_renderer.render() generated validation error: {error:?}");
        }
    }
}

/// Draw multiple text batches with their respective scissor rects.
#[inline]
pub(super) fn draw_text_batches(
    pass: &mut RenderPass<'_>,
    batches: Vec<TextBatch>,
    gpu: &GpuContext,
    text: &TextRendererState,
) {
    for (scissor_opt, items) in batches.into_iter().filter(|(_, items)| !items.is_empty()) {
        draw_text_batch(pass, &items, scissor_opt, gpu, text);
    }
}
