//! Surface and layer management for `RenderState`.

use super::{Layer, RenderState};
use crate::text::map_text_item;
use renderer::display_list::DisplayList;
use renderer::renderer::{DrawRect, DrawText};
use wgpu::PollType;
use winit::dpi::PhysicalSize;
use winit::window::Window;

impl RenderState {
    /// Window getter for integrations that require it.
    pub fn get_window(&self) -> &Window {
        self.gpu.window()
    }

    /// Set the framebuffer clear color (canvas background). RGBA in [0,1].
    pub const fn set_clear_color(&mut self, rgba: [f32; 4]) {
        self.clear_color = rgba;
    }

    /// Get glyph bounds from the last prepared text rendering.
    /// Returns per-glyph bounding boxes in screen coordinates.
    #[inline]
    pub fn glyph_bounds(&self) -> &[super::text_renderer_state::GlyphBounds] {
        self.text.glyph_bounds()
    }

    /// Handle window resize and reconfigure the surface.
    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.gpu.resize(new_size);
        self.offscreen.clear();
    }

    /// Clear any compositor layers.
    pub fn clear_layers(&mut self) {
        self.layers.clear();
    }

    /// Reset rendering state for the next frame.
    pub fn reset_for_next_frame(&mut self) {
        let _unused = self.gpu.device().poll(PollType::Wait);
        self.resources.clear();
        self.layers.clear();
        self.text.reset(self.gpu.device());
        self.display_list.clear();
        self.text_list.clear();
    }

    /// Push a new compositor layer to be rendered in order.
    pub fn push_layer(&mut self, layer: Layer) {
        self.layers.push(layer);
    }

    /// Update the current display list to be drawn each frame.
    pub fn set_display_list(&mut self, list: Vec<DrawRect>) {
        self.display_list = list;
    }

    /// Update the current text list to be drawn each frame.
    pub fn set_text_list(&mut self, list: Vec<DrawText>) {
        self.text_list = list;
    }

    /// Install a retained display list as the source of truth for rendering.
    pub fn set_retained_display_list(&mut self, list: DisplayList) {
        self.layers.clear();
        self.retained_display_list = Some(list);
        self.display_list.clear();
        self.text_list.clear();
    }

    /// Prepare glyphon buffers for the current text list.
    pub(super) fn glyphon_prepare(&mut self) {
        let scale = self.gpu.window().scale_factor() as f32;
        self.text.prepare(
            self.gpu.device(),
            self.gpu.queue(),
            &self.text_list,
            (self.gpu.size(), scale),
        );
    }

    /// Prepare text for rendering based on display mode.
    pub(super) fn prepare_text_for_rendering(&mut self, use_retained: bool, use_layers: bool) {
        if use_retained {
            if let Some(display_list) = &self.retained_display_list {
                self.text_list = display_list
                    .items
                    .iter()
                    .filter_map(map_text_item)
                    .collect();
            }
            self.glyphon_prepare();
        } else if !use_layers {
            self.glyphon_prepare();
        }
    }
}
