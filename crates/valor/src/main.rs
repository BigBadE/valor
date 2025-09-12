use crate::state::AppState;
use anyhow::{anyhow, Error};
use log::{error, info};
use page_handler::state::HtmlPage;
use std::env;
use std::sync::Arc;
use tokio::runtime::Runtime;
use url::Url;
use wgpu_renderer::state::RenderState;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

mod state;
mod window;

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

impl App {
    fn resume(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Error> {
        // Create window object
        let window = Arc::new(event_loop.create_window(Window::default_attributes())?);

        let runtime = Runtime::new()?;

        self.state = Some(AppState {
            render_state: runtime.block_on(RenderState::new(window.clone())),
            pages: vec![runtime.block_on(HtmlPage::new(
                runtime.handle(),
                Url::parse(&format!(
                    "file://{}/assets/home.html",
                    env::current_dir()?.display()
                ))?,
            ))?],
            runtime,
        });

        window.request_redraw();
        Ok(())
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) -> Result<(), Error> {
        let state = self.state.as_mut().ok_or_else(|| anyhow!("App state is not set."))?;
        match event {
            WindowEvent::CloseRequested => {
                info!("The close button was pressed; stopping");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                info!("App: RedrawRequested -> render()");
                state.render_state.render()?;
            }
            WindowEvent::Resized(size) => {
                state.render_state.resize(size);
            }
            _ => (),
        }
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.resume(event_loop) {
            error!("Failed to resume: {error}");
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let Err(error) = self.window_event(event_loop, id, event) {
            error!("Failed to handle event: {error}");
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = self.state.as_mut() {
            for page in &mut state.pages {
                if let Err(err) = state.runtime.block_on(page.update()) {
                    error!("Failed to apply page updates: {err:?}");
                }
            }
            // Build and install the current retained display list from the first page
            if let Some(first_page) = state.pages.get_mut(0) {
                match first_page.display_list_retained_snapshot() {
                    Ok(dl) => state.render_state.set_retained_display_list(dl),
                    Err(err) => error!("Failed to build retained display list: {err:?}"),
                }
            }
            // Schedule a redraw so the latest display list is rendered
            info!("App: requesting redraw");
            state.render_state.get_window().request_redraw();
        }
    }
}
