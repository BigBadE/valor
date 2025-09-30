use crate::display_list::{DisplayItem, DisplayList, Scissor, StackingContextBoundary};
use crate::renderer::DrawText;
use glyphon::{
    Attrs, Buffer as GlyphonBuffer, Color as GlyphonColor, Metrics, Resolution, Shaping, TextArea,
    TextBounds,
};
use log::debug;

pub(crate) type TextBatch = (Option<Scissor>, Vec<DrawText>);

#[inline]
pub(crate) fn batch_texts_with_scissor(
    dl: &DisplayList,
    framebuffer_w: u32,
    framebuffer_h: u32,
) -> Vec<TextBatch> {
    let mut out: Vec<TextBatch> = Vec::new();
    let mut stack: Vec<Scissor> = Vec::new();
    let mut current_scissor: Option<Scissor> = None;
    let mut current_texts: Vec<DrawText> = Vec::new();
    let mut sc_stack_is_opacity: Vec<bool> = Vec::new();
    let mut opacity_depth: usize = 0;
    for item in &dl.items {
        match item {
            DisplayItem::BeginClip {
                x,
                y,
                width,
                height,
            } => {
                if !current_texts.is_empty() {
                    out.push((current_scissor, std::mem::take(&mut current_texts)));
                }
                let new_sc =
                    rect_to_scissor((framebuffer_w, framebuffer_h), *x, *y, *width, *height);
                let effective = match current_scissor {
                    Some(sc) => intersect_scissor(sc, new_sc),
                    None => new_sc,
                };
                stack.push(new_sc);
                current_scissor = Some(effective);
            }
            DisplayItem::EndClip => {
                if !current_texts.is_empty() {
                    out.push((current_scissor, std::mem::take(&mut current_texts)));
                }
                let _ = stack.pop();
                current_scissor = stack.iter().cloned().reduce(intersect_scissor);
            }
            DisplayItem::BeginStackingContext { boundary } => {
                let is_opacity =
                    matches!(boundary, StackingContextBoundary::Opacity { alpha } if *alpha < 1.0);
                sc_stack_is_opacity.push(is_opacity);
                if is_opacity {
                    opacity_depth += 1;
                }
            }
            DisplayItem::EndStackingContext => {
                if sc_stack_is_opacity.pop().unwrap_or(false) && opacity_depth > 0 {
                    opacity_depth -= 1;
                }
            }
            DisplayItem::Text {
                x,
                y,
                text,
                color,
                font_size,
                bounds,
            } => {
                if opacity_depth == 0 {
                    current_texts.push(DrawText {
                        x: *x,
                        y: *y,
                        text: text.clone(),
                        color: *color,
                        font_size: *font_size,
                        bounds: *bounds,
                    });
                }
            }
            _ => {}
        }
    }

    if !current_texts.is_empty() {
        out.push((current_scissor, current_texts));
    }
    out
}

#[inline]
pub(crate) fn batch_layer_texts_with_scissor(
    layers: &[Layer],
    framebuffer_w: u32,
    framebuffer_h: u32,
) -> Vec<TextBatch> {
    let mut out: Vec<TextBatch> = Vec::new();
    for layer in layers {
        let dl = match layer {
            Layer::Content(dl) | Layer::Chrome(dl) => dl,
            Layer::Background => continue,
        };
        let mut stack: Vec<Scissor> = Vec::new();
        let mut current_scissor: Option<Scissor> = None;
        let mut current_texts: Vec<DrawText> = Vec::new();
        let mut sc_stack_is_opacity: Vec<bool> = Vec::new();
        let mut opacity_depth: usize = 0;
        for item in &dl.items {
            match item {
                DisplayItem::BeginClip {
                    x,
                    y,
                    width,
                    height,
                } => {
                    if !current_texts.is_empty() {
                        out.push((current_scissor, std::mem::take(&mut current_texts)));
                    }
                    let new_sc =
                        rect_to_scissor((framebuffer_w, framebuffer_h), *x, *y, *width, *height);
                    let effective = match current_scissor {
                        Some(sc) => intersect_scissor(sc, new_sc),
                        None => new_sc,
                    };
                    stack.push(new_sc);
                    current_scissor = Some(effective);
                }
                DisplayItem::EndClip => {
                    if !current_texts.is_empty() {
                        out.push((current_scissor, std::mem::take(&mut current_texts)));
                    }
                    let _ = stack.pop();
                    current_scissor = stack.iter().cloned().reduce(intersect_scissor);
                }
                DisplayItem::BeginStackingContext { boundary } => {
                    let is_opacity = matches!(boundary, StackingContextBoundary::Opacity { alpha } if *alpha < 1.0);
                    sc_stack_is_opacity.push(is_opacity);
                    if is_opacity {
                        opacity_depth += 1;
                    }
                }
                DisplayItem::EndStackingContext => {
                    if sc_stack_is_opacity.pop().unwrap_or(false) && opacity_depth > 0 {
                        opacity_depth -= 1;
                    }
                }
                DisplayItem::Text {
                    x,
                    y,
                    text,
                    color,
                    font_size,
                    bounds,
                } => {
                    if opacity_depth == 0 {
                        current_texts.push(DrawText {
                            x: *x,
                            y: *y,
                            text: text.clone(),
                            color: *color,
                            font_size: *font_size,
                            bounds: *bounds,
                        });
                    }
                }
                _ => {}
            }
        }
        if !current_texts.is_empty() {
            out.push((current_scissor, current_texts));
        }
    }
    out
}

#[inline]
pub(crate) fn map_text_item(item: &DisplayItem) -> Option<DrawText> {
    if let DisplayItem::Text {
        x,
        y,
        text,
        color,
        font_size,
        bounds,
    } = item
    {
        return Some(DrawText {
            x: *x,
            y: *y,
            text: text.clone(),
            color: *color,
            font_size: *font_size,
            bounds: *bounds,
        });
    }
    None
}

#[inline]
fn rect_to_scissor(framebuffer: (u32, u32), x: f32, y: f32, w: f32, h: f32) -> Scissor {
    let framebuffer_w = framebuffer.0.max(1);
    let framebuffer_h = framebuffer.1.max(1);
    let mut sx = x.max(0.0).floor() as i32;
    let mut sy = y.max(0.0).floor() as i32;
    let mut sw = w.max(0.0).ceil() as i32;
    let mut sh = h.max(0.0).ceil() as i32;
    if sx < 0 {
        sw += sx;
        sx = 0;
    }
    if sy < 0 {
        sh += sy;
        sy = 0;
    }
    let max_w = framebuffer_w as i32 - sx;
    let max_h = framebuffer_h as i32 - sy;
    let sw = sw.clamp(0, max_w) as u32;
    let sh = sh.clamp(0, max_h) as u32;
    (sx as u32, sy as u32, sw, sh)
}

#[inline]
fn intersect_scissor(a: Scissor, b: Scissor) -> Scissor {
    let (ax, ay, aw, ah) = a;
    let (bx, by, bw, bh) = b;
    let x0 = ax.max(bx);
    let y0 = ay.max(by);
    let x1 = (ax + aw).min(bx + bw);
    let y1 = (ay + ah).min(by + bh);
    let w = x1.saturating_sub(x0);
    let h = y1.saturating_sub(y0);
    (x0, y0, w, h)
}

use super::state::{Layer, RenderState};

impl RenderState {
    /// Prepare glyphon buffers for the current text list and upload glyphs into the atlas.
    pub(crate) fn glyphon_prepare(&mut self) {
        let framebuffer_width = self.size.width;
        let framebuffer_height = self.size.height;
        let scale: f32 = self.window.scale_factor() as f32;
        // Build buffers first
        let mut buffers: Vec<GlyphonBuffer> = Vec::with_capacity(self.text_list.len());
        for item in &self.text_list {
            let mut buffer = GlyphonBuffer::new(
                &mut self.font_system,
                Metrics::new(item.font_size * scale, item.font_size * scale),
            );
            let attrs = Attrs::new();
            buffer.set_text(&mut self.font_system, &item.text, &attrs, Shaping::Advanced);
            buffers.push(buffer);
        }
        // Build areas referencing buffers
        let mut areas: Vec<TextArea> = Vec::with_capacity(self.text_list.len());
        for (index, item) in self.text_list.iter().enumerate() {
            // Visible on white: use opaque black (ARGB alpha-highest)
            let color = GlyphonColor(0xFF00_0000);
            let bounds = match item.bounds {
                Some((l, t, r, b)) => TextBounds {
                    left: (l as f32 * scale).round() as i32,
                    top: (t as f32 * scale).round() as i32,
                    right: (r as f32 * scale).round() as i32,
                    bottom: (b as f32 * scale).round() as i32,
                },
                None => TextBounds {
                    left: 0,
                    top: 0,
                    right: framebuffer_width as i32,
                    bottom: framebuffer_height as i32,
                },
            };
            let buffer_ref = &buffers[index];
            areas.push(TextArea {
                buffer: buffer_ref,
                left: item.x * scale,
                top: item.y * scale,
                scale: 1.0,
                bounds,
                default_color: color,
                custom_glyphs: &[],
            });
        }
        // Prepare text (atlas upload + layout)
        self.viewport.update(
            &self.queue,
            Resolution {
                width: framebuffer_width,
                height: framebuffer_height,
            },
        );
        let areas_count = areas.len();
        let prep_res = self.text_renderer.prepare(
            &self.device,
            &self.queue,
            &mut self.font_system,
            &mut self.text_atlas,
            &self.viewport,
            areas,
            &mut self.swash_cache,
        );
        debug!(
            target: "wgpu_renderer",
            "glyphon_prepare: areas={areas_count} viewport={framebuffer_width}x{framebuffer_height} result={prep_res:?}"
        );
        debug!(
            target: "wgpu_renderer",
            "glyphon_prepare: text_items={} ",
            self.text_list.len()
        );
    }

    pub(crate) fn glyphon_prepare_for(&mut self, items: &[DrawText]) {
        let framebuffer_width = self.size.width;
        let framebuffer_height = self.size.height;
        let scale: f32 = self.window.scale_factor() as f32;
        let mut buffers: Vec<GlyphonBuffer> = Vec::with_capacity(items.len());
        for item in items.iter() {
            let mut buffer = GlyphonBuffer::new(
                &mut self.font_system,
                Metrics::new(item.font_size * scale, item.font_size * scale),
            );
            let attrs = Attrs::new();
            buffer.set_text(&mut self.font_system, &item.text, &attrs, Shaping::Advanced);
            buffers.push(buffer);
        }
        let mut areas: Vec<TextArea> = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            let color = GlyphonColor(0xFF00_0000);
            let bounds = match item.bounds {
                Some((l, t, r, b)) => TextBounds {
                    left: (l as f32 * scale).round() as i32,
                    top: (t as f32 * scale).round() as i32,
                    right: (r as f32 * scale).round() as i32,
                    bottom: (b as f32 * scale).round() as i32,
                },
                None => TextBounds {
                    left: 0,
                    top: 0,
                    right: framebuffer_width as i32,
                    bottom: framebuffer_height as i32,
                },
            };
            let buffer_ref = &buffers[index];
            areas.push(TextArea {
                buffer: buffer_ref,
                left: item.x * scale,
                top: item.y * scale,
                scale: 1.0,
                bounds,
                default_color: color,
                custom_glyphs: &[],
            });
        }
        self.viewport.update(
            &self.queue,
            Resolution {
                width: framebuffer_width,
                height: framebuffer_height,
            },
        );
        let areas_len = areas.len();
        let prep_res = self.text_renderer.prepare(
            &self.device,
            &self.queue,
            &mut self.font_system,
            &mut self.text_atlas,
            &self.viewport,
            areas,
            &mut self.swash_cache,
        );
        debug!(
            target: "wgpu_renderer",
            "glyphon_prepare_for: items={} areas={} viewport={}x{} result={:?}",
            items.len(),
            areas_len,
            framebuffer_width,
            framebuffer_height,
            prep_res
        );
    }

    #[inline]
    pub(crate) fn draw_text_batch(
        &mut self,
        pass: &mut wgpu::RenderPass<'_>,
        items: &[DrawText],
        scissor_opt: Option<Scissor>,
    ) {
        self.glyphon_prepare_for(items);
        pass.set_viewport(
            0.0,
            0.0,
            self.size.width as f32,
            self.size.height as f32,
            0.0,
            1.0,
        );
        match scissor_opt {
            Some((x, y, w, h)) => pass.set_scissor_rect(x, y, w, h),
            None => pass.set_scissor_rect(0, 0, self.size.width.max(1), self.size.height.max(1)),
        }
        let _ = self
            .text_renderer
            .render(&self.text_atlas, &self.viewport, pass);
    }

    #[inline]
    pub(crate) fn draw_text_batches(
        &mut self,
        pass: &mut wgpu::RenderPass<'_>,
        batches: Vec<TextBatch>,
    ) {
        for (scissor_opt, items) in batches.into_iter().filter(|(_, it)| !it.is_empty()) {
            self.draw_text_batch(pass, &items, scissor_opt);
        }
    }
}
