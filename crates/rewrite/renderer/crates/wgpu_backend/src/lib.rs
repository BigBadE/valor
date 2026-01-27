//! WGPU-based renderer backend.
//!
//! This crate provides GPU-accelerated rendering using WGPU.

mod pipelines;
mod renderer;
mod resources;
mod shaders;

pub use renderer::WgpuBackend;
