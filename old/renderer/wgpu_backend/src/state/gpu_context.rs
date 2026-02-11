//! GPU context management for WGPU backend.
//!
//! This module contains the `GpuContext` struct which encapsulates all GPU device,
//! queue, instance, and surface management. This is a focused component with a single
//! responsibility: managing the GPU resources and their lifecycle.

use anyhow::{Error as AnyhowError, anyhow};
use std::sync::Arc;
use wgpu::*;
use winit::dpi::PhysicalSize;
use winit::window::Window;

/// GPU context encapsulating device, queue, instance, and surface management.
/// This struct has a single responsibility: managing GPU resources.
pub struct GpuContext {
    /// Window handle for the render target.
    window: Arc<Window>,
    /// WGPU instance (must be kept alive for surface lifetime).
    _instance: Instance,
    /// GPU device for creating resources.
    device: Arc<Device>,
    /// Command queue for submitting work to the GPU.
    queue: Queue,
    /// Current framebuffer size.
    size: PhysicalSize<u32>,
    /// Optional surface for presenting to the window.
    surface: Option<Surface<'static>>,
    /// Surface texture format.
    surface_format: TextureFormat,
    /// Render target format.
    render_format: TextureFormat,
}

impl GpuContext {
    /// Create a new GPU context with device, queue, and surface initialization.
    ///
    /// # Errors
    /// Returns an error if adapter or device initialization fails.
    pub async fn new(window: Arc<Window>) -> Result<Self, AnyhowError> {
        let (instance, adapter, device, queue) = Self::initialize_device().await?;
        let size = window.inner_size();
        let (surface, surface_format, render_format) =
            Self::setup_surface(&window, &instance, &adapter, &device, size);

        Ok(Self {
            window,
            _instance: instance,
            device,
            queue,
            size,
            surface,
            surface_format,
            render_format,
        })
    }

    /// Initialize GPU device and queue.
    ///
    /// # Errors
    /// Returns an error if adapter or device initialization fails.
    async fn initialize_device() -> Result<(Instance, Adapter, Arc<Device>, Queue), AnyhowError> {
        let instance = Instance::new(&InstanceDescriptor {
            backends: Backends::DX12 | Backends::VULKAN | Backends::GL,
            flags: InstanceFlags::VALIDATION | InstanceFlags::DEBUG,
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|err| anyhow!("Failed to find a suitable GPU adapter: {err}"))?;
        let device_descriptor = DeviceDescriptor {
            label: Some("valor-render-device"),
            required_features: Features::DUAL_SOURCE_BLENDING,
            required_limits: Limits::default(),
            memory_hints: MemoryHints::default(),
            trace: Trace::default(),
        };
        let (device, queue) = adapter
            .request_device(&device_descriptor)
            .await
            .map_err(|err| anyhow!("Failed to create GPU device: {err}"))?;
        device.on_uncaptured_error(Box::new(|error| {
            log::error!(target: "wgpu_renderer", "Uncaptured WGPU error: {error:?}");
        }));
        Ok((instance, adapter, Arc::new(device), queue))
    }

    /// Setup surface with format selection and configuration.
    fn setup_surface(
        window: &Arc<Window>,
        instance: &Instance,
        adapter: &Adapter,
        device: &Arc<Device>,
        size: PhysicalSize<u32>,
    ) -> (Option<Surface<'static>>, TextureFormat, TextureFormat) {
        instance.create_surface(Arc::clone(window)).map_or_else(
            |_| {
                // CSS requires sRGB-space rendering - use Rgba8Unorm throughout
                (None, TextureFormat::Rgba8Unorm, TextureFormat::Rgba8Unorm)
            },
            |surface| {
                let capabilities = surface.get_capabilities(adapter);
                if capabilities.formats.is_empty() {
                    // Headless/software - use Bgra8Unorm (widely supported)
                    (None, TextureFormat::Bgra8Unorm, TextureFormat::Bgra8Unorm)
                } else {
                    // Use Bgra8Unorm - widely supported and matches WSL surface capabilities
                    // CSS requires sRGB-space blending, so use non-sRGB format
                    let surface_fmt = TextureFormat::Bgra8Unorm;
                    let render_fmt = TextureFormat::Bgra8Unorm;
                    let surface_config = SurfaceConfiguration {
                        usage: TextureUsages::RENDER_ATTACHMENT,
                        format: surface_fmt,
                        view_formats: vec![render_fmt],
                        alpha_mode: CompositeAlphaMode::Auto,
                        width: size.width,
                        height: size.height,
                        desired_maximum_frame_latency: 2,
                        present_mode: PresentMode::AutoVsync,
                    };
                    surface.configure(device, &surface_config);
                    (Some(surface), surface_fmt, render_fmt)
                }
            },
        )
    }

    /// Configure the swapchain/surface to match the current size and formats.
    pub fn configure_surface(&self) {
        let surface_config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: self.surface_format,
            view_formats: vec![self.render_format],
            alpha_mode: CompositeAlphaMode::Auto,
            width: self.size.width,
            height: self.size.height,
            desired_maximum_frame_latency: 2,
            present_mode: PresentMode::AutoVsync,
        };
        if let Some(surface) = &self.surface {
            surface.configure(&self.device, &surface_config);
        }
    }

    /// Resize the GPU context and reconfigure the surface.
    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.size = new_size;
        self.configure_surface();
    }

    /// Get the current surface texture for rendering.
    ///
    /// # Errors
    /// Returns an error if no surface is available or surface texture acquisition fails.
    pub fn get_current_texture(&self) -> Result<SurfaceTexture, AnyhowError> {
        let surface = self
            .surface
            .as_ref()
            .ok_or_else(|| anyhow!("no surface available for on-screen render"))?;
        Ok(surface.get_current_texture()?)
    }

    /// Get a reference to the GPU device.
    pub const fn device(&self) -> &Arc<Device> {
        &self.device
    }

    /// Get a reference to the GPU queue.
    pub const fn queue(&self) -> &Queue {
        &self.queue
    }

    /// Get a reference to the window.
    pub fn window(&self) -> &Window {
        &self.window
    }

    /// Get the current framebuffer size.
    pub const fn size(&self) -> PhysicalSize<u32> {
        self.size
    }

    /// Get the render texture format.
    pub const fn render_format(&self) -> TextureFormat {
        self.render_format
    }
}
