//! Resource management for textures, buffers, and other GPU resources.

use std::collections::HashMap;

/// Manages GPU resources (textures, buffers, etc.).
pub struct ResourceManager {
    textures: HashMap<u64, TextureResource>,
    buffers: HashMap<u64, BufferResource>,
    next_id: u64,
}

impl ResourceManager {
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
            buffers: HashMap::new(),
            next_id: 0,
        }
    }

    pub fn allocate_texture(&mut self, texture: wgpu::Texture) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.textures.insert(
            id,
            TextureResource {
                texture,
                view: None,
            },
        );
        id
    }

    pub fn get_texture(&self, id: u64) -> Option<&TextureResource> {
        self.textures.get(&id)
    }

    pub fn allocate_buffer(&mut self, buffer: wgpu::Buffer) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.buffers.insert(id, BufferResource { buffer });
        id
    }

    pub fn get_buffer(&self, id: u64) -> Option<&BufferResource> {
        self.buffers.get(&id)
    }

    pub fn remove_texture(&mut self, id: u64) {
        self.textures.remove(&id);
    }

    pub fn remove_buffer(&mut self, id: u64) {
        self.buffers.remove(&id);
    }
}

pub struct TextureResource {
    pub texture: wgpu::Texture,
    pub view: Option<wgpu::TextureView>,
}

pub struct BufferResource {
    pub buffer: wgpu::Buffer,
}
