use anyhow::{Result, anyhow};
use chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat;
use chromiumoxide::page::{Page, ScreenshotParams};
use image::{RgbaImage, load_from_memory};
use log::{debug, error, info};
use renderer::{DisplayItem, DisplayList, batch_display_list};
use std::fs::{create_dir_all, read, read_dir, remove_dir_all, remove_file, write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::runtime::Runtime;
use wgpu_backend::RenderState;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};
use zstd::bulk::{compress as zstd_compress, decompress as zstd_decompress};

use super::browser::{ChromeBrowser, TestType, navigate_and_prepare_page, setup_chrome_browser};
use super::common::{
    artifacts_subdir, get_filtered_fixtures, init_test_logger, setup_page_for_fixture,
    write_png_rgba_if_changed,
};

fn safe_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("fixture")
        .to_string()
}

fn now_millis() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("{millis}")
}

const fn fnv1a64_bytes(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut index = 0;
    while index < bytes.len() {
        hash ^= bytes[index] as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
        index += 1;
    }
    hash
}

fn file_content_hash(path: &Path) -> u64 {
    read(path).map_or(0, |bytes| fnv1a64_bytes(&bytes))
}

fn should_remove_out_dir_artifact(
    fname: &str,
    name: &str,
    path_hash_hex: &str,
    hash_hex: &str,
) -> bool {
    let prefix = format!("{name}_{path_hash_hex}_");
    let is_this_fixture = fname.starts_with(&prefix);
    if !is_this_fixture {
        return false;
    }

    let is_stable_chrome = fname.ends_with("_chrome.png") || fname.ends_with("_chrome.rgba.zst");
    if !is_stable_chrome {
        return false;
    }

    let is_current_hash = fname.contains(&format!("_{hash_hex}_"))
        || fname.ends_with(&format!("_{hash_hex}_chrome.png"))
        || fname.ends_with(&format!("_{hash_hex}_chrome.rgba.zst"));

    !is_current_hash
}

fn should_remove_failing_dir_artifact(
    fname: &str,
    name: &str,
    path_hash_hex: &str,
    hash_hex: &str,
) -> bool {
    let prefix = format!("{name}_{path_hash_hex}_");
    let is_this_fixture = fname.starts_with(&prefix);
    if !is_this_fixture {
        return false;
    }

    let is_current_hash = fname.contains(&format!("_{hash_hex}_"));
    !is_current_hash
}

/// Cleans up old artifact files for a given hash.
fn cleanup_artifacts_for_hash(
    name: &str,
    path_hash_hex: &str,
    out_dir: &Path,
    failing_dir: &Path,
    hash_hex: &str,
) {
    if let Ok(entries) = read_dir(out_dir) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            let Some(fname) = entry_path.file_name().and_then(|os_name| os_name.to_str()) else {
                continue;
            };
            if should_remove_out_dir_artifact(fname, name, path_hash_hex, hash_hex) {
                let _ignore_error = remove_file(entry_path);
            }
        }
    }
    if let Ok(entries) = read_dir(failing_dir) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            let Some(fname) = entry_path.file_name().and_then(|os_name| os_name.to_str()) else {
                continue;
            };
            if should_remove_failing_dir_artifact(fname, name, path_hash_hex, hash_hex) {
                let _ignore_error = remove_file(entry_path);
            }
        }
    }
}

type TextMask = (u32, u32, u32, u32);

struct DiffCtx<'diff> {
    width: u32,
    height: u32,
    eps: u8,
    masks: &'diff [TextMask],
}

fn is_pixel_masked(x_coord: u32, y_coord: u32, masks: &[TextMask]) -> bool {
    masks.iter().any(|&(left, top, right, bottom)| {
        x_coord >= left && x_coord < right && y_coord >= top && y_coord < bottom
    })
}

fn count_channel_diffs(pixel_a: &[u8], pixel_b: &[u8], idx: usize, eps: u8) -> (u64, u64) {
    let mut total = 0;
    let mut over = 0;
    for channel in 0..4 {
        let diff_ab = i16::from(pixel_a[idx + channel]) - i16::from(pixel_b[idx + channel]);
        let abs_diff = diff_ab.unsigned_abs() as u8;
        total += 1;
        if abs_diff > eps {
            over += 1;
        }
    }
    (over, total)
}

fn per_pixel_diff_masked(pixels_a: &[u8], pixels_b: &[u8], ctx: &DiffCtx<'_>) -> (u64, u64) {
    let mut total: u64 = 0;
    let mut over: u64 = 0;
    for y_coord in 0..ctx.height {
        for x_coord in 0..ctx.width {
            if is_pixel_masked(x_coord, y_coord, ctx.masks) {
                continue;
            }
            let idx = ((y_coord * ctx.width + x_coord) * 4) as usize;
            if idx + 3 >= pixels_a.len() || idx + 3 >= pixels_b.len() {
                continue;
            }
            let (ch_over, ch_total) = count_channel_diffs(pixels_a, pixels_b, idx, ctx.eps);
            over += ch_over;
            total += ch_total;
        }
    }
    (over, total)
}

fn compute_max_channel_diff(pixel_a: &[u8], pixel_b: &[u8], idx: usize) -> u8 {
    let mut max_diff = 0u8;
    for channel in 0..3 {
        let diff = i16::from(pixel_a[idx + channel]) - i16::from(pixel_b[idx + channel]);
        let abs_diff = diff.unsigned_abs() as u8;
        if abs_diff > max_diff {
            max_diff = abs_diff;
        }
    }
    max_diff
}

fn make_diff_image_masked(pixels_a: &[u8], pixels_b: &[u8], ctx: &DiffCtx<'_>) -> Vec<u8> {
    let mut out = vec![0u8; (ctx.width * ctx.height * 4) as usize];
    for y_coord in 0..ctx.height {
        for x_coord in 0..ctx.width {
            let idx = ((y_coord * ctx.width + x_coord) * 4) as usize;
            if is_pixel_masked(x_coord, y_coord, ctx.masks) {
                continue;
            }
            if idx + 3 >= pixels_a.len() || idx + 3 >= pixels_b.len() || idx + 3 >= out.len() {
                continue;
            }
            let max_channel_diff = compute_max_channel_diff(pixels_a, pixels_b, idx);
            let val = if max_channel_diff > ctx.eps { 255 } else { 0 };
            out[idx] = val;
            out[idx + 1] = 0;
            out[idx + 2] = 0;
            out[idx + 3] = 255;
        }
    }
    out
}

fn extract_text_masks(display_list: &DisplayList, width: u32, height: u32) -> Vec<TextMask> {
    let mut masks = Vec::new();
    for item in &display_list.items {
        if let DisplayItem::Text {
            bounds: Some((left, top, right, bottom)),
            ..
        } = item
        {
            let left = (*left).max(0) as u32;
            let top = (*top).max(0) as u32;
            let right = (*right).max(0) as u32;
            let bottom = (*bottom).max(0) as u32;
            let left = left.min(width);
            let top = top.min(height);
            let right = right.min(width);
            let bottom = bottom.min(height);
            if right > left && bottom > top {
                masks.push((left, top, right, bottom));
            }
        }
    }
    masks
}

/// Captures a PNG screenshot from Chromium for a given fixture.
///
/// # Errors
///
/// Returns an error if navigation or screenshot capture fails.
async fn capture_chrome_png(page: &Page, path: &Path) -> Result<Vec<u8>> {
    navigate_and_prepare_page(page, path).await?;
    let params = ScreenshotParams::builder()
        .format(CaptureScreenshotFormat::Png)
        .full_page(true)
        .build();
    let screenshot = page.screenshot(params).await?;
    Ok(screenshot)
}

/// Builds a Valor display list for a given fixture.
///
/// # Errors
///
/// Returns an error if page creation, parsing, or display list generation fails.
async fn build_valor_display_list_for(
    handle: &tokio::runtime::Handle,
    path: &Path,
    viewport_w: u32,
    viewport_h: u32,
) -> Result<DisplayList> {
    let mut page = setup_page_for_fixture(handle, path).await?;
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
    use winit::event_loop::EventLoop;
    #[cfg(target_os = "macos")]
    use winit::platform::macos::EventLoopBuilderExtMacOS as _;
    #[cfg(target_os = "windows")]
    use winit::platform::windows::EventLoopBuilderExtWindows as _;
    #[cfg(target_os = "linux")]
    use winit::platform::x11::EventLoopBuilderExtX11 as _;

    RENDER_STATE.get_or_init(|| {
        let runtime = Runtime::new().unwrap_or_else(|err| {
            log::error!("Failed to create tokio runtime: {err}");
            process::abort();
        });

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
        let state = runtime
            .block_on(RenderState::new(window))
            .unwrap_or_else(|err| {
                log::error!("Failed to create render state: {err}");
                process::abort();
            });
        Mutex::new(state)
    })
}

/// Rasterizes a display list to RGBA bytes using the GPU backend.
///
/// # Errors
///
/// Returns an error if render state locking or rendering fails.
fn rasterize_display_list_to_rgba(
    display_list: &DisplayList,
    width: u32,
    height: u32,
) -> Result<Vec<u8>> {
    let state_mutex = initialize_render_state(width, height);

    let mut state = state_mutex
        .lock()
        .map_err(|err| anyhow!("Failed to lock render state: {err}"))?;
    let render_num = RENDER_COUNTER.fetch_add(1, Ordering::SeqCst);

    info!(
        "=== Rendering fixture #{} with {} items ===",
        render_num + 1,
        display_list.items.len()
    );

    if render_num >= 25 {
        info!("Display list #{} contents:", render_num + 1);
        for (idx, item) in display_list.items.iter().enumerate() {
            info!("  Item {idx}: {item:?}");
        }
    }

    state.reset_for_next_frame();
    state.resize(PhysicalSize::new(width, height));
    state.set_retained_display_list(display_list.clone());

    let result = state.render_to_rgba();

    if let Err(render_err) = &result {
        error!("=== RENDER FAILED for fixture #{} ===", render_num + 1);
        error!("Error: {render_err:?}");
        error!("Display list had {} items", display_list.items.len());
        error!("Display list items:");
        for (idx, item) in display_list.items.iter().enumerate() {
            error!("  Item {idx}: {item:?}");
        }
    } else {
        info!("=== Fixture #{} rendered successfully ===", render_num + 1);
    }

    result
}

/// Initializes a headless Chrome browser with a page for graphics testing.
///
/// # Errors
///
/// Returns an error if browser launch or page creation fails.
async fn init_browser() -> Result<(ChromeBrowser, Page)> {
    let chrome_browser = setup_chrome_browser(TestType::Graphics).await?;
    let chrome_page = chrome_browser.new_page().await?;
    Ok((chrome_browser, chrome_page))
}

/// Loads Chrome RGBA image data from cache or by capturing a screenshot.
///
/// # Errors
///
/// Returns an error if file reading, browser initialization, screenshot capture, or image decoding fails.
async fn load_chrome_rgba(
    stable_path: &Path,
    fixture: &Path,
    browser: &mut Option<ChromeBrowser>,
    page: &mut Option<Page>,
    timings: &mut Timings,
) -> Result<RgbaImage> {
    let t_start = Instant::now();
    if stable_path.exists() {
        let zbytes = read(stable_path)?;
        timings.cache_io += t_start.elapsed();
        let expected = (784u32 * 453u32 * 4) as usize;
        if let Ok(bytes) = zstd_decompress(&zbytes, expected) {
            return RgbaImage::from_raw(784, 453, bytes)
                .ok_or_else(|| anyhow!("Failed to create RgbaImage from decompressed bytes"));
        }
    }
    if browser.is_none() {
        let (chrome_browser, chrome_page) = init_browser().await?;
        *page = Some(chrome_page);
        *browser = Some(chrome_browser);
    }
    let page_ref = page
        .as_ref()
        .ok_or_else(|| anyhow!("page not initialized"))?;
    let t_cap = Instant::now();
    let png_bytes = capture_chrome_png(page_ref, fixture).await?;
    timings.chrome_capture += t_cap.elapsed();
    let t_decode = Instant::now();
    let img = load_from_memory(&png_bytes)?.to_rgba8();
    timings.png_decode += t_decode.elapsed();
    debug!(
        "Chrome image: width={}, height={}, buffer_size={}, expected_size={}",
        img.width(),
        img.height(),
        img.as_raw().len(),
        (784u32 * 453u32 * 4) as usize
    );
    let compressed = zstd_compress(img.as_raw(), 1).unwrap_or_default();
    let _ignore_error = write(stable_path, compressed);
    Ok(img)
}

struct Timings {
    cache_io: Duration,
    chrome_capture: Duration,
    build_dl: Duration,
    batch_dbg: Duration,
    raster: Duration,
    png_decode: Duration,
    equal_check: Duration,
    masked_diff: Duration,
    fail_write: Duration,
}

impl Timings {
    const fn new() -> Self {
        Self {
            cache_io: Duration::ZERO,
            chrome_capture: Duration::ZERO,
            build_dl: Duration::ZERO,
            batch_dbg: Duration::ZERO,
            raster: Duration::ZERO,
            png_decode: Duration::ZERO,
            equal_check: Duration::ZERO,
            masked_diff: Duration::ZERO,
            fail_write: Duration::ZERO,
        }
    }
}

struct CompareInfo<'cmp> {
    name: &'cmp str,
    path_hash_hex: &'cmp str,
    hash_hex: &'cmp str,
    failing_dir: &'cmp Path,
}

struct ComparisonContext<'ctx> {
    chrome_img: &'ctx RgbaImage,
    valor_img: &'ctx [u8],
    display_list: &'ctx DisplayList,
    dimensions: (u32, u32),
    info: &'ctx CompareInfo<'ctx>,
    timings: &'ctx mut Timings,
}

/// Compares Chrome and Valor images and writes failure artifacts if they differ.
///
/// # Errors
///
/// Returns an error if PNG writing fails.
fn compare_and_write_failures(ctx: &mut ComparisonContext<'_>) -> Result<bool> {
    let (width, height) = ctx.dimensions;
    let t_equal = Instant::now();
    ctx.timings.equal_check += t_equal.elapsed();
    if ctx.chrome_img.as_raw() == ctx.valor_img {
        return Ok(false);
    }
    let eps: u8 = 3;
    let masks = extract_text_masks(ctx.display_list, width, height);
    let diff_ctx = DiffCtx {
        width,
        height,
        eps,
        masks: &masks,
    };
    let t_diff = Instant::now();
    let (over, total) = per_pixel_diff_masked(ctx.chrome_img.as_raw(), ctx.valor_img, &diff_ctx);
    ctx.timings.masked_diff += t_diff.elapsed();
    let diff_ratio = (over as f64) / (total as f64);
    let allowed = 0.0125;
    if diff_ratio > allowed {
        let stamp = now_millis();
        let base = ctx.info.failing_dir.join(format!(
            "{}_{}_{}_{stamp}",
            ctx.info.name, ctx.info.path_hash_hex, ctx.info.hash_hex
        ));
        let chrome_path = base.with_extension("chrome.png");
        let valor_path = base.with_extension("valor.png");
        let diff_path = base.with_extension("diff.png");
        let t_write = Instant::now();
        create_dir_all(ctx.info.failing_dir)?;
        write_png_rgba_if_changed(&chrome_path, ctx.chrome_img.as_raw(), width, height)?;
        write_png_rgba_if_changed(&valor_path, ctx.valor_img, width, height)?;
        let diff_img = make_diff_image_masked(ctx.chrome_img.as_raw(), ctx.valor_img, &diff_ctx);
        write_png_rgba_if_changed(&diff_path, &diff_img, width, height)?;
        ctx.timings.fail_write += t_write.elapsed();
        error!(
            "[GRAPHICS] {} — pixel diffs found ({} over {}, {:.4}%); wrote\n  {}\n  {}\n  {}",
            ctx.info.name,
            over,
            total,
            diff_ratio * 100.0,
            chrome_path.display(),
            valor_path.display(),
            diff_path.display()
        );
        return Ok(true);
    }
    if over > 0 {
        info!(
            "[GRAPHICS] {} — {} pixels over epsilon out of {} ({:.4}%)",
            ctx.info.name,
            over,
            total,
            diff_ratio * 100.0
        );
    } else {
        info!(
            "[GRAPHICS] {} — exact match within masked regions",
            ctx.info.name
        );
    }
    Ok(false)
}

/// Sets up test directories for graphics artifacts.
///
/// # Errors
///
/// Returns an error if directory creation or removal fails.
fn setup_test_dirs() -> Result<(PathBuf, PathBuf)> {
    let out_dir = artifacts_subdir("graphics_artifacts");
    create_dir_all(&out_dir)?;
    let failing_dir = out_dir.join("failing");
    if failing_dir.exists() {
        remove_dir_all(&failing_dir)?;
    }
    create_dir_all(&failing_dir)?;
    Ok((out_dir, failing_dir))
}

struct FixtureContext<'ctx> {
    fixture: &'ctx Path,
    out_dir: &'ctx Path,
    failing_dir: &'ctx Path,
    browser: &'ctx mut Option<ChromeBrowser>,
    page: &'ctx mut Option<Page>,
    timings: &'ctx mut Timings,
    handle: &'ctx tokio::runtime::Handle,
}

/// Processes a single graphics fixture by comparing Chrome and Valor renders.
///
/// # Errors
///
/// Returns an error if fixture processing, rendering, or comparison fails.
async fn process_single_fixture(ctx: &mut FixtureContext<'_>) -> Result<bool> {
    let name = safe_stem(ctx.fixture);
    let canon = ctx
        .fixture
        .canonicalize()
        .unwrap_or_else(|_| ctx.fixture.to_path_buf());
    let current_hash = file_content_hash(&canon);
    let hash_hex = format!("{current_hash:016x}");
    let path_hash = fnv1a64_bytes(canon.to_string_lossy().as_bytes());
    let path_hash_hex = format!("{path_hash:016x}");
    cleanup_artifacts_for_hash(
        &name,
        &path_hash_hex,
        ctx.out_dir,
        ctx.failing_dir,
        &hash_hex,
    );

    let stable_chrome_rgba_zst = ctx
        .out_dir
        .join(format!("{name}_{path_hash_hex}_{hash_hex}_chrome.rgba.zst"));
    let chrome_img = load_chrome_rgba(
        &stable_chrome_rgba_zst,
        ctx.fixture,
        ctx.browser,
        ctx.page,
        ctx.timings,
    )
    .await?;

    let (width, height) = (784u32, 453u32);
    let t_build = Instant::now();
    let display_list =
        build_valor_display_list_for(ctx.handle, ctx.fixture, width, height).await?;
    ctx.timings.build_dl += t_build.elapsed();
    debug!(
        "[GRAPHICS][DEBUG] {}: DL items={} (first 5: {:?})",
        name,
        display_list.items.len(),
        display_list.items.iter().take(5).collect::<Vec<_>>()
    );

    let t_batch = Instant::now();
    let dbg_batches = batch_display_list(&display_list, width, height);
    ctx.timings.batch_dbg += t_batch.elapsed();
    let dbg_quads: usize = dbg_batches.iter().map(|batch| batch.quads.len()).sum();
    debug!(
        "[GRAPHICS][DEBUG] {}: batches={} total_quads={}",
        name,
        dbg_batches.len(),
        dbg_quads
    );

    let t_rast = Instant::now();
    let valor_img = rasterize_display_list_to_rgba(&display_list, width, height)?;
    ctx.timings.raster += t_rast.elapsed();

    let info = CompareInfo {
        name: &name,
        path_hash_hex: &path_hash_hex,
        hash_hex: &hash_hex,
        failing_dir: ctx.failing_dir,
    };

    compare_and_write_failures(&mut ComparisonContext {
        chrome_img: &chrome_img,
        valor_img: &valor_img,
        display_list: &display_list,
        dimensions: (width, height),
        info: &info,
        timings: ctx.timings,
    })
}

/// Tests graphics rendering by comparing Valor output with Chromium screenshots.
///
/// # Errors
///
/// Returns an error if test setup fails or any graphics comparisons fail.
pub fn chromium_graphics_smoke_compare_png() -> Result<()> {
    init_test_logger();
    let (out_dir, failing_dir) = setup_test_dirs()?;
    let fixtures = get_filtered_fixtures("GRAPHICS")?;
    if fixtures.is_empty() {
        info!(
            "[GRAPHICS] No fixtures found — add files under any crate's tests/fixtures/graphics/ subfolders"
        );
        return Ok(());
    }

    let runtime = Runtime::new()?;
    let handle = runtime.handle();
    let mut browser: Option<ChromeBrowser> = None;
    let mut page: Option<Page> = None;
    let mut any_failed = false;
    let mut ran = 0;
    let mut timings = Timings::new();

    for fixture in fixtures {
        if runtime.block_on(process_single_fixture(&mut FixtureContext {
            fixture: &fixture,
            out_dir: &out_dir,
            failing_dir: &failing_dir,
            browser: &mut browser,
            page: &mut page,
            timings: &mut timings,
            handle,
        }))? {
            any_failed = true;
        }
        ran += 1;
    }

    info!(
        "[GRAPHICS][TIMING][TOTALS] cache_io={:?} chrome_capture={:?} build_dl={:?} batch_dbg={:?} raster={:?} png_decode={:?} equal_check={:?} masked_diff={:?} fail_write={:?}",
        timings.cache_io,
        timings.chrome_capture,
        timings.build_dl,
        timings.batch_dbg,
        timings.raster,
        timings.png_decode,
        timings.equal_check,
        timings.masked_diff,
        timings.fail_write
    );

    if any_failed {
        return Err(anyhow!(
            "graphics comparison found differences — see artifacts under {}",
            failing_dir.display()
        ));
    }
    info!("[GRAPHICS] {ran} fixtures passed");
    Ok(())
}
