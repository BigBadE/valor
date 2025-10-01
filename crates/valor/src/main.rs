//! Valor browser main application binary.

use crate::state::AppState;
use anyhow::{Error, anyhow};
use env_logger::init as env_logger_init;
use js::ChromeHostCommand;
use log::{error, info};
use page_handler::config::ValorConfig;
use page_handler::events::KeyMods;
use page_handler::state::HtmlPage;
use renderer::{DisplayItem, DisplayList};
use std::process::exit;
use std::sync::Arc;
use tokio::runtime::Runtime;
use url::Url;
use valor::factory::create_chrome_and_content;
use wgpu_backend::{Layer, RenderState};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

/// Application state module.
mod state;
/// Window utilities module.
mod window;

/// Main entry point for the Valor browser application.
///
/// # Panics
/// Panics if the event loop cannot be created or if the application fails to run.
pub fn main() {
    env_logger_init();

    let event_loop = match EventLoop::new() {
        Ok(event_loop_instance) => event_loop_instance,
        Err(err) => {
            error!("Failed to create event loop: {err}");
            exit(1);
        }
    };

    // Use Wait so we sleep between events; we explicitly request redraws when needed.
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::default();
    if let Err(err) = event_loop.run_app(&mut app) {
        error!("Failed to run application: {err}");
        exit(1);
    }
}

/// Main application state holder.
#[derive(Default)]
struct App {
    /// Optional application state (initialized on resume).
    state: Option<AppState>,
}

impl App {
    /// Set the canvas background color from the content page.
    #[inline]
    fn set_canvas_background_from_content(state: &mut AppState) {
        if let Some(page) = state.pages.get_mut(1) {
            let clear_color = page.background_rgba();
            state.render_state.set_clear_color(clear_color);
        }
    }

    /// Tick all pages and redraw if any page needs it.
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

    /// Process a chrome host command.
    #[inline]
    fn process_chrome_cmd(state: &mut AppState, cmd: ChromeHostCommand) {
        match cmd {
            ChromeHostCommand::Navigate(url_str) => Self::handle_navigate(state, &url_str),
            ChromeHostCommand::Back
            | ChromeHostCommand::Forward
            | ChromeHostCommand::Reload
            | ChromeHostCommand::OpenTab(_)
            | ChromeHostCommand::CloseTab(_) => {
                info!("chromeHost command stub: {cmd:?}");
            }
        }
    }

    /// Push a layer with an optional display list.
    #[inline]
    fn push_layer_opt(state: &mut AppState, layer: &Layer, display_list_opt: Option<DisplayList>) {
        if let Some(display_list) = display_list_opt {
            match *layer {
                Layer::Content(_) => state.render_state.push_layer(Layer::Content(display_list)),
                Layer::Chrome(_) => state.render_state.push_layer(Layer::Chrome(display_list)),
                Layer::Background => { /* not used with dl here */ }
            }
        }
    }

    /// Push a layer with an optional background rectangle and display list.
    #[inline]
    fn push_layer_with_bg(
        render_state: &mut RenderState,
        layer: Layer,
        background_rect: Option<DisplayItem>,
        display_list: DisplayList,
    ) {
        match (background_rect, layer) {
            (Some(rect), Layer::Content(_)) => {
                let mut items = Vec::with_capacity(display_list.items.len() + 1);
                items.push(rect);
                items.extend(display_list.items);
                render_state.push_layer(Layer::Content(DisplayList::from_items(items)));
            }
            (Some(rect), Layer::Chrome(_)) => {
                let mut items = Vec::with_capacity(display_list.items.len() + 1);
                items.push(rect);
                items.extend(display_list.items);
                render_state.push_layer(Layer::Chrome(DisplayList::from_items(items)));
            }
            (None, Layer::Content(_)) => {
                render_state.push_layer(Layer::Content(display_list));
            }
            (None, Layer::Chrome(_)) => {
                render_state.push_layer(Layer::Chrome(display_list));
            }
            _ => {}
        }
    }

    /// Rebuild rendering layers after a page update.
    fn rebuild_layers_after_update(state: &mut AppState) {
        state.render_state.clear_layers();
        Self::set_canvas_background_from_content(state);
        let win_size = state.render_state.get_window().inner_size();
        let frame_width = win_size.width as f32;
        let frame_height = win_size.height as f32;
        let content_dl = state
            .pages
            .get_mut(1)
            .and_then(|page| page.display_list_retained_snapshot().ok());
        let chrome_dl = state
            .pages
            .get_mut(0)
            .and_then(|page| page.display_list_retained_snapshot().ok());
        if let Some(display_list) = content_dl {
            // Prepend full-viewport bg
            let clear_color = state
                .pages
                .get(1)
                .map_or([1.0, 1.0, 1.0, 1.0], HtmlPage::background_rgba);
            let background = DisplayItem::Rect {
                x: 0.0,
                y: 0.0,
                width: frame_width,
                height: frame_height,
                color: clear_color,
            };
            Self::push_layer_with_bg(
                &mut state.render_state,
                Layer::Content(DisplayList::new()),
                Some(background),
                display_list,
            );
        }
        if let Some(display_list) = chrome_dl {
            // Prepend chrome strip bg
            let clear_color = state
                .pages
                .first()
                .map_or([1.0, 1.0, 1.0, 1.0], HtmlPage::background_rgba);
            let background = DisplayItem::Rect {
                x: 0.0,
                y: 0.0,
                width: frame_width,
                height: state.chrome_bar_height_px as f32,
                color: clear_color,
            };
            Self::push_layer_with_bg(
                &mut state.render_state,
                Layer::Chrome(DisplayList::new()),
                Some(background),
                display_list,
            );
        }
        state.render_state.get_window().request_redraw();
    }

    /// Handle navigation to a new URL.
    fn handle_navigate(state: &mut AppState, url_str: &str) {
        let parsed = Url::parse(url_str).or_else(|_err| Url::parse(&format!("https://{url_str}")));
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
            Err(err) => error!("Navigate failed to create page: {err:?}"),
        }
    }
    /// Resume the application by creating the window and initializing state.
    ///
    /// # Errors
    /// Returns an error if window creation, runtime initialization, or page creation fails.
    fn resume(&mut self, event_loop: &ActiveEventLoop) -> Result<(), Error> {
        // Create window object
        let window = Arc::new(event_loop.create_window(Window::default_attributes())?);

        let runtime = Runtime::new()?;

        // Create renderer
        let render_state = runtime.block_on(RenderState::new(Arc::clone(&window)));

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

    /// Handle cursor movement events.
    fn handle_cursor_moved(&mut self, x: f64, y: f64) {
        let Some(cursor_state) = self.state.as_mut() else {
            return;
        };
        cursor_state.last_cursor_pos = Some((x, y));
        let target = usize::from(y >= cursor_state.chrome_bar_height_px);
        cursor_state.pointer_target_index = Some(target);
        if let Some(page) = cursor_state.pages.get_mut(target) {
            page.dispatch_pointer_move(x, y);
            if let Err(err) = cursor_state.runtime.block_on(page.update()) {
                error!("Failed to apply page updates: {err:?}");
            }
            if page.take_needs_redraw() {
                Self::rebuild_layers_after_update(cursor_state);
            }
        }
    }

    /// Handle mouse button input events.
    fn handle_mouse_input(&mut self, button_state: ElementState, button: MouseButton) {
        let Some(state_ref) = self.state.as_mut() else {
            return;
        };
        let target = state_ref
            .pointer_target_index
            .unwrap_or(state_ref.focused_page_index);
        let (x, y) = state_ref.last_cursor_pos.unwrap_or((0.0f64, 0.0f64));
        if let Some(page) = state_ref.pages.get_mut(target) {
            let button_code = match button {
                MouseButton::Right => 2u32,
                MouseButton::Middle => 1u32,
                MouseButton::Left
                | MouseButton::Back
                | MouseButton::Forward
                | MouseButton::Other(_) => 0u32,
            };
            match button_state {
                ElementState::Pressed => {
                    state_ref.focused_page_index = target;
                    page.dispatch_pointer_down(x, y, button_code);
                }
                ElementState::Released => {
                    page.dispatch_pointer_up(x, y, button_code);
                }
            }
            if let Err(err) = state_ref.runtime.block_on(page.update()) {
                error!("Failed to apply page updates: {err:?}");
            }
            if page.take_needs_redraw() {
                Self::rebuild_layers_after_update(state_ref);
            }
        }
    }

    /// Handle keyboard input events.
    fn handle_keyboard_input(&mut self, key_event: &KeyEvent) {
        let Some(state_ref) = self.state.as_mut() else {
            return;
        };
        let idx = state_ref
            .focused_page_index
            .min(state_ref.pages.len().saturating_sub(1));
        let key_str: String = format!("{:?}", key_event.logical_key);
        let code_str: String = format!("{:?}", key_event.physical_key);
        let is_down = key_event.state == ElementState::Pressed;
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
            if let Err(err) = state_ref.runtime.block_on(page.update()) {
                error!("Failed to apply page updates: {err:?}");
            }
            if page.take_needs_redraw() {
                Self::rebuild_simple_layers(state_ref);
            }
        }
    }

    /// Rebuild layers with simple approach (used after keyboard input).
    fn rebuild_simple_layers(state: &mut AppState) {
        state.render_state.clear_layers();
        let content_dl = state
            .pages
            .get_mut(1)
            .and_then(|page_ref| page_ref.display_list_retained_snapshot().ok());
        let chrome_dl = state
            .pages
            .get_mut(0)
            .and_then(|page_ref| page_ref.display_list_retained_snapshot().ok());
        Self::push_layer_opt(state, &Layer::Content(DisplayList::new()), content_dl);
        Self::push_layer_opt(state, &Layer::Chrome(DisplayList::new()), chrome_dl);
        info!("App: requesting redraw");
        state.render_state.get_window().request_redraw();
    }

    /// Handle window events for the application.
    ///
    /// # Errors
    /// Returns an error if event handling fails.
    fn handle_window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        window_event: WindowEvent,
    ) -> Result<(), Error> {
        let state = self
            .state
            .as_mut()
            .ok_or_else(|| anyhow!("App state is not set."))?;
        match window_event {
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
                self.handle_cursor_moved(position.x, position.y);
            }
            WindowEvent::MouseInput {
                state: button_state,
                button,
                ..
            } => {
                self.handle_mouse_input(button_state, button);
            }
            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                self.handle_keyboard_input(&key_event);
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

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if let Err(error) = self.handle_window_event(event_loop, window_id, event) {
            error!("Failed to handle event: {error}");
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = self.state.as_mut() {
            // Drain any pending chromeHost commands
            while let Ok(cmd) = state.chrome_host_rx.try_recv() {
                Self::process_chrome_cmd(state, cmd);
            }

            // During initial loading or navigation, tick pages; otherwise idle.
            if state.pages.iter().any(|page| !page.parsing_finished()) {
                Self::tick_pages_and_maybe_redraw(state);
            }
        }
    }
}
