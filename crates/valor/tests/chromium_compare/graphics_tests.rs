use anyhow::{Result, anyhow};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use chromiumoxide::cdp::browser_protocol::page::{
    CaptureScreenshotFormat, CaptureScreenshotParams,
};
use chromiumoxide::page::Page;
use image::{RgbaImage, load_from_memory};
use pollster::block_on;
use renderer::{DisplayItem, DisplayList};
use std::fs::{create_dir_all, remove_dir_all};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use tokio::runtime::Handle;
use wgpu_backend::RenderState;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};
use zstd::bulk::{compress as zstd_compress, decompress as zstd_decompress};

use super::browser::navigate_and_prepare_page;
use super::common::{
    CacheFetcher, read_or_fetch_cache, setup_page_for_fixture, test_cache_dir,
    write_png_rgba_if_changed,
};

fn safe_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("fixture")
        .to_string()
}

/// Check if a pixel coordinate is inside any glyph bounding box.
fn is_pixel_in_text_region(x: u32, y: u32, glyph_bounds: &[wgpu_backend::GlyphBounds]) -> bool {
    let x_f = x as f32;
    let y_f = y as f32;
    glyph_bounds.iter().any(|glyph| {
        x_f >= glyph.left && x_f < glyph.right && y_f >= glyph.top && y_f < glyph.bottom
    })
}

/// Compare two pixels with per-channel and per-pixel thresholds.
/// For text regions: max 50% per-channel diff, max 20% total pixel diff.
/// For non-text: exact match required.
/// Returns true if pixel is acceptable (within thresholds).
fn compare_pixel_with_thresholds(
    pixel_a: &[u8],
    pixel_b: &[u8],
    idx: usize,
    is_text: bool,
) -> bool {
    if idx + 3 >= pixel_a.len() || idx + 3 >= pixel_b.len() {
        return true; // Out of bounds, skip
    }

    if !is_text {
        // Non-text regions must match exactly
        return pixel_a[idx..idx + 4] == pixel_b[idx..idx + 4];
    }

    // Text region: apply relaxed thresholds
    // Per-channel threshold: 50% (128 out of 255)
    const CHANNEL_THRESHOLD: u16 = 128;

    // Check each RGB channel (skip alpha at idx+3)
    let mut total_diff = 0u32;
    for channel_offset in 0..3 {
        let a = pixel_a[idx + channel_offset];
        let b = pixel_b[idx + channel_offset];
        let diff = (i16::from(a) - i16::from(b)).unsigned_abs();

        // Check per-channel threshold
        if diff > CHANNEL_THRESHOLD {
            return false;
        }

        total_diff += u32::from(diff);
    }

    // Per-pixel threshold: 20% of max possible diff (255 * 3 = 765)
    // 20% of 765 = 153
    const PIXEL_THRESHOLD: u32 = 153;
    total_diff <= PIXEL_THRESHOLD
}

/// Count pixels that fail comparison thresholds.
fn count_pixel_failures(
    pixels_a: &[u8],
    pixels_b: &[u8],
    width: u32,
    height: u32,
    glyph_bounds: &[wgpu_backend::GlyphBounds],
) -> u64 {
    let mut failures = 0u64;
    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * 4) as usize;
            let is_text = is_pixel_in_text_region(x, y, glyph_bounds);
            if !compare_pixel_with_thresholds(pixels_a, pixels_b, idx, is_text) {
                failures += 1;
            }
        }
    }
    failures
}

/// Create a diff image highlighting failed pixels (red = fail, black = pass).
fn make_diff_image(
    pixels_a: &[u8],
    pixels_b: &[u8],
    width: u32,
    height: u32,
    glyph_bounds: &[wgpu_backend::GlyphBounds],
) -> Vec<u8> {
    let mut out = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * 4) as usize;
            let is_text = is_pixel_in_text_region(x, y, glyph_bounds);
            let matches = compare_pixel_with_thresholds(pixels_a, pixels_b, idx, is_text);

            if idx + 3 < out.len() {
                // Red for failures, black for passes
                out[idx] = if matches { 0 } else { 255 };
                out[idx + 1] = 0;
                out[idx + 2] = 0;
                out[idx + 3] = 255;
            }
        }
    }
    out
}

/// Captures a PNG screenshot from Chromium for a given fixture.
///
/// # Errors
///
/// Returns an error if navigation or screenshot capture fails.
async fn capture_chrome_png(page: &Page, path: &Path) -> Result<Vec<u8>> {
    use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;

    navigate_and_prepare_page(page, path).await?;

    // Set viewport to exact size we want for screenshots
    let viewport_params = SetDeviceMetricsOverrideParams::builder()
        .width(784)
        .height(453)
        .device_scale_factor(1.0)
        .mobile(false)
        .build()
        .map_err(|err| anyhow!("Failed to build viewport params: {err}"))?;
    page.execute(viewport_params).await?;

    let params = CaptureScreenshotParams::builder()
        .format(CaptureScreenshotFormat::Png)
        .from_surface(true)
        .build();
    let response = page.execute(params).await?;
    let base64_str: &str = response.data.as_ref();
    let bytes = BASE64_STANDARD
        .decode(base64_str)
        .map_err(|err| anyhow!("Failed to decode base64 screenshot: {err}"))?;
    Ok(bytes)
}

/// Builds a Valor display list for a given fixture.
///
/// # Errors
///
/// Returns an error if page creation, parsing, or display list generation fails.
async fn build_valor_display_list_for(
    path: &Path,
    viewport_w: u32,
    viewport_h: u32,
) -> Result<DisplayList> {
    let handle = Handle::current();
    let mut page = setup_page_for_fixture(&handle, path).await?;
    let display_list = page.display_list_retained_snapshot()?;
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

fn initialize_render_state(width: u32, height: u32) -> &'static Mutex<RenderState> {
    use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows as _};

    RENDER_STATE.get_or_init(|| {
        let event_loop = EventLoop::builder()
            .with_any_thread(true)
            .build()
            .unwrap_or_else(|err| {
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

        // Use pollster to block on the async RenderState::new without requiring a tokio runtime
        let state = block_on(RenderState::new(window)).unwrap_or_else(|err| {
            log::error!("Failed to create render state: {err}");
            process::abort();
        });
        Mutex::new(state)
    })
}

/// Rasterizes a display list to RGBA bytes using the GPU backend.
/// Also returns glyph bounds for text region masking.
///
/// # Errors
///
/// Returns an error if render state locking or rendering fails.
fn rasterize_display_list_to_rgba(
    display_list: &DisplayList,
    width: u32,
    height: u32,
) -> Result<(Vec<u8>, Vec<wgpu_backend::GlyphBounds>)> {
    let state_mutex = initialize_render_state(width, height);

    let mut state = state_mutex
        .lock()
        .map_err(|err| anyhow!("Failed to lock render state: {err}"))?;
    let _render_num = RENDER_COUNTER.fetch_add(1, Ordering::SeqCst);

    state.reset_for_next_frame();
    state.resize(PhysicalSize::new(width, height));
    state.set_retained_display_list(display_list.clone());

    let rgba = state.render_to_rgba()?;
    let glyph_bounds = state.glyph_bounds().to_vec();
    Ok((rgba, glyph_bounds))
}

/// Loads Chrome RGBA image data from cache or by capturing a screenshot.
///
/// # Errors
///
/// Returns an error if browser initialization, screenshot capture, or image decoding fails.
async fn load_chrome_rgba(fixture: &Path, page: &Page, _harness_src: &str) -> Result<RgbaImage> {
    read_or_fetch_cache(CacheFetcher {
        test_name: "graphics",
        fixture_path: fixture,
        cache_suffix: "_chrome.rgba.zst",
        fetch_fn: || async {
            let png_bytes = capture_chrome_png(page, fixture).await?;
            let img = load_from_memory(&png_bytes)?.to_rgba8();
            Ok(img)
        },
        deserialize_fn: |bytes| {
            let expected = (784u32 * 453u32 * 4) as usize;
            let decompressed = zstd_decompress(bytes, expected)?;
            if decompressed.len() == expected {
                RgbaImage::from_raw(784, 453, decompressed)
                    .ok_or_else(|| anyhow!("Failed to create RgbaImage from decompressed bytes"))
            } else {
                Err(anyhow!(
                    "Cached image has wrong size ({})",
                    decompressed.len()
                ))
            }
        },
        serialize_fn: |img| {
            let compressed = zstd_compress(img.as_raw(), 1)?;
            Ok(compressed)
        },
    })
    .await
}

/// Sets up test directories for graphics artifacts with optional clear.
///
/// # Errors
///
/// Returns an error if directory creation or removal fails.
fn setup_test_dirs_with_clear(clear_failing: bool) -> Result<(PathBuf, PathBuf)> {
    let out_dir = test_cache_dir("graphics")?;
    let failing_dir = out_dir.join("failing");
    if clear_failing && failing_dir.exists() {
        remove_dir_all(&failing_dir)?;
    }
    create_dir_all(&failing_dir)?;
    Ok((out_dir, failing_dir))
}

struct FixtureContext<'ctx> {
    fixture: &'ctx Path,
    failing_dir: &'ctx Path,
    page: &'ctx Page,
    harness_src: &'ctx str,
}

struct FailureArtifacts<'artifacts> {
    ctx: &'artifacts FixtureContext<'artifacts>,
    name: String,
    chrome_img: RgbaImage,
    valor_img: Vec<u8>,
    diff_img: Vec<u8>,
    width: u32,
    height: u32,
    over: u64,
    total: u64,
    diff_ratio: f64,
}

impl FailureArtifacts<'_> {
    /// Writes failure artifacts to disk.
    ///
    /// # Errors
    ///
    /// Returns an error if file operations fail.
    fn write_to_disk(&self) -> Result<()> {
        let base = self.ctx.failing_dir.join(&self.name);
        let chrome_path = base.with_extension("chrome.png");
        let valor_path = base.with_extension("valor.png");
        let diff_path = base.with_extension("diff.png");
        create_dir_all(self.ctx.failing_dir)?;
        write_png_rgba_if_changed(
            &chrome_path,
            self.chrome_img.as_raw(),
            self.width,
            self.height,
        )?;
        write_png_rgba_if_changed(&valor_path, &self.valor_img, self.width, self.height)?;
        write_png_rgba_if_changed(&diff_path, &self.diff_img, self.width, self.height)?;

        // No error logging here - will be shown in summary
        Ok(())
    }
}

/// Processes a single graphics fixture by comparing Chrome and Valor renders.
///
/// # Errors
///
/// Returns an error if fixture processing, rendering, or comparison fails.
async fn process_single_fixture(ctx: &FixtureContext<'_>) -> Result<bool> {
    let name = safe_stem(ctx.fixture);

    let chrome_start = Instant::now();
    let chrome_img = load_chrome_rgba(ctx.fixture, ctx.page, ctx.harness_src).await?;
    let chrome_time = chrome_start.elapsed();

    let (width, height) = (784u32, 453u32);

    let valor_build_start = Instant::now();
    let display_list = build_valor_display_list_for(ctx.fixture, width, height).await?;
    let valor_build_time = valor_build_start.elapsed();

    let valor_render_start = Instant::now();
    let (valor_img, glyph_bounds) = rasterize_display_list_to_rgba(&display_list, width, height)?;
    let valor_render_time = valor_render_start.elapsed();

    let _compare_start = Instant::now();
    if chrome_img.as_raw() == &valor_img {
        let _ = (chrome_time, valor_build_time, valor_render_time);
        return Ok(false);
    }

    // Use new per-pixel per-channel comparison with text region awareness
    let failures = count_pixel_failures(
        chrome_img.as_raw(),
        &valor_img,
        width,
        height,
        &glyph_bounds,
    );

    if failures > 0 {
        let diff_img = make_diff_image(
            chrome_img.as_raw(),
            &valor_img,
            width,
            height,
            &glyph_bounds,
        );
        let total_pixels = u64::from(width) * u64::from(height);
        let artifacts = FailureArtifacts {
            ctx,
            name: name.clone(),
            chrome_img,
            valor_img,
            diff_img,
            width,
            height,
            over: failures,
            total: total_pixels,
            diff_ratio: (failures as f64) / (total_pixels as f64),
        };
        artifacts.write_to_disk()?;
        let _ = (chrome_time, valor_build_time, valor_render_time);
        return Ok(true);
    }

    let _ = (chrome_time, valor_build_time, valor_render_time);
    Ok(false)
}

/// Runs a single graphics test for a given fixture with a provided page.
///
/// # Errors
///
/// Returns an error if rendering or comparison fails.
pub async fn run_single_graphics_test_with_page(fixture_path: &Path, page: &Page) -> Result<()> {
    let harness_src = concat!(
        include_str!("graphics_tests.rs"),
        include_str!("common.rs"),
        include_str!("browser.rs"),
    );
    // Don't clear failing dir for individual tests - accumulate failures
    let (_out_dir, failing_dir) = setup_test_dirs_with_clear(false)?;

    let result = process_single_fixture(&FixtureContext {
        fixture: fixture_path,
        failing_dir: &failing_dir,
        page,
        harness_src,
    })
    .await;

    match result {
        Ok(true) => Err(anyhow!(
            "Graphics comparison failed - pixel differences found"
        )),
        Ok(false) => Ok(()),
        Err(err) => Err(err),
    }
}
