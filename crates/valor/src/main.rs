use crate::state::AppState;
use anyhow::{Error, anyhow};
use js::ChromeHostCommand;
use log::{error, info};
use page_handler::config::ValorConfig;
use page_handler::events::KeyMods;
use page_handler::state::HtmlPage;
use std::sync::Arc;
use tokio::runtime::Runtime;
use url::Url;
use valor::factory::create_chrome_and_content;
use wgpu_renderer::display_list::{DisplayItem, DisplayList};
use wgpu_renderer::state::{Layer, RenderState};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

mod state;
mod window;

pub fn main() {
    env_logger::init();

    let event_loop = EventLoop::new().unwrap();

    // Use Wait so we sleep between events; we explicitly request redraws when needed.
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}

#[derive(Default)]
struct App {
    state: Option<AppState>,
}

impl App {
    #[inline]
    fn set_canvas_background_from_content(state: &mut AppState) {
        if let Some(p) = state.pages.get_mut(1) {
            let cc = p.background_rgba();
            state.render_state.set_clear_color(cc);
        }
    }

    fn tick_pages_and_maybe_redraw(state: &mut AppState) {
        let mut any_needs_redraw = false;
        for page in &mut state.pages {
            if let Err(err) = state.runtime.block_on(page.update()) {
                error!("Failed to apply page updates: {err:?}");
            }
        }
        for page in &mut state.pages {
            if page.take_needs_redraw() {
                any_needs_redraw = true;
            }
        }
        if any_needs_redraw {
            Self::rebuild_layers_after_update(state);
        }
    }

    #[inline]
    fn process_chrome_cmd(state: &mut AppState, cmd: ChromeHostCommand) {
        match cmd {
            ChromeHostCommand::Navigate(url_str) => Self::handle_navigate(state, url_str),
            ChromeHostCommand::Back
            | ChromeHostCommand::Forward
            | ChromeHostCommand::Reload
            | ChromeHostCommand::OpenTab(_)
            | ChromeHostCommand::CloseTab(_) => {
                info!("chromeHost command stub: {cmd:?}");
            }
        }
    }

    #[inline]
    fn push_layer_opt(state: &mut AppState, layer: Layer, dl_opt: Option<DisplayList>) {
        if let Some(dl) = dl_opt {
            match layer {
                Layer::Content(_) => state.render_state.push_layer(Layer::Content(dl)),
                Layer::Chrome(_) => state.render_state.push_layer(Layer::Chrome(dl)),
                Layer::Background => { /* not used with dl here */ }
            }
        }
    }

    #[inline]
    fn push_layer_with_bg(
        render_state: &mut RenderState,
        layer: Layer,
        bg_rect: Option<DisplayItem>,
        dl: DisplayList,
    ) {
        match (bg_rect, layer) {
            (Some(rect), Layer::Content(_)) => {
                let mut items = Vec::with_capacity(dl.items.len() + 1);
                items.push(rect);
                items.extend(dl.items);
                render_state.push_layer(Layer::Content(DisplayList::from_items(items)));
            }
            (Some(rect), Layer::Chrome(_)) => {
                let mut items = Vec::with_capacity(dl.items.len() + 1);
                items.push(rect);
                items.extend(dl.items);
                render_state.push_layer(Layer::Chrome(DisplayList::from_items(items)));
            }
            (None, Layer::Content(_)) => render_state.push_layer(Layer::Content(dl)),
            (None, Layer::Chrome(_)) => render_state.push_layer(Layer::Chrome(dl)),
            _ => {}
        }
    }

    fn rebuild_layers_after_update(state: &mut AppState) {
        state.render_state.clear_layers();
        Self::set_canvas_background_from_content(state);
        let win_size = state.render_state.get_window().inner_size();
        let fw = win_size.width as f32;
        let fh = win_size.height as f32;
        let content_dl = state
            .pages
            .get_mut(1)
            .and_then(|p| p.display_list_retained_snapshot().ok());
        let chrome_dl = state
            .pages
            .get_mut(0)
            .and_then(|p| p.display_list_retained_snapshot().ok());
        if let Some(dl) = content_dl {
            // Prepend full-viewport bg
            let cc = state
                .pages
                .get_mut(1)
                .map(|p| p.background_rgba())
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            let bg = DisplayItem::Rect {
                x: 0.0,
                y: 0.0,
                width: fw,
                height: fh,
                color: cc,
            };
            Self::push_layer_with_bg(
                &mut state.render_state,
                Layer::Content(DisplayList::new()),
                Some(bg),
                dl,
            );
        }
        if let Some(dl) = chrome_dl {
            // Prepend chrome strip bg
            let cc = state
                .pages
                .get_mut(0)
                .map(|p| p.background_rgba())
                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
            let bg = DisplayItem::Rect {
                x: 0.0,
                y: 0.0,
                width: fw,
                height: state.chrome_bar_height_px as f32,
                color: cc,
            };
            Self::push_layer_with_bg(
                &mut state.render_state,
                Layer::Chrome(DisplayList::new()),
                Some(bg),
                dl,
            );
        }
        state.render_state.get_window().request_redraw();
    }

    fn handle_navigate(state: &mut AppState, url_str: String) {
        let parsed = Url::parse(&url_str).or_else(|_| Url::parse(&format!("https://{url_str}")));
        let Ok(target_url) = parsed else {
            error!("Invalid URL '{url_str}': {:?}", parsed.err());
            return;
        };
        let config = ValorConfig::from_env();
        match state
            .runtime
            .block_on(HtmlPage::new(state.runtime.handle(), target_url, config))
        {
            Ok(new_page) => {
                if state.pages.len() >= 2 {
                    state.pages[1] = new_page;
                } else {
                    state.pages.push(new_page);
                }
            }
            Err(e) => error!("Navigate failed to create page: {e:?}"),
        }
    }
    fn resume(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Error> {
        // Create window object
        let window = Arc::new(event_loop.create_window(Window::default_attributes())?);

        let runtime = Runtime::new()?;

        // Create renderer
        let render_state = runtime.block_on(RenderState::new(window.clone()));

        // Create chrome and initial content pages via shared factory
        let init = create_chrome_and_content(&runtime, Url::parse("https://example.com/")?)?;
        let chrome_page = init.chrome_page;
        let content_page = init.content_page;
        let chrome_rx = init.chrome_host_rx;

        self.state = Some(AppState {
            render_state,
            pages: vec![chrome_page, content_page],
            runtime,
            chrome_host_rx: chrome_rx,
            focused_page_index: 0, // focus chrome by default
            pointer_target_index: None,
            last_cursor_pos: None,
            chrome_bar_height_px: 56.0,
        });
        // Build initial layers so the first redraw has content instead of a blank clear.
        if let Some(state) = self.state.as_mut() {
            Self::rebuild_layers_after_update(state);
        }
        window.request_redraw();
        Ok(())
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) -> Result<(), Error> {
        let state = self
            .state
            .as_mut()
            .ok_or_else(|| anyhow!("App state is not set."))?;
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
            WindowEvent::CursorMoved { position, .. } => {
                let state = self.state.as_mut().unwrap();
                let x = position.x;
                let y = position.y;
                state.last_cursor_pos = Some((x, y));
                let target = if y < state.chrome_bar_height_px {
                    0usize
                } else {
                    1usize
                };
                state.pointer_target_index = Some(target);
                if let Some(page) = state.pages.get_mut(target) {
                    page.dispatch_pointer_move(x, y);
                    // Apply a single update for responsive interactions
                    if let Err(err) = state.runtime.block_on(page.update()) {
                        error!("Failed to apply page updates: {err:?}");
                    }
                    if page.take_needs_redraw() {
                        Self::rebuild_layers_after_update(state);
                    }
                }
            }
            WindowEvent::MouseInput {
                state: btn_state,
                button,
                ..
            } => {
                let state_ref = self.state.as_mut().unwrap();
                // Determine pointer target from last known cursor position
                let target = state_ref
                    .pointer_target_index
                    .unwrap_or(state_ref.focused_page_index);
                let (x, y) = state_ref.last_cursor_pos.unwrap_or((0.0, 0.0));
                if let Some(page) = state_ref.pages.get_mut(target) {
                    let button_code = match button {
                        MouseButton::Left => 0u32,
                        MouseButton::Right => 2u32,
                        MouseButton::Middle => 1u32,
                        _ => 0u32,
                    };
                    match btn_state {
                        ElementState::Pressed => {
                            state_ref.focused_page_index = target; // focus follows click
                            page.dispatch_pointer_down(x, y, button_code);
                        }
                        ElementState::Released => {
                            page.dispatch_pointer_up(x, y, button_code);
                        }
                    }
                    // Apply a single update for responsive interactions
                    if let Err(err) = state_ref.runtime.block_on(page.update()) {
                        error!("Failed to apply page updates: {err:?}");
                    }
                    if page.take_needs_redraw() {
                        Self::rebuild_layers_after_update(state_ref);
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let state_ref = self.state.as_mut().unwrap();
                let idx = state_ref
                    .focused_page_index
                    .min(state_ref.pages.len().saturating_sub(1));
                let key_str: String = format!("{:?}", event.logical_key);
                let code_str: String = format!("{:?}", event.physical_key);
                let is_down = event.state == ElementState::Pressed;
                if let Some(page) = state_ref.pages.get_mut(idx) {
                    let mods = KeyMods {
                        ctrl: false,
                        alt: false,
                        shift: false,
                    };
                    if is_down {
                        page.dispatch_key_down(&key_str, &code_str, mods);
                    } else {
                        page.dispatch_key_up(&key_str, &code_str, mods);
                    }
                    // Apply a single update for responsive interactions
                    if let Err(err) = state_ref.runtime.block_on(page.update()) {
                        error!("Failed to apply page updates: {err:?}");
                    }
                    if page.take_needs_redraw() {
                        state_ref.render_state.clear_layers();
                        let content_dl = state_ref
                            .pages
                            .get_mut(1)
                            .and_then(|p| p.display_list_retained_snapshot().ok());
                        let chrome_dl = state_ref
                            .pages
                            .get_mut(0)
                            .and_then(|p| p.display_list_retained_snapshot().ok());
                        Self::push_layer_opt(
                            state_ref,
                            Layer::Content(DisplayList::new()),
                            content_dl,
                        );
                        Self::push_layer_opt(
                            state_ref,
                            Layer::Chrome(DisplayList::new()),
                            chrome_dl,
                        );
                        info!("App: requesting redraw");
                        state_ref.render_state.get_window().request_redraw();
                    }
                }
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
            // Drain any pending chromeHost commands
            loop {
                match state.chrome_host_rx.try_recv() {
                    Ok(cmd) => Self::process_chrome_cmd(state, cmd),
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
                }
            }

            // During initial loading or navigation, tick pages; otherwise idle.
            if state.pages.iter().any(|p| !p.parsing_finished()) {
                Self::tick_pages_and_maybe_redraw(state);
            }
        }
    }
}
