//! Resource pool for managing GPU resource lifetimes and reuse.
//!
//! This module provides centralized resource management, separating lifetime
//! concerns from the rendering logic.

use std::collections::BTreeMap;

/// Handle to a pooled texture resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TextureHandle(pub usize);

/// Handle to a pooled buffer resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BufferHandle(pub usize);

/// Handle to a pooled bind group resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BindGroupHandle(pub usize);

/// Metadata for a pooled texture.
#[derive(Debug, Clone)]
pub struct TextureMetadata {
    pub width: u32,
    pub height: u32,
    pub in_use: bool,
}

/// Resource pool for managing GPU resources across frames.
///
/// This pool tracks resource lifetimes and enables reuse of offscreen
/// textures, buffers, and bind groups to reduce allocation overhead.
pub struct ResourcePool {
    textures: BTreeMap<TextureHandle, TextureMetadata>,
    next_texture_id: usize,
    frame_textures: Vec<TextureHandle>,
    frame_buffers: Vec<BufferHandle>,
    frame_bind_groups: Vec<BindGroupHandle>,
}

impl ResourcePool {
    /// Create a new empty resource pool.
    pub const fn new() -> Self {
        Self {
            textures: BTreeMap::new(),
            next_texture_id: 0,
            frame_textures: Vec::new(),
            frame_buffers: Vec::new(),
            frame_bind_groups: Vec::new(),
        }
    }

    /// Acquire a texture handle for the given dimensions.
    ///
    /// This will reuse an existing texture if one is available, or
    /// allocate a new handle if needed.
    pub fn acquire_texture(&mut self, width: u32, height: u32) -> TextureHandle {
        // Try to find an unused texture with matching dimensions
        for (handle, metadata) in &mut self.textures {
            if !metadata.in_use && metadata.width == width && metadata.height == height {
                metadata.in_use = true;
                self.frame_textures.push(*handle);
                return *handle;
            }
        }

        // Allocate a new texture handle
        let handle = TextureHandle(self.next_texture_id);
        self.next_texture_id += 1;

        self.textures.insert(
            handle,
            TextureMetadata {
                width,
                height,
                in_use: true,
            },
        );

        self.frame_textures.push(handle);
        handle
    }

    /// Release a texture handle, making it available for reuse.
    pub fn release_texture(&mut self, handle: TextureHandle) {
        if let Some(metadata) = self.textures.get_mut(&handle) {
            metadata.in_use = false;
        }
    }

    /// Register a buffer used in the current frame.
    pub fn register_frame_buffer(&mut self, handle: BufferHandle) {
        self.frame_buffers.push(handle);
    }

    /// Register a bind group used in the current frame.
    pub fn register_frame_bind_group(&mut self, handle: BindGroupHandle) {
        self.frame_bind_groups.push(handle);
    }

    /// Get all textures used in the current frame.
    pub fn frame_textures(&self) -> &[TextureHandle] {
        &self.frame_textures
    }

    /// Get all buffers used in the current frame.
    pub fn frame_buffers(&self) -> &[BufferHandle] {
        &self.frame_buffers
    }

    /// Get all bind groups used in the current frame.
    pub fn frame_bind_groups(&self) -> &[BindGroupHandle] {
        &self.frame_bind_groups
    }

    /// Clear per-frame resource tracking.
    ///
    /// This should be called at the end of each frame after submission.
    /// It releases all frame resources but keeps the pool for reuse.
    pub fn clear_frame_resources(&mut self) {
        // Release all textures used this frame
        let handles: Vec<TextureHandle> = self.frame_textures.clone();
        for handle in handles {
            self.release_texture(handle);
        }

        self.frame_textures.clear();
        self.frame_buffers.clear();
        self.frame_bind_groups.clear();
    }

    /// Clear all resources from the pool.
    ///
    /// This should be called when the renderer is being destroyed or reset.
    pub fn clear_all(&mut self) {
        self.textures.clear();
        self.frame_textures.clear();
        self.frame_buffers.clear();
        self.frame_bind_groups.clear();
        self.next_texture_id = 0;
    }

    /// Get the number of textures in the pool.
    pub fn texture_count(&self) -> usize {
        self.textures.len()
    }

    /// Get the number of textures currently in use.
    pub fn textures_in_use(&self) -> usize {
        self.textures.values().filter(|m| m.in_use).count()
    }
}

impl Default for ResourcePool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_texture() {
        let mut pool = ResourcePool::new();
        let handle = pool.acquire_texture(100, 100);
        assert_eq!(pool.texture_count(), 1);
        assert_eq!(pool.textures_in_use(), 1);
        assert!(pool.frame_textures().contains(&handle));
    }

    #[test]
    fn texture_reuse() {
        let mut pool = ResourcePool::new();
        let handle1 = pool.acquire_texture(100, 100);
        pool.clear_frame_resources();

        let handle2 = pool.acquire_texture(100, 100);
        assert_eq!(handle1, handle2); // Should reuse the same texture
        assert_eq!(pool.texture_count(), 1);
    }

    #[test]
    fn different_sizes() {
        let mut pool = ResourcePool::new();
        let handle1 = pool.acquire_texture(100, 100);
        let handle2 = pool.acquire_texture(200, 200);

        assert_ne!(handle1, handle2);
        assert_eq!(pool.texture_count(), 2);
    }

    #[test]
    fn clear_frame_resources() {
        let mut pool = ResourcePool::new();
        pool.acquire_texture(100, 100);
        pool.register_frame_buffer(BufferHandle(0));
        pool.register_frame_bind_group(BindGroupHandle(0));

        assert_eq!(pool.frame_textures().len(), 1);
        assert_eq!(pool.frame_buffers().len(), 1);
        assert_eq!(pool.frame_bind_groups().len(), 1);

        pool.clear_frame_resources();

        assert_eq!(pool.frame_textures().len(), 0);
        assert_eq!(pool.frame_buffers().len(), 0);
        assert_eq!(pool.frame_bind_groups().len(), 0);
        assert_eq!(pool.textures_in_use(), 0);
    }

    #[test]
    fn clear_all() {
        let mut pool = ResourcePool::new();
        pool.acquire_texture(100, 100);
        pool.acquire_texture(200, 200);

        pool.clear_all();

        assert_eq!(pool.texture_count(), 0);
        assert_eq!(pool.frame_textures().len(), 0);
    }
}
