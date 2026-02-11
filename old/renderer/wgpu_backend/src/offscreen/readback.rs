//! Texture readback from GPU to CPU.

use anyhow::Result as AnyhowResult;
use std::sync::mpsc::channel;
use wgpu::*;

/// Parameters for texture readback.
pub struct ReadbackParams<'readback> {
    /// Command encoder (consumed).
    pub encoder: CommandEncoder,
    /// Texture to read back.
    pub texture: &'readback Texture,
    /// GPU device for creating buffers.
    pub device: &'readback Device,
    /// Command queue for submission.
    pub queue: &'readback Queue,
    /// Texture width in pixels.
    pub width: u32,
    /// Texture height in pixels.
    pub height: u32,
}

/// Read back texture from GPU to CPU buffer.
///
/// # Errors
/// Returns an error if buffer mapping or readback fails.
pub fn readback_texture(params: ReadbackParams<'_>) -> AnyhowResult<Vec<u8>> {
    let bytes_per_pixel: u32 = 4;
    let row_bytes: u32 = params.width * bytes_per_pixel;
    let align: u32 = 256;
    let padded_bpr: u32 = row_bytes.div_ceil(align) * align;
    let buffer_size = u64::from(padded_bpr) * u64::from(params.height);
    let readback = params.device.create_buffer(&BufferDescriptor {
        label: Some("offscreen-readback"),
        size: buffer_size,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = params.encoder;
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: params.texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        TexelCopyBufferInfo {
            buffer: &readback,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bpr),
                rows_per_image: Some(params.height),
            },
        },
        Extent3d {
            width: params.width,
            height: params.height,
            depth_or_array_layers: 1,
        },
    );
    params.queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    let (sender, receiver) = channel();
    slice.map_async(MapMode::Read, move |res| {
        drop(sender.send(res));
    });
    loop {
        drop(params.device.poll(PollType::Wait));
        if let Ok(res) = receiver.try_recv() {
            res?;
            break;
        }
    }
    let mapped = slice.get_mapped_range();
    let mut data = vec![0u8; (row_bytes as usize) * (params.height as usize)];
    for row in 0..params.height as usize {
        let src_offset = row * (padded_bpr as usize);
        let dst_offset = row * (row_bytes as usize);
        let src = &mapped[src_offset..src_offset + (row_bytes as usize)];
        let dst = &mut data[dst_offset..dst_offset + (row_bytes as usize)];
        dst.copy_from_slice(src);
    }
    drop(mapped);
    readback.unmap();
    Ok(data)
}
