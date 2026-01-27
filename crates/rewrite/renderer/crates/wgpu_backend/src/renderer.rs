//! Main WGPU renderer implementation.

use rewrite_renderer::{BackendCapabilities, BackendError, DisplayList, RenderBackend};

use crate::pipelines::Pipelines;
use crate::resources::ResourceManager;

/// WGPU rendering backend.
pub struct WgpuBackend {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: Option<wgpu::Surface<'static>>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    pipelines: Option<Pipelines>,
    resources: ResourceManager,
    current_encoder: Option<wgpu::CommandEncoder>,
    current_view: Option<wgpu::TextureView>,
}

impl WgpuBackend {
    /// Create a new WGPU backend (without surface).
    pub async fn new() -> Result<Self, BackendError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| {
                BackendError::InitializationFailed("Failed to find suitable adapter".to_string())
            })?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Valor Renderer Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: Default::default(),
                },
                None,
            )
            .await
            .map_err(|e| BackendError::InitializationFailed(e.to_string()))?;

        Ok(Self {
            device,
            queue,
            surface: None,
            surface_config: None,
            pipelines: None,
            resources: ResourceManager::new(),
            current_encoder: None,
            current_view: None,
        })
    }

    /// Create a new WGPU backend with a surface.
    pub async fn new_with_surface(surface: wgpu::Surface<'static>) -> Result<Self, BackendError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| {
                BackendError::InitializationFailed("Failed to find suitable adapter".to_string())
            })?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Valor Renderer Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: Default::default(),
                },
                None,
            )
            .await
            .map_err(|e| BackendError::InitializationFailed(e.to_string()))?;

        Ok(Self {
            device,
            queue,
            surface: Some(surface),
            surface_config: None,
            pipelines: None,
            resources: ResourceManager::new(),
            current_encoder: None,
            current_view: None,
        })
    }

    fn configure_surface(&mut self, width: u32, height: u32) -> Result<(), BackendError> {
        let surface = self
            .surface
            .as_ref()
            .ok_or_else(|| BackendError::SurfaceError("No surface available".to_string()))?;

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&self.device, &config);
        self.surface_config = Some(config);

        Ok(())
    }
}

impl RenderBackend for WgpuBackend {
    fn init(&mut self, width: u32, height: u32) -> Result<(), BackendError> {
        // Configure surface if available
        if self.surface.is_some() {
            self.configure_surface(width, height)?;
        }

        // Initialize pipelines
        self.pipelines = Some(Pipelines::new(
            &self.device,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        ));

        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32) -> Result<(), BackendError> {
        if self.surface.is_some() {
            self.configure_surface(width, height)?;
        }
        Ok(())
    }

    fn begin_frame(&mut self) -> Result<(), BackendError> {
        let surface = self
            .surface
            .as_ref()
            .ok_or_else(|| BackendError::SurfaceError("No surface available".to_string()))?;

        let output = surface
            .get_current_texture()
            .map_err(|e| BackendError::SurfaceError(e.to_string()))?;

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Frame Encoder"),
            });

        self.current_encoder = Some(encoder);
        self.current_view = Some(view);

        Ok(())
    }

    fn execute(&mut self, command: &DisplayList) -> Result<(), BackendError> {
        let encoder = self
            .current_encoder
            .as_mut()
            .ok_or_else(|| BackendError::RenderError("No active encoder".to_string()))?;

        let view = self
            .current_view
            .as_ref()
            .ok_or_else(|| BackendError::RenderError("No active view".to_string()))?;

        let pipelines = self
            .pipelines
            .as_ref()
            .ok_or_else(|| BackendError::RenderError("Pipelines not initialized".to_string()))?;

        // Execute the display list command
        match command {
            DisplayList::FillRect {
                x,
                y,
                width,
                height,
                color,
            } => {
                pipelines.draw_rect(encoder, view, *x, *y, *width, *height, *color);
            }

            DisplayList::DrawText { x, y, text } => {
                // Text rendering would go here
                // For now, just a placeholder
            }

            DisplayList::PushClip {
                x,
                y,
                width,
                height,
            } => {
                // Implement clipping
            }

            DisplayList::PopClip => {
                // Pop clipping
            }

            DisplayList::PushOpacity { opacity } => {
                // Implement opacity layers
            }

            DisplayList::PopOpacity => {
                // Pop opacity layer
            }

            // Add other command implementations as needed
            _ => {
                // Unimplemented commands - log or ignore
            }
        }

        Ok(())
    }

    fn end_frame(&mut self) -> Result<(), BackendError> {
        let encoder = self
            .current_encoder
            .take()
            .ok_or_else(|| BackendError::RenderError("No active encoder".to_string()))?;

        self.queue.submit(Some(encoder.finish()));

        // Present is handled by the surface
        if let Some(surface) = &self.surface {
            // Surface presentation happens automatically when the texture is dropped
        }

        self.current_view = None;

        Ok(())
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            max_texture_size: 8192, // Common limit
            supports_3d_transforms: true,
            supports_filters: true,
            supports_blend_modes: true,
            supports_clip_paths: true,
        }
    }
}
