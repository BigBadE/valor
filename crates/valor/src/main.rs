use std::sync::Arc;
use log::{error, info};
use tokio::runtime::Runtime;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};
use page_handler::state::PageState;
use wgpu_renderer::state::RenderState;
use crate::state::AppState;

mod window;
mod state;

pub fn main() {
    env_logger::init();

    let event_loop = EventLoop::new().unwrap();

    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}

#[derive(Default)]
struct App {
    state: Option<AppState>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Create window object
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes())
                .unwrap(),
        );

        let runtime = Runtime::new().unwrap();

        self.state = Some(AppState {
            render_state: runtime.block_on(RenderState::new(window.clone())),
            runtime,
            pages: vec![PageState::new("")]
        });

        window.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let state = self.state.as_mut().unwrap();
        match event {
            WindowEvent::CloseRequested => {
                info!("The close button was pressed; stopping");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = state.render_state.render() {
                    error!("Failed to render: {}", error);
                }
                state.render_state.get_window().request_redraw();
            }
            WindowEvent::Resized(size) => {
                state.render_state.resize(size);
            }
            _ => (),
        }
    }
}