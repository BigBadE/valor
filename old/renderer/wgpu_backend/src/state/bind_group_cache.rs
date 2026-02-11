//! Bind group cache for efficient GPU resource binding.
//!
//! This module caches bind groups to avoid redundant creation of identical
//! bind groups across frames. Bind groups are expensive to create, so reusing
//! them when the underlying resources haven't changed provides significant
//! CPU performance benefits.

use std::collections::HashMap;
use std::sync::Arc;
use wgpu::{BindGroup, BindGroupLayout, Device, Sampler, TextureView};

/// Key for identifying a bind group in the cache.
///
/// This uses raw pointers for texture views since TextureView doesn't implement
/// Hash or Eq. The cache must be invalidated when textures are recreated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct BindGroupKey {
    /// Pointer to the texture view (used for identity comparison).
    texture_view_ptr: usize,
    /// Pointer to the sampler (used for identity comparison).
    sampler_ptr: usize,
}

impl BindGroupKey {
    /// Create a new bind group key from a texture view and sampler.
    ///
    /// # Safety
    /// The caller must ensure that the texture view and sampler remain valid
    /// for the lifetime of the cache entry.
    fn new(texture_view: &TextureView, sampler: &Sampler) -> Self {
        Self {
            texture_view_ptr: texture_view as *const _ as usize,
            sampler_ptr: sampler as *const _ as usize,
        }
    }
}

/// Cache for storing and reusing bind groups.
///
/// Bind groups are immutable GPU objects that bind resources (textures, buffers)
/// to shader binding points. Creating them is CPU-intensive, so caching identical
/// bind groups across frames reduces overhead.
pub struct BindGroupCache {
    /// Map from bind group key to cached bind group.
    cache: HashMap<BindGroupKey, Arc<BindGroup>>,
    /// Number of cache hits (for statistics).
    hits: usize,
    /// Number of cache misses (for statistics).
    misses: usize,
}

impl BindGroupCache {
    /// Create a new empty bind group cache.
    #[inline]
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            hits: 0,
            misses: 0,
        }
    }

    /// Get or create a bind group for a texture and sampler.
    ///
    /// If a bind group with the same resources already exists in the cache,
    /// it will be returned. Otherwise, a new bind group will be created,
    /// cached, and returned.
    ///
    /// # Arguments
    ///
    /// * `device` - The WGPU device to create the bind group with
    /// * `layout` - The bind group layout
    /// * `texture_view` - The texture view to bind
    /// * `sampler` - The sampler to bind
    /// * `label` - Optional label for debugging
    ///
    /// # Returns
    ///
    /// An Arc to the bind group (cached or newly created).
    pub fn get_or_create(
        &mut self,
        device: &Device,
        layout: &BindGroupLayout,
        texture_view: &TextureView,
        sampler: &Sampler,
        label: Option<&str>,
    ) -> Arc<BindGroup> {
        let key = BindGroupKey::new(texture_view, sampler);

        if let Some(bind_group) = self.cache.get(&key) {
            self.hits += 1;
            return Arc::clone(bind_group);
        }

        self.misses += 1;

        // Create new bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label,
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        let bind_group = Arc::new(bind_group);
        self.cache.insert(key, Arc::clone(&bind_group));
        bind_group
    }

    /// Clear the cache.
    ///
    /// This should be called when textures are recreated (e.g., window resize)
    /// to ensure stale bind groups are not reused.
    #[inline]
    pub fn clear(&mut self) {
        self.cache.clear();
        self.hits = 0;
        self.misses = 0;
    }

    /// Get the number of cache hits.
    #[inline]
    pub const fn hits(&self) -> usize {
        self.hits
    }

    /// Get the number of cache misses.
    #[inline]
    pub const fn misses(&self) -> usize {
        self.misses
    }

    /// Get the cache hit rate as a percentage.
    #[inline]
    pub fn hit_rate(&self) -> f32 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            (self.hits as f32 / total as f32) * 100.0
        }
    }

    /// Get the number of cached bind groups.
    #[inline]
    pub fn size(&self) -> usize {
        self.cache.len()
    }
}

impl Default for BindGroupCache {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_statistics() {
        let cache = BindGroupCache::new();
        assert_eq!(cache.hits(), 0);
        assert_eq!(cache.misses(), 0);
        assert_eq!(cache.hit_rate(), 0.0);
        assert_eq!(cache.size(), 0);
    }

    #[test]
    fn cache_clear() {
        let mut cache = BindGroupCache::new();
        // Simulate some cache activity
        cache.hits = 10;
        cache.misses = 5;

        cache.clear();
        assert_eq!(cache.hits(), 0);
        assert_eq!(cache.misses(), 0);
        assert_eq!(cache.size(), 0);
    }

    #[test]
    fn hit_rate_calculation() {
        let mut cache = BindGroupCache::new();
        cache.hits = 80;
        cache.misses = 20;

        assert!((cache.hit_rate() - 80.0).abs() < 0.01);
    }
}
