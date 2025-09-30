use wgpu::{
    Device, Extent3d, Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
};

/// Texture pool for efficient reuse of offscreen textures in opacity groups.
/// Spec: Performance optimization for stacking context rendering
#[derive(Debug)]
pub struct TexturePool {
    /// Available textures: (width, height, texture)
    available: Vec<(u32, u32, Texture)>,
}

impl Default for TexturePool {
    fn default() -> Self {
        Self::new()
    }
}

impl TexturePool {
    /// Create a new texture pool
    pub fn new() -> Self {
        Self {
            available: Vec::new(),
        }
    }

    /// Get or create a texture with the specified dimensions and format
    /// Spec: Reuse textures to minimize GPU memory allocation overhead
    pub fn get_or_create(
        &mut self,
        device: &Device,
        width: u32,
        height: u32,
        format: TextureFormat,
    ) -> Texture {
        // Find suitable existing texture (allow up to 25% larger to improve reuse)
        let max_width = width + width / 4;
        let max_height = height + height / 4;

        if let Some(pos) = self.available.iter().position(|(w, h, _)| {
            *w >= width && *h >= height && *w <= max_width && *h <= max_height
        }) {
            let (_w, _h, texture) = self.available.remove(pos);
            return texture;
        }

        // Create new texture with tight bounds
        device.create_texture(&TextureDescriptor {
            label: Some("opacity-group-texture"),
            size: Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    }

    /// Return a texture to the pool for reuse
    pub fn return_texture(&mut self, texture: Texture, width: u32, height: u32) {
        self.available.push((width.max(1), height.max(1), texture));
    }

    /// Clear all textures from the pool (called on resize)
    pub fn clear(&mut self) {
        self.available.clear();
    }
}
