//! Offscreen rendering functionality for opacity compositing.

use super::batching::ItemBatcher;
use super::texture::TextureManager;
use crate::state::error_scope::ErrorScopeGuard;
use crate::state::gpu_context::GpuContext;
use crate::state::pipeline_cache::PipelineCache;
use crate::state::resource_tracker::ResourceTracker;
use crate::state::text_renderer_state::TextRendererState;
use crate::state::{OffscreenRenderParams, RenderContext, ScissorRect};
use crate::text::map_text_item;
use log::{debug, error};
use renderer::display_list::DisplayItem;
use renderer::renderer::DrawText;
use wgpu::*;
use winit::dpi::PhysicalSize;

/// Helper struct for offscreen rendering operations.
pub(super) struct OffscreenRenderer<'state> {
    pub(super) gpu: &'state GpuContext,
    pub(super) pipelines: &'state PipelineCache,
    pub(super) text: &'state mut TextRendererState,
    pub(super) resources: &'state mut ResourceTracker,
}

impl OffscreenRenderer<'_> {
    /// Render items to offscreen texture with bind group for opacity compositing.
    pub(super) fn render_items_to_offscreen_bounded_with_bind_group(
        &mut self,
        encoder: &mut CommandEncoder,
        items: &[DisplayItem],
        bounds: (f32, f32, f32, f32),
        alpha: f32,
    ) -> (Texture, TextureView, u32, u32, BindGroup) {
        let (x, y, width, height) = bounds;
        let tex_width = (width.ceil() as u32).max(1);
        let tex_height = (height.ceil() as u32).max(1);

        debug!(target: "wgpu_renderer", "render_items_to_offscreen_bounded: bounds=({}, {}, {}, {}), tex_size={}x{}, items={}",
            x, y, width, height, tex_width, tex_height, items.len());

        let texture =
            TextureManager::create_offscreen_texture_static(self.gpu, tex_width, tex_height);
        let view = texture.create_view(&TextureViewDescriptor {
            label: Some("offscreen-opacity-view"),
            format: Some(TextureFormat::Bgra8Unorm),
            ..Default::default()
        });

        let ctx = RenderContext::new(PhysicalSize::new(tex_width, tex_height));
        let translated_items = Self::translate_items_to_local(items, x, y);

        let text_items: Vec<DrawText> = translated_items.iter().filter_map(map_text_item).collect();
        if !text_items.is_empty() {
            debug!(target: "wgpu_renderer", "Pre-preparing glyphon for {} text items before encoder operations", text_items.len());
            self.glyphon_prepare_for(&text_items);
        }

        self.render_offscreen_rects_pass(&mut OffscreenRenderParams {
            encoder,
            view: &view,
            translated_items: &translated_items,
            tex_width,
            tex_height,
            ctx,
        });
        self.render_offscreen_text_pass(&mut OffscreenRenderParams {
            encoder,
            view: &view,
            translated_items: &translated_items,
            tex_width,
            tex_height,
            ctx,
        });

        debug!(target: "wgpu_renderer", "Offscreen render passes complete, creating bind group");
        let bind_group = TextureManager::create_opacity_bind_group_static(
            self.gpu,
            self.pipelines,
            self.resources,
            &view,
            alpha,
        );
        debug!(target: "wgpu_renderer", "Bind group created, texture ready for compositing");

        (texture, view, tex_width, tex_height, bind_group)
    }

    /// Translate display items to texture-local coordinates.
    fn translate_items_to_local(
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
                    font_weight,
                    matched_font_weight,
                    font_family,
                    line_height,
                    line_height_unrounded,
                    bounds: text_bounds,
                    measured_width,
                } => DisplayItem::Text {
                    x: text_x - offset_x,
                    y: text_y - offset_y,
                    text: text.clone(),
                    color: *color,
                    font_size: *font_size,
                    font_weight: *font_weight,
                    matched_font_weight: *matched_font_weight,
                    font_family: font_family.clone(),
                    line_height: *line_height,
                    line_height_unrounded: *line_height_unrounded,
                    bounds: text_bounds.map(|(left, top, right, bottom)| {
                        (
                            (left as f32 - offset_x) as i32,
                            (top as f32 - offset_y) as i32,
                            (right as f32 - offset_x) as i32,
                            (bottom as f32 - offset_y) as i32,
                        )
                    }),
                    measured_width: *measured_width,
                },
                other => other.clone(),
            })
            .collect()
    }

    /// Render rectangles to offscreen texture.
    fn render_offscreen_rects_pass(&mut self, params: &mut OffscreenRenderParams<'_>) {
        debug!(target: "wgpu_renderer", ">>> CREATING offscreen rects pass");
        {
            let mut pass = params.encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("opacity-offscreen-pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: params.view,
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
                params.tex_width as f32,
                params.tex_height as f32,
                0.0,
                1.0,
            );
            pass.set_pipeline(self.pipelines.offscreen_pipeline());
            debug!(target: "wgpu_renderer", "    Drawing items (batched, no nested stacking contexts)");

            let mut batcher = ItemBatcher {
                gpu: self.gpu,
                resources: self.resources,
            };
            batcher.draw_items_batched_with_size(
                &mut pass,
                params.translated_items,
                params.tex_width,
                params.tex_height,
            );
            debug!(target: "wgpu_renderer", "    Pass ending...");
        };
    }

    /// Render text to offscreen texture.
    fn render_offscreen_text_pass(&self, params: &mut OffscreenRenderParams<'_>) {
        let text_items: Vec<DrawText> = params
            .translated_items
            .iter()
            .filter_map(map_text_item)
            .collect();
        if text_items.is_empty() {
            return;
        }

        debug!(target: "wgpu_renderer", ">>> CREATING offscreen text pass");
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
        self.draw_text_batch_ctx(&mut text_pass, text_items.as_slice(), None, params.ctx);
    }

    /// Context-aware version of `draw_text_batch`.
    fn draw_text_batch_ctx(
        &self,
        pass: &mut RenderPass<'_>,
        _text_items: &[DrawText],
        scissor: Option<ScissorRect>,
        ctx: RenderContext,
    ) {
        pass.set_viewport(0.0, 0.0, ctx.width() as f32, ctx.height() as f32, 0.0, 1.0);
        match scissor {
            Some((x, y, width, height)) => pass.set_scissor_rect(x, y, width, height),
            None => pass.set_scissor_rect(0, 0, ctx.width().max(1), ctx.height().max(1)),
        }
        {
            let scope = ErrorScopeGuard::push(self.gpu.device(), "glyphon-text-render");
            if let Err(error) = self.text.render(self.gpu.device(), pass) {
                error!(target: "wgpu_renderer", "Glyphon text_renderer.render() failed: {error:?}");
            }
            if let Err(error) = scope.check() {
                error!(target: "wgpu_renderer", "Glyphon text_renderer.render() generated validation error: {error:?}");
            }
        }
    }

    /// Prepare glyphon buffers for a specific set of text items.
    fn glyphon_prepare_for(&mut self, items: &[DrawText]) {
        let scale = self.gpu.window().scale_factor() as f32;
        self.text.prepare(
            self.gpu.device(),
            self.gpu.queue(),
            items,
            (self.gpu.size(), scale),
        );
    }
}
