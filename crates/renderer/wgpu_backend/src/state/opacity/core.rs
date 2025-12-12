//! Core opacity compositor struct and extraction logic.

use super::offscreen::OffscreenRenderer;
use crate::error::submit_with_validation;
use crate::state::gpu_context::GpuContext;
use crate::state::offscreen_target::OffscreenTarget;
use crate::state::pipeline_cache::PipelineCache;
use crate::state::rectangle_renderer::RectangleRenderer;
use crate::state::resource_tracker::ResourceTracker;
use crate::state::text_renderer_state::TextRendererState;
use crate::state::{Bounds, OpacityComposite, TexVertex};
use anyhow::Result as AnyResult;
use bytemuck::cast_slice;
use log::debug;
use renderer::display_list::{DisplayItem, StackingContextBoundary};
use wgpu::util::DeviceExt as _;
use wgpu::*;

/// Pre-rendered opacity layer for two-phase compositing.
pub struct OpacityLayer {
    pub(crate) bounds: Bounds,
    pub(crate) texture: Texture,
    pub(crate) bind_group: BindGroup,
}

/// Result of extracting opacity layers from a display list.
pub struct OpacityExtraction {
    pub(crate) layers: Vec<OpacityLayer>,
    pub(crate) clean_items: Vec<DisplayItem>,
}

/// Component responsible for opacity compositing and offscreen rendering.
///
/// This component extracts opacity groups from display lists, renders them
/// to offscreen textures with alpha blending, and composites them back into
/// the main framebuffer.
pub struct OpacityCompositor<'state> {
    pub(in crate::state) gpu: &'state GpuContext,
    pub(in crate::state) pipelines: &'state PipelineCache,
    pub(in crate::state) text: &'state mut TextRendererState,
    pub(in crate::state) _rectangles: &'state mut RectangleRenderer,
    pub(in crate::state) _offscreen: &'state OffscreenTarget,
    pub(in crate::state) resources: &'state mut ResourceTracker,
}

impl OpacityCompositor<'_> {
    /// Find the matching end marker for a stacking context.
    pub(crate) fn find_stacking_context_end(items: &[DisplayItem], start: usize) -> usize {
        let mut depth = 1usize;
        for (index, item) in items.iter().enumerate().skip(start) {
            match item {
                DisplayItem::BeginStackingContext { .. } => depth += 1,
                DisplayItem::EndStackingContext => {
                    depth -= 1;
                    if depth == 0 {
                        return index;
                    }
                }
                _ => {}
            }
        }
        items.len()
    }

    /// Compute the bounding box of a set of display items.
    pub(crate) fn compute_items_bounds(items: &[DisplayItem]) -> Option<Bounds> {
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;

        for item in items {
            match item {
                DisplayItem::Rect {
                    x,
                    y,
                    width,
                    height,
                    ..
                } => {
                    min_x = min_x.min(*x);
                    min_y = min_y.min(*y);
                    max_x = max_x.max(x + width);
                    max_y = max_y.max(y + height);
                }
                DisplayItem::Text { x, y, bounds, .. } => {
                    if let Some((left, top, right, bottom)) = bounds {
                        min_x = min_x.min(*left as f32);
                        min_y = min_y.min(*top as f32);
                        max_x = max_x.max(*right as f32);
                        max_y = max_y.max(*bottom as f32);
                    } else {
                        min_x = min_x.min(*x);
                        min_y = min_y.min(*y);
                        max_x = max_x.max(x + 100.0);
                        max_y = max_y.max(y + 20.0);
                    }
                }
                _ => {}
            }
        }

        (min_x.is_finite() && max_x.is_finite() && min_y.is_finite() && max_y.is_finite()).then(
            || {
                let width = (max_x - min_x).max(1.0);
                let height = (max_y - min_y).max(1.0);
                (min_x, min_y, width, height)
            },
        )
    }

    /// Collect opacity composites from display items for offscreen rendering.
    pub(crate) fn collect_opacity_composites(
        &mut self,
        encoder: &mut CommandEncoder,
        items: &[DisplayItem],
    ) -> Vec<OpacityComposite> {
        let mut out: Vec<OpacityComposite> = Vec::new();
        let mut index = 0usize;
        while index < items.len() {
            if let DisplayItem::BeginStackingContext { boundary } = &items[index]
                && matches!(boundary, StackingContextBoundary::Opacity { alpha } if *alpha < 1.0)
            {
                let end = Self::find_stacking_context_end(items, index + 1);
                let group_items = &items[index + 1..end];
                let alpha = match boundary {
                    StackingContextBoundary::Opacity { alpha } => *alpha,
                    _ => 1.0,
                };
                let bounds =
                    Self::compute_items_bounds(group_items).unwrap_or((0.0, 0.0, 1.0, 1.0));
                let (tex, view, tex_width, tex_height, bind_group) = self
                    .render_items_to_offscreen_bounded_with_bind_group(
                        encoder,
                        group_items,
                        bounds,
                        alpha,
                    );
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

    /// Extract and render opacity layers for two-phase compositing.
    ///
    /// # Errors
    /// Returns an error if opacity layer rendering or submission fails.
    pub(crate) fn extract_and_render_opacity_layers(
        &mut self,
        _main_encoder: &mut CommandEncoder,
        items: &[DisplayItem],
    ) -> AnyResult<OpacityExtraction> {
        let mut layers = Vec::new();
        let mut clean_items = Vec::new();
        let mut index = 0usize;

        debug!(target: "wgpu_renderer", "=== PHASE 1: Extracting and pre-rendering opacity layers ===");
        debug!(target: "wgpu_renderer", "    Using dedicated encoder per opacity group with immediate submission");

        while index < items.len() {
            match &items[index] {
                DisplayItem::BeginStackingContext { boundary } => {
                    if let StackingContextBoundary::Opacity { alpha } = boundary
                        && *alpha < 1.0
                    {
                        let (layer, next_index) =
                            self.process_opacity_group(items, index, *alpha)?;

                        let bounds = layer.bounds;
                        clean_items.push(DisplayItem::Rect {
                            x: bounds.0,
                            y: bounds.1,
                            width: 0.0,
                            height: 0.0,
                            color: [0.0, 0.0, 0.0, 0.0],
                        });

                        layers.push(layer);
                        index = next_index;
                        continue;
                    }

                    index += 1;
                }
                DisplayItem::EndStackingContext => {
                    index += 1;
                }
                other_item => {
                    clean_items.push(other_item.clone());
                    index += 1;
                }
            }
        }

        debug!(target: "wgpu_renderer", "=== PHASE 1 COMPLETE: {} layers extracted ===", layers.len());
        Ok(OpacityExtraction {
            layers,
            clean_items,
        })
    }

    /// Process a single opacity group: render it offscreen and return the layer data.
    ///
    /// # Errors
    /// Returns an error if offscreen rendering or submission fails.
    fn process_opacity_group(
        &mut self,
        items: &[DisplayItem],
        index: usize,
        alpha: f32,
    ) -> AnyResult<(OpacityLayer, usize)> {
        let start_index = index;
        let end = Self::find_stacking_context_end(items, index + 1);
        let group_items_raw = &items[index + 1..end];

        let group_items_clean = Self::remove_stacking_markers(group_items_raw);

        debug!(target: "wgpu_renderer", "  Found opacity group: index={}, alpha={}, raw_items={}, clean_items={}",
               start_index, alpha, group_items_raw.len(), group_items_clean.len());

        let bounds = Self::compute_items_bounds(&group_items_clean).unwrap_or((0.0, 0.0, 1.0, 1.0));

        let mut offscreen_encoder =
            self.gpu
                .device()
                .create_command_encoder(&CommandEncoderDescriptor {
                    label: Some("opacity-group-encoder"),
                });

        let (texture, _view, _tex_width, _tex_height, bind_group) = self
            .render_items_to_offscreen_bounded_with_bind_group(
                &mut offscreen_encoder,
                &group_items_clean,
                bounds,
                alpha,
            );

        let offscreen_cmd_buf = offscreen_encoder.finish();
        submit_with_validation(self.gpu.device(), self.gpu.queue(), [offscreen_cmd_buf])?;

        Ok((
            OpacityLayer {
                bounds,
                texture,
                bind_group,
            },
            end + 1,
        ))
    }

    /// Remove all stacking context markers from a list of display items.
    fn remove_stacking_markers(items: &[DisplayItem]) -> Vec<DisplayItem> {
        items
            .iter()
            .filter(|item| {
                !matches!(
                    item,
                    DisplayItem::BeginStackingContext { .. } | DisplayItem::EndStackingContext
                )
            })
            .cloned()
            .collect()
    }

    /// Render items to offscreen texture with bind group for opacity compositing.
    fn render_items_to_offscreen_bounded_with_bind_group(
        &mut self,
        encoder: &mut CommandEncoder,
        items: &[DisplayItem],
        bounds: (f32, f32, f32, f32),
        alpha: f32,
    ) -> (Texture, TextureView, u32, u32, BindGroup) {
        let mut offscreen_renderer = OffscreenRenderer {
            gpu: self.gpu,
            pipelines: self.pipelines,
            text: self.text,
            resources: self.resources,
        };

        offscreen_renderer
            .render_items_to_offscreen_bounded_with_bind_group(encoder, items, bounds, alpha)
    }

    /// Draw a textured quad using a pre-created bind group.
    pub(crate) fn draw_texture_quad_with_bind_group(
        &mut self,
        pass: &mut RenderPass<'_>,
        bind_group: &BindGroup,
        bounds: Bounds,
    ) {
        let (rect_x, rect_y, rect_width, rect_height) = bounds;
        debug!(target: "wgpu_renderer", ">>> draw_texture_quad_with_bind_group: bounds=({rect_x}, {rect_y}, {rect_width}, {rect_height})");

        let framebuffer_width = self.gpu.size().width.max(1) as f32;
        let framebuffer_height = self.gpu.size().height.max(1) as f32;
        let x0 = (rect_x / framebuffer_width).mul_add(2.0, -1.0);
        let x1 = ((rect_x + rect_width) / framebuffer_width).mul_add(2.0, -1.0);
        let y0 = (rect_y / framebuffer_height).mul_add(-2.0, 1.0);
        let y1 = ((rect_y + rect_height) / framebuffer_height).mul_add(-2.0, 1.0);
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
        let vertex_buffer = self
            .gpu
            .device()
            .create_buffer_init(&util::BufferInitDescriptor {
                label: Some("opacity-quad-vertices"),
                contents: cast_slice(&verts),
                usage: BufferUsages::VERTEX,
            });
        self.resources.track_buffer(vertex_buffer.clone());

        pass.set_pipeline(self.pipelines.texture_pipeline());

        let scissor_x = rect_x.max(0.0).floor() as u32;
        let scissor_y = rect_y.max(0.0).floor() as u32;
        let scissor_width = rect_width.max(0.0).ceil() as u32;
        let scissor_height = rect_height.max(0.0).ceil() as u32;

        let clipped_x = scissor_x.min(self.gpu.size().width);
        let clipped_y = scissor_y.min(self.gpu.size().height);
        let clipped_width = scissor_width.min(self.gpu.size().width.saturating_sub(clipped_x));
        let clipped_height = scissor_height.min(self.gpu.size().height.saturating_sub(clipped_y));

        if clipped_width == 0 || clipped_height == 0 {
            debug!(target: "wgpu_renderer", ">>> Skipping draw: scissor rect is empty after clipping");
            return;
        }

        pass.set_scissor_rect(clipped_x, clipped_y, clipped_width, clipped_height);
        pass.set_bind_group(0, bind_group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..6, 0..1);
        debug!(target: "wgpu_renderer", ">>> Textured quad drawn");
    }
}
