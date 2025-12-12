//! Offscreen rendering functionality.

mod initialization;
mod readback;
mod rendering;
mod text_preparation;

use anyhow::Result as AnyhowResult;
use glyphon::Cache as GlyphonCache;
use renderer::display_list::DisplayList;
use wgpu::*;

use initialization::{
    GlyphonInitParams, OffscreenGpuContext, create_render_texture, initialize_glyphon,
    initialize_gpu,
};
use readback::{ReadbackParams, readback_texture};
use rendering::{RenderRectsParams, build_pipeline, render_rectangles_pass, render_text_pass};
use text_preparation::{PrepareTextParams, prepare_text_items};

/// Render a display list to an RGBA buffer using offscreen rendering.
///
/// # Errors
/// Returns an error if GPU initialization, rendering, or buffer readback fails.
pub fn render_display_list_to_rgba(
    display_list: &DisplayList,
    width: u32,
    height: u32,
) -> AnyhowResult<Vec<u8>> {
    let OffscreenGpuContext { device, queue } = initialize_gpu()?;
    let render_format = TextureFormat::Rgba8UnormSrgb;
    let (texture, texture_view) = create_render_texture(&device, width, height, render_format);
    let pipeline = build_pipeline(&device, render_format);
    let glyphon_cache = GlyphonCache::new(&device);
    let mut glyphon_state = initialize_glyphon(&GlyphonInitParams {
        device: &device,
        queue: &queue,
        glyphon_cache: &glyphon_cache,
        render_format,
        width,
        height,
    });
    let (_texts, _buffers) = prepare_text_items(&mut PrepareTextParams {
        display_list,
        glyphon_state: &mut glyphon_state,
        device: &device,
        queue: &queue,
        width,
        height,
    })?;
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
    render_rectangles_pass(&mut RenderRectsParams {
        encoder: &mut encoder,
        texture_view: &texture_view,
        pipeline: &pipeline,
        display_list,
        device: &device,
        width,
        height,
    });
    render_text_pass(&mut encoder, &texture_view, &glyphon_state, width, height)?;
    readback_texture(ReadbackParams {
        encoder,
        texture: &texture,
        device: &device,
        queue: &queue,
        width,
        height,
    })
}
