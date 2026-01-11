//! Core rendering passes for `RenderState`.

use super::error_scope::ErrorScopeGuard;
use super::render_orchestrator;
use super::{RenderRectanglesParams, RenderState, RenderTextParams};
use crate::error::submit_with_validation;
use crate::pipelines::Vertex;
use crate::text::batch_layer_texts_with_scissor;
use crate::text::batch_texts_with_scissor;
use anyhow::Error as AnyhowError;
use anyhow::Result as AnyResult;
use bytemuck::cast_slice;
use log::{debug, error};
use tracing::info_span;
use wgpu::util::DeviceExt as _;
use wgpu::*;

impl RenderState {
    /// Render a frame by clearing and drawing quads from the current display list.
    ///
    /// # Errors
    /// Returns an error if surface acquisition or rendering fails.
    pub fn render(&mut self) -> Result<(), AnyhowError> {
        let _span = info_span!("renderer.render").entered();
        self.resources.clear();

        let surface_texture = self.gpu.get_current_texture()?;
        let texture_view = surface_texture.texture.create_view(&TextureViewDescriptor {
            format: Some(self.gpu.render_format()),
            ..Default::default()
        });

        let mut encoder = self
            .gpu
            .device()
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("onscreen-frame"),
            });
        self.record_draw_passes(&texture_view, &mut encoder, false)?;

        let command_buffer = encoder.finish();
        submit_with_validation(self.gpu.device(), self.gpu.queue(), [command_buffer])?;

        self.resources.clear();
        self.gpu.window().pre_present_notify();
        surface_texture.present();
        Ok(())
    }

    /// Render the current display list to an RGBA buffer.
    ///
    /// # Errors
    /// Returns an error if rendering or texture readback fails.
    ///
    /// # Panics
    /// Panics if pixel chunks are not exactly 4 bytes.
    pub fn render_to_rgba(&mut self) -> Result<Vec<u8>, AnyhowError> {
        use std::time::Instant;
        let start = Instant::now();

        self.resources.clear();

        let validation_scope = ErrorScopeGuard::push(self.gpu.device(), "pre-render-validation");
        validation_scope.check()?;

        self.offscreen
            .ensure_texture(self.gpu.device(), self.gpu.size(), self.gpu.render_format());
        eprintln!("[GPU_TIMING] render_to_offscreen START");
        self.render_to_offscreen()?;
        eprintln!(
            "[GPU_TIMING] render_to_offscreen took: {:?}",
            start.elapsed()
        );

        let mut copy_encoder =
            self.gpu
                .device()
                .create_command_encoder(&CommandEncoderDescriptor {
                    label: Some("texture-copy-encoder"),
                });

        let width = self.gpu.size().width.max(1);
        let height = self.gpu.size().height.max(1);
        let bpp = 4u32;
        let row_bytes = width * bpp;
        let padded_bpr = row_bytes.div_ceil(256) * 256;
        let buffer_size = u64::from(padded_bpr) * u64::from(height);

        self.offscreen
            .ensure_readback_buffer(self.gpu.device(), padded_bpr, buffer_size);
        self.offscreen
            .copy_to_readback(&mut copy_encoder, width, height, padded_bpr)?;

        let copy_command_buffer = {
            let scope = ErrorScopeGuard::push(self.gpu.device(), "copy_encoder.finish");
            let buffer = copy_encoder.finish();
            scope.check()?;
            buffer
        };
        eprintln!("[GPU_TIMING] submit_with_validation START");
        submit_with_validation(self.gpu.device(), self.gpu.queue(), [copy_command_buffer])?;
        eprintln!("[GPU_TIMING] submit_with_validation took: {:?}", start.elapsed());

        eprintln!("[GPU_TIMING] read_pixels START");
        let mut out = self
            .offscreen
            .read_pixels(self.gpu.device(), (width, height, bpp, padded_bpr))?;
        eprintln!("[GPU_TIMING] read_pixels took: {:?}", start.elapsed());

        eprintln!("[GPU_TIMING] convert_bgra_to_rgba START");
        self.convert_bgra_to_rgba(&mut out);
        eprintln!("[GPU_TIMING] convert_bgra_to_rgba took: {:?}", start.elapsed());
        Ok(out)
    }

    /// Record all draw passes (rectangles and text) for the current frame.
    ///
    /// # Errors
    /// Returns an error if rendering fails.
    pub(super) fn record_draw_passes(
        &mut self,
        texture_view: &TextureView,
        encoder: &mut CommandEncoder,
        use_retained: bool,
    ) -> AnyResult<()> {
        let use_layers = !self.layers.is_empty();
        let is_offscreen = false;
        let main_load = LoadOp::Load;
        let text_load = LoadOp::Load;

        self.prepare_text_for_rendering(use_retained, use_layers);
        self.render_rectangles_pass(&mut RenderRectanglesParams {
            encoder,
            texture_view,
            use_retained,
            use_layers,
            is_offscreen,
            main_load,
        })?;

        let text_scope = ErrorScopeGuard::push(self.gpu.device(), "render-text-pass");
        self.render_text_pass(&mut RenderTextParams {
            encoder,
            texture_view,
            text_load,
            use_retained,
            use_layers,
        });
        if let Err(err) = text_scope.check() {
            error!(target: "wgpu_renderer", "render_text_pass error scope caught error: {err:?}");
            return Err(err);
        }

        Ok(())
    }

    /// Render rectangles pass with support for layers, retained, and immediate modes.
    ///
    /// # Errors
    /// Returns an error if rendering fails.
    pub(super) fn render_rectangles_pass(
        &mut self,
        params: &mut RenderRectanglesParams<'_>,
    ) -> AnyResult<()> {
        if !params.is_offscreen {
            self.render_clear_pass(params.encoder, params.texture_view);
        }

        if params.use_layers {
            let mut components = render_orchestrator::RenderComponents {
                gpu: &self.gpu,
                pipelines: &self.pipelines,
                text: &mut self.text,
                rectangles: &mut self.rectangles,
                offscreen: &self.offscreen,
                resources: &mut self.resources,
            };
            render_orchestrator::render_layers_rectangles(
                params.encoder,
                params.texture_view,
                params.main_load,
                &self.layers,
                &mut components,
            );
        } else if params.use_retained {
            let mut components = render_orchestrator::RenderComponents {
                gpu: &self.gpu,
                pipelines: &self.pipelines,
                text: &mut self.text,
                rectangles: &mut self.rectangles,
                offscreen: &self.offscreen,
                resources: &mut self.resources,
            };
            render_orchestrator::render_retained_rectangles(
                params.encoder,
                params.texture_view,
                params.main_load,
                self.retained_display_list.as_ref(),
                &mut components,
            )?;
        } else {
            self.render_immediate_rectangles(
                params.encoder,
                params.texture_view,
                params.is_offscreen,
            );
        }
        Ok(())
    }

    /// Render immediate mode rectangles in a single batched draw call.
    pub(super) fn render_immediate_rectangles(
        &mut self,
        encoder: &mut CommandEncoder,
        texture_view: &TextureView,
        is_offscreen: bool,
    ) {
        let mut vertices: Vec<Vertex> = Vec::with_capacity(self.display_list.len() * 6);
        for rect in &self.display_list {
            let rgba = [rect.color[0], rect.color[1], rect.color[2], 1.0];
            self.push_rect_vertices_ndc(
                &mut vertices,
                [rect.x, rect.y, rect.width, rect.height],
                rgba,
            );
        }
        let vertex_bytes = cast_slice(vertices.as_slice());
        let vertex_buffer = self
            .gpu
            .device()
            .create_buffer_init(&util::BufferInitDescriptor {
                label: Some("rect-vertices"),
                contents: vertex_bytes,
                usage: BufferUsages::VERTEX,
            });
        self.rectangles.set_vertex_buffer(vertex_buffer);
        self.rectangles.set_vertex_count(vertices.len() as u32);
        let immediate_load = if is_offscreen {
            LoadOp::Clear(Color::TRANSPARENT)
        } else {
            LoadOp::Load
        };
        self.render_immediate_pass(encoder, texture_view, immediate_load);
    }

    /// Render text pass for layer or retained mode.
    pub(super) fn render_text_pass(&self, params: &mut RenderTextParams<'_>) {
        let mut pass = params.encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("text-pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: params.texture_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: params.text_load,
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        debug!(target: "wgpu_renderer", "start text-pass");
        pass.push_debug_group("text-pass");
        if params.use_layers {
            let batches = batch_layer_texts_with_scissor(
                &self.layers,
                self.gpu.size().width,
                self.gpu.size().height,
            );
            render_orchestrator::draw_text_batches(&mut pass, batches, &self.gpu, &self.text);
        } else if params.use_retained
            && let Some(display_list) = &self.retained_display_list
        {
            let batches = batch_texts_with_scissor(
                display_list,
                self.gpu.size().width,
                self.gpu.size().height,
            );
            render_orchestrator::draw_text_batches(&mut pass, batches, &self.gpu, &self.text);
        }
        pass.pop_debug_group();
        debug!(target: "wgpu_renderer", "end text-pass");
    }

    /// Render to offscreen texture.
    ///
    /// # Errors
    /// Returns an error if rendering or command submission fails.
    pub(super) fn render_to_offscreen(&mut self) -> Result<(), AnyhowError> {
        let tmp_view = self.offscreen.get_texture(self.gpu.render_format())?;
        let mut encoder = self
            .gpu
            .device()
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("render-to-rgba"),
            });
        self.record_draw_passes(&tmp_view, &mut encoder, true)?;
        let command_buffer = encoder.finish();
        submit_with_validation(self.gpu.device(), self.gpu.queue(), [command_buffer])?;
        self.resources.clear();
        Ok(())
    }

    /// Convert BGRA to RGBA if needed.
    ///
    /// # Panics
    /// Panics if pixel chunks are not exactly 4 bytes.
    pub(super) fn convert_bgra_to_rgba(&self, out: &mut [u8]) {
        match self.gpu.render_format() {
            TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => {
                for pixel in out.chunks_exact_mut(4) {
                    assert!(
                        pixel.len() > 2,
                        "pixel chunks from chunks_exact_mut(4) must have at least 3 elements"
                    );
                    let blue = pixel[0];
                    let red = pixel[2];
                    pixel[0] = red;
                    pixel[2] = blue;
                }
            }
            _ => {}
        }
    }

    /// Render clear pass for non-offscreen rendering.
    pub(super) fn render_clear_pass(
        &self,
        encoder: &mut CommandEncoder,
        texture_view: &TextureView,
    ) {
        debug!(target: "wgpu_renderer", "start clear-pass");
        {
            let _clear_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("clear-pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: texture_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color {
                            r: f64::from(self.clear_color[0]),
                            g: f64::from(self.clear_color[1]),
                            b: f64::from(self.clear_color[2]),
                            a: f64::from(self.clear_color[3]),
                        }),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        debug!(target: "wgpu_renderer", "end clear-pass");
    }

    /// Helper method to render immediate mode pass.
    pub(super) fn render_immediate_pass(
        &self,
        encoder: &mut CommandEncoder,
        texture_view: &TextureView,
        load_op: LoadOp<Color>,
    ) {
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("main-pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: texture_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: load_op,
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            debug!(target: "wgpu_renderer", "start main-pass");
            pass.push_debug_group("main-pass(immediate)");
            pass.set_pipeline(self.pipelines.main_pipeline());
            self.rectangles.render(&mut pass);
            pass.pop_debug_group();
            debug!(target: "wgpu_renderer", "end main-pass");
        };
    }

    /// Push rectangle vertices in NDC coordinates.
    #[inline]
    pub(super) fn push_rect_vertices_ndc(
        &self,
        out: &mut Vec<Vertex>,
        rect_xywh: [f32; 4],
        color: [f32; 4],
    ) {
        let framebuffer_width = self.gpu.size().width.max(1) as f32;
        let framebuffer_height = self.gpu.size().height.max(1) as f32;
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
