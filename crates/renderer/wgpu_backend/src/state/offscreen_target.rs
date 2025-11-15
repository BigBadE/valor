//! Offscreen rendering target for WGPU backend.
//!
//! This module contains the `OffscreenTarget` struct which manages offscreen textures
//! and readback buffers for rendering to memory. This is a focused component with a
//! single responsibility: managing offscreen rendering resources.

use anyhow::{Error as AnyhowError, anyhow};
use std::sync::Arc;
use std::sync::mpsc::channel;
use wgpu::*;
use winit::dpi::PhysicalSize;

/// Offscreen rendering target managing textures and readback buffers.
/// This struct has a single responsibility: managing offscreen rendering resources.
pub struct OffscreenTarget {
    /// Persistent offscreen render target.
    offscreen_tex: Option<Texture>,
    /// Persistent readback buffer sized for current framebuffer.
    readback_buf: Option<Buffer>,
    /// Padded bytes per row for readback buffer.
    readback_padded_bpr: u32,
    /// Total size of readback buffer in bytes.
    readback_size: u64,
}

impl OffscreenTarget {
    /// Create a new offscreen target with no initial resources.
    pub const fn new() -> Self {
        Self {
            offscreen_tex: None,
            readback_buf: None,
            readback_padded_bpr: 0,
            readback_size: 0,
        }
    }

    /// Ensure offscreen texture exists and matches the given size.
    pub fn ensure_texture(
        &mut self,
        device: &Arc<Device>,
        size: PhysicalSize<u32>,
        render_format: TextureFormat,
    ) {
        let framebuffer_width = size.width.max(1);
        let framebuffer_height = size.height.max(1);
        let need_offscreen = self.offscreen_tex.as_ref().is_none_or(|tex| {
            let tex_size = tex.size();
            tex_size.width != framebuffer_width
                || tex_size.height != framebuffer_height
                || tex_size.depth_or_array_layers != 1
        });
        if need_offscreen {
            let base_format = match render_format {
                TextureFormat::Rgba8UnormSrgb => TextureFormat::Rgba8Unorm,
                TextureFormat::Bgra8UnormSrgb => TextureFormat::Bgra8Unorm,
                other => other,
            };
            let tex = device.create_texture(&TextureDescriptor {
                label: Some("offscreen-target"),
                size: Extent3d {
                    width: framebuffer_width,
                    height: framebuffer_height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: base_format,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
                view_formats: &[render_format],
            });
            self.offscreen_tex = Some(tex);
        }
    }

    /// Ensure readback buffer exists and is large enough.
    pub fn ensure_readback_buffer(
        &mut self,
        device: &Arc<Device>,
        padded_bpr: u32,
        buffer_size: u64,
    ) {
        let need_readback = self.readback_buf.is_none()
            || self.readback_padded_bpr != padded_bpr
            || self.readback_size < buffer_size;
        if need_readback {
            let buf = device.create_buffer(&BufferDescriptor {
                label: Some("render-readback"),
                size: buffer_size,
                usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            self.readback_buf = Some(buf);
            self.readback_padded_bpr = padded_bpr;
            self.readback_size = buffer_size;
        }
    }

    /// Copy offscreen texture to readback buffer.
    ///
    /// # Errors
    /// Returns an error if texture or buffer is not available.
    pub fn copy_to_readback(
        &self,
        encoder: &mut CommandEncoder,
        width: u32,
        height: u32,
        padded_bpr: u32,
    ) -> Result<(), AnyhowError> {
        encoder.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: self
                    .offscreen_tex
                    .as_ref()
                    .ok_or_else(|| anyhow!("offscreen texture not available"))?,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            TexelCopyBufferInfo {
                buffer: self
                    .readback_buf
                    .as_ref()
                    .ok_or_else(|| anyhow!("readback buffer not available"))?,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bpr),
                    rows_per_image: Some(height),
                },
            },
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }

    /// Read pixels from the readback buffer.
    ///
    /// # Errors
    /// Returns an error if buffer mapping or readback fails.
    pub fn read_pixels(
        &self,
        device: &Arc<Device>,
        dimensions: (u32, u32, u32, u32), // (width, height, bytes_per_pixel, padded_bytes_per_row)
    ) -> Result<Vec<u8>, AnyhowError> {
        let (width, height, bytes_per_pixel, padded_bytes_per_row) = dimensions;
        let readback = self
            .readback_buf
            .as_ref()
            .ok_or_else(|| anyhow!("readback buffer not available"))?;
        let slice = readback.slice(..);
        let (sender, receiver) = channel();
        slice.map_async(MapMode::Read, move |res| {
            drop(sender.send(res));
        });
        loop {
            let _unused = device.poll(PollType::Wait);
            if let Ok(res) = receiver.try_recv() {
                res?;
                break;
            }
        }
        let mapped = slice.get_mapped_range();
        let row_bytes = width * bytes_per_pixel;
        let expected_total_bytes =
            (width as usize) * (height as usize) * (bytes_per_pixel as usize);
        let mut out = vec![0u8; expected_total_bytes];
        for row in 0..height as usize {
            let src_off = row * (padded_bytes_per_row as usize);
            let dst_off = row * (row_bytes as usize);
            out[dst_off..dst_off + (row_bytes as usize)]
                .copy_from_slice(&mapped[src_off..src_off + (row_bytes as usize)]);
        }
        drop(mapped);
        readback.unmap();
        Ok(out)
    }

    /// Get the offscreen texture for rendering.
    ///
    /// # Errors
    /// Returns an error if texture is not available.
    pub fn get_texture(&self, render_format: TextureFormat) -> Result<TextureView, AnyhowError> {
        let tex = self
            .offscreen_tex
            .as_ref()
            .ok_or_else(|| anyhow!("offscreen texture not available"))?;
        Ok(tex.create_view(&TextureViewDescriptor {
            format: Some(render_format),
            ..Default::default()
        }))
    }

    /// Clear all offscreen resources (called on resize).
    pub fn clear(&mut self) {
        self.offscreen_tex = None;
        self.readback_buf = None;
        self.readback_padded_bpr = 0;
        self.readback_size = 0;
    }
}

impl Default for OffscreenTarget {
    fn default() -> Self {
        Self::new()
    }
}
