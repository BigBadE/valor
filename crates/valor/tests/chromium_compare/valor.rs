use anyhow::{Result, anyhow};
use pollster::block_on;
use renderer::{DisplayItem, DisplayList};
use std::env::set_var;
use std::path::Path;
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::runtime::Handle;
use wgpu_backend::{GlyphBounds, RenderState};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use super::common::setup_page_for_fixture;

static RENDER_COUNTER: AtomicUsize = AtomicUsize::new(0);
static RENDER_STATE: OnceLock<Mutex<RenderState>> = OnceLock::new();
static WINDOW: OnceLock<Arc<Window>> = OnceLock::new();

struct WindowCreator {
    window: Option<Window>,
    width: u32,
    height: u32,
}

impl WindowCreator {
    const fn new(width: u32, height: u32) -> Self {
        Self {
            window: None,
            width,
            height,
        }
    }

    /// Creates a window if one hasn't been created yet.
    ///
    /// # Errors
    ///
    /// Returns an error if window creation fails.
    fn create_window_if_needed(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        if self.window.is_some() {
            return Ok(());
        }
        let window = event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("Valor Test")
                    .with_inner_size(LogicalSize::new(self.width, self.height))
                    .with_visible(false),
            )
            .map_err(|err| anyhow!("Failed to create window: {err}"))?;
        self.window = Some(window);
        Ok(())
    }

    /// Consumes the creator and returns the created window.
    ///
    /// # Errors
    ///
    /// Returns an error if no window was created.
    fn into_window(self) -> Result<Window> {
        self.window.ok_or_else(|| anyhow!("Window not created"))
    }
}

impl ApplicationHandler for WindowCreator {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let _ignore_result = self.create_window_if_needed(event_loop);
        event_loop.exit();
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
    }
}

/// Initializes the render state singleton with the given dimensions.
fn initialize_render_state(width: u32, height: u32) -> &'static Mutex<RenderState> {
    use winit::event_loop::EventLoop;

    #[cfg(target_os = "macos")]
    use winit::platform::macos::EventLoopBuilderExtMacOS as _;
    #[cfg(target_os = "windows")]
    use winit::platform::windows::EventLoopBuilderExtWindows as _;
    #[cfg(all(unix, not(target_os = "macos")))]
    use winit::platform::x11::EventLoopBuilderExtX11 as _;

    RENDER_STATE.get_or_init(|| {
        let mut builder = EventLoop::builder();

        // Allow running event loop on any thread for tests
        #[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
        {
            let _ignore = builder.with_any_thread(true);
        }

        let event_loop = builder.build().unwrap_or_else(|err| {
            log::error!("Failed to create event loop: {err}");
            process::abort();
        });

        let window = {
            let mut app = WindowCreator::new(width, height);
            event_loop.run_app(&mut app).unwrap_or_else(|err| {
                log::error!("Failed to run event loop: {err}");
                process::abort();
            });
            app.into_window().unwrap_or_else(|err| {
                log::error!("{err}");
                process::abort();
            })
        };

        let window = Arc::new(window);
        let _ignore_result = WINDOW.set(Arc::clone(&window));

        let state = block_on(RenderState::new(window)).unwrap_or_else(|err| {
            log::error!("Failed to create render state: {err}");
            process::abort();
        });
        Mutex::new(state)
    })
}

/// Builds BOTH layout JSON and display list from a single HtmlPage to avoid
/// creating two V8 isolates per fixture.
///
/// # Errors
///
/// Returns an error if page creation, parsing, or data extraction fails.
pub async fn build_layout_and_graphics_for_fixture(
    path: &Path,
    viewport_w: u32,
    viewport_h: u32,
) -> Result<(serde_json::Value, DisplayList)> {
    use std::time::Instant;
    let start = Instant::now();

    // Set viewport dimensions
    unsafe {
        set_var("VALOR_VIEWPORT_WIDTH", viewport_w.to_string());
        set_var("VALOR_VIEWPORT_HEIGHT", viewport_h.to_string());
    }

    let handle = Handle::current();
    eprintln!("[VALOR_TIMING] setup_page_for_fixture START");
    let mut page = setup_page_for_fixture(&handle, path).await?;
    eprintln!(
        "[VALOR_TIMING] setup_page_for_fixture took: {:?}",
        start.elapsed()
    );

    // setup_page_for_fixture already injected CSS reset and ran layout
    // No need for additional update() calls

    // Extract layout JSON first
    eprintln!("[VALOR_TIMING] serialize_layout START");
    let layout_json = super::layout_tests::serialization::serialize_valor_layout(&mut page)?;
    eprintln!(
        "[VALOR_TIMING] serialize_layout took: {:?}",
        start.elapsed()
    );

    // Then extract display list for graphics
    eprintln!("[VALOR_TIMING] display_list_retained_snapshot START");
    let display_list = page.display_list_retained_snapshot();
    eprintln!(
        "[VALOR_TIMING] display_list_retained_snapshot took: {:?}",
        start.elapsed()
    );

    let clear_color = page.background_rgba();
    let mut items = Vec::with_capacity(display_list.items.len() + 1);
    items.push(DisplayItem::Rect {
        x: 0.0,
        y: 0.0,
        width: viewport_w as f32,
        height: viewport_h as f32,
        color: clear_color,
    });
    items.extend(display_list.items);

    let final_display_list = DisplayList::from_items(items);
    eprintln!(
        "[VALOR_TIMING] build_layout_and_graphics_for_fixture TOTAL: {:?}",
        start.elapsed()
    );

    Ok((layout_json, final_display_list))
}

/// Builds a Valor display list for a given fixture.
///
/// # Errors
///
/// Returns an error if page creation, parsing, or display list generation fails.
pub async fn build_display_list_for_fixture(
    path: &Path,
    viewport_w: u32,
    viewport_h: u32,
) -> Result<DisplayList> {
    use std::time::Instant;
    let start = Instant::now();

    // Set viewport dimensions via environment for the page
    // Safety: This is a test environment where we control the execution
    unsafe {
        set_var("VALOR_VIEWPORT_WIDTH", viewport_w.to_string());
        set_var("VALOR_VIEWPORT_HEIGHT", viewport_h.to_string());
    }

    let handle = Handle::current();
    eprintln!("[VALOR_TIMING] setup_page_for_fixture START");
    let mut page = setup_page_for_fixture(&handle, path).await?;
    eprintln!(
        "[VALOR_TIMING] setup_page_for_fixture took: {:?}",
        start.elapsed()
    );

    // setup_page_for_fixture already injected CSS reset and ran layout
    // No need for additional update() calls

    eprintln!("[VALOR_TIMING] display_list_retained_snapshot START");
    let display_list = page.display_list_retained_snapshot();
    eprintln!(
        "[VALOR_TIMING] display_list_retained_snapshot took: {:?}",
        start.elapsed()
    );
    let clear_color = page.background_rgba();
    let mut items = Vec::with_capacity(display_list.items.len() + 1);
    items.push(DisplayItem::Rect {
        x: 0.0,
        y: 0.0,
        width: viewport_w as f32,
        height: viewport_h as f32,
        color: clear_color,
    });

    items.extend(display_list.items);
    Ok(DisplayList::from_items(items))
}

type RasterizeResult = (Vec<u8>, Vec<GlyphBounds>);

/// Rasterizes a display list to RGBA bytes using the GPU backend.
/// Also returns glyph bounds for text region masking.
///
/// # Errors
///
/// Returns an error if render state locking or rendering fails.
pub fn rasterize_display_list_to_rgba(
    display_list: &DisplayList,
    width: u32,
    height: u32,
) -> Result<RasterizeResult> {
    use std::time::Instant;
    let start = Instant::now();

    eprintln!("[VALOR_TIMING] initialize_render_state START");
    let state_mutex = initialize_render_state(width, height);
    eprintln!(
        "[VALOR_TIMING] initialize_render_state took: {:?}",
        start.elapsed()
    );

    eprintln!("[VALOR_TIMING] lock render state START");
    let mut state = state_mutex
        .lock()
        .map_err(|err| anyhow!("Failed to lock render state: {err}"))?;
    eprintln!(
        "[VALOR_TIMING] lock render state took: {:?}",
        start.elapsed()
    );

    let _render_num = RENDER_COUNTER.fetch_add(1, Ordering::SeqCst);

    eprintln!("[VALOR_TIMING] reset_for_next_frame START");
    state.reset_for_next_frame();
    eprintln!(
        "[VALOR_TIMING] reset_for_next_frame took: {:?}",
        start.elapsed()
    );

    eprintln!("[VALOR_TIMING] resize START");
    state.resize(PhysicalSize::new(width, height));
    eprintln!("[VALOR_TIMING] resize took: {:?}", start.elapsed());

    eprintln!("[VALOR_TIMING] set_retained_display_list START");
    state.set_retained_display_list(display_list.clone());
    eprintln!(
        "[VALOR_TIMING] set_retained_display_list took: {:?}",
        start.elapsed()
    );

    eprintln!("[VALOR_TIMING] render_to_rgba START");
    let rgba = state.render_to_rgba()?;
    eprintln!("[VALOR_TIMING] render_to_rgba took: {:?}", start.elapsed());

    let glyph_bounds = state.glyph_bounds().to_vec();
    eprintln!(
        "[VALOR_TIMING] rasterize_display_list_to_rgba TOTAL: {:?}",
        start.elapsed()
    );
    Ok((rgba, glyph_bounds))
}
