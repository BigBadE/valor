use std::sync::Arc;
use wgpu::*;
use winit::dpi::PhysicalSize;
use winit::window::Window;

pub struct RenderState {
    window: Arc<Window>,
    device: Device,
    queue: Queue,
    size: PhysicalSize<u32>,
    surface: Surface<'static>,
    surface_format: TextureFormat,
}

impl RenderState {
    pub async fn new(window: Arc<Window>) -> RenderState {
        let instance = Instance::new(&InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&RequestAdapterOptions::default())
            .await
            .unwrap();
        let (device, queue) = adapter
            .request_device(&DeviceDescriptor::default())
            .await
            .unwrap();

        let size = window.inner_size();

        let surface = instance.create_surface(window.clone()).unwrap();
        let cap = surface.get_capabilities(&adapter);
        let surface_format = cap.formats[0];

        let state = RenderState {
            window,
            device,
            queue,
            size,
            surface,
            surface_format,
        };

        // Configure surface for the first time
        state.configure_surface();

        state
    }

    pub fn get_window(&self) -> &Window {
        &self.window
    }

    fn configure_surface(&self) {
        let surface_config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: self.surface_format,
            // Request compatibility with the sRGB-format texture view we‘re going to create later.
            view_formats: vec![self.surface_format.add_srgb_suffix()],
            alpha_mode: CompositeAlphaMode::Auto,
            width: self.size.width,
            height: self.size.height,
            desired_maximum_frame_latency: 2,
            present_mode: PresentMode::AutoVsync,
        };
        self.surface.configure(&self.device, &surface_config);
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.size = new_size;

        // reconfigure the surface
        self.configure_surface();
    }

    pub fn render(&mut self) -> Result<(), anyhow::Error>{
        // Create texture view
        let surface_texture = self
            .surface
            .get_current_texture()?;
        let texture_view = surface_texture
            .texture
            .create_view(&TextureViewDescriptor {
                // Without add_srgb_suffix() the image we will be working with
                // might not be "gamma correct".
                format: Some(self.surface_format.add_srgb_suffix()),
                ..Default::default()
            });

        // Renders a GREEN screen
        let mut encoder = self.device.create_command_encoder(&Default::default());
        // Create the renderpass which will clear the screen.
        let renderpass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &texture_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::GREEN),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        // If you wanted to call any drawing commands, they would go here.

        // End the renderpass.
        drop(renderpass);

        // Submit the command in the queue to execute
        self.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        surface_texture.present();
        Ok(())
    }
}