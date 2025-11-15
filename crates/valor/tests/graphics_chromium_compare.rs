use anyhow::{Result, anyhow};
use headless_chrome::{
    Browser, LaunchOptionsBuilder, Tab, protocol::cdp::Page::CaptureScreenshotFormatOption,
};
use image::{RgbaImage, load_from_memory};
use log::{debug, error, info, trace};
use renderer::batch_display_list;
use renderer::{DisplayItem, DisplayList};
use std::{
    ffi::OsStr,
    fs::{self, create_dir_all, read_dir, remove_dir_all, remove_file, write as fs_write},
    path::Path,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant, SystemTime},
};
use wgpu_backend::RenderState;
use winit::{
    dpi::{LogicalSize, PhysicalSize},
    event_loop::ActiveEventLoop,
    window::Window,
};

mod common;

#[cfg(test)]
mod tests {
    use super::*;

    use common::{css_reset_injection_script, fixture_html_files};
    use valor::test_support::{
        artifacts_subdir, create_page, to_file_url, update_until_finished_simple,
        write_png_rgba_if_changed,
    };

    // Fast compression for cached RGBA snapshots
    use zstd::bulk::{compress as zstd_compress, decompress as zstd_decompress};

    fn safe_stem(p: &Path) -> String {
        p.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("fixture")
            .to_string()
    }

    fn now_millis() -> String {
        let ms = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        format!("{ms}")
    }

    const fn fnv1a64_bytes(bytes: &[u8]) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325; // FNV-1a 64-bit offset basis
        let mut index = 0;
        while index < bytes.len() {
            hash ^= bytes[index] as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
            index += 1;
        }
        hash
    }

    fn file_content_hash(path: &Path) -> u64 {
        match fs::read(path) {
            Ok(bytes) => fnv1a64_bytes(&bytes),
            Err(_) => 0,
        }
    }

    fn cleanup_artifacts_for_hash(
        name: &str,
        path_hash_hex: &str,
        out_dir: &Path,
        failing_dir: &Path,
        hash_hex: &str,
    ) -> Result<()> {
        // Remove any prior artifacts for this fixture name that don't match the current hash.
        if let Ok(entries) = read_dir(out_dir) {
            for ent in entries.flatten() {
                let entry_path = ent.path();
                if let Some(fname) = entry_path.file_name().and_then(|n| n.to_str()) {
                    let prefix = format!("{name}_{path_hash_hex}_");
                    let is_this_fixture = fname.starts_with(&prefix);
                    let is_current_hash = fname.contains(&format!("_{hash_hex}_"))
                        || fname.ends_with(&format!("_{hash_hex}_chrome.png"))
                        || fname.ends_with(&format!("_{hash_hex}_chrome.rgba.zst"));
                    let is_stable_chrome =
                        fname.ends_with("_chrome.png") || fname.ends_with("_chrome.rgba.zst");
                    if is_this_fixture && is_stable_chrome && !is_current_hash {
                        drop(remove_file(entry_path));
                    }
                }
            }
        }
        if let Ok(entries) = read_dir(failing_dir) {
            for ent in entries.flatten() {
                let entry_path = ent.path();
                if let Some(fname) = entry_path.file_name().and_then(|n| n.to_str()) {
                    let prefix = format!("{name}_{path_hash_hex}_");
                    let is_this_fixture = fname.starts_with(&prefix);
                    let is_current_hash = fname.contains(&format!("_{hash_hex}_"));
                    if is_this_fixture && !is_current_hash {
                        drop(remove_file(entry_path));
                    }
                }
            }
        }
        Ok(())
    }

    type TextMask = (u32, u32, u32, u32);

    struct DiffCtx<'diff> {
        width: u32,
        height: u32,
        eps: u8,
        masks: &'diff [TextMask],
    }

    fn per_pixel_diff_masked(a: &[u8], b: &[u8], ctx: &DiffCtx<'_>) -> (u64, u64) {
        let mut total: u64 = 0;
        let mut over: u64 = 0;
        for y in 0..ctx.height {
            for x in 0..ctx.width {
                // Skip masked text regions
                let mut masked = false;
                for &(l, t, r, b) in ctx.masks {
                    if x >= l && x < r && y >= t && y < b {
                        masked = true;
                        break;
                    }
                }
                if masked {
                    continue;
                }
                let idx = ((y * ctx.width + x) * 4) as usize;
                // Ensure we don't access beyond array bounds
                if idx + 3 >= a.len() || idx + 3 >= b.len() {
                    continue;
                }
                for c in 0..4 {
                    let da = a[idx + c] as i16 - b[idx + c] as i16;
                    let ad = da.unsigned_abs() as u8;
                    total += 1;
                    if ad > ctx.eps {
                        over += 1;
                    }
                }
            }
        }
        (over, total)
    }

    fn make_diff_image_masked(a: &[u8], b: &[u8], ctx: &DiffCtx<'_>) -> Vec<u8> {
        let mut out = vec![0u8; (ctx.width * ctx.height * 4) as usize];
        for y in 0..ctx.height {
            for x in 0..ctx.width {
                let idx = ((y * ctx.width + x) * 4) as usize;
                // Masked pixels are left black/transparent in diff
                let mut masked = false;
                for &(l, t, r, b) in ctx.masks {
                    if x >= l && x < r && y >= t && y < b {
                        masked = true;
                        break;
                    }
                }
                if masked {
                    continue;
                }
                // Ensure we don't access beyond array bounds
                if idx + 3 >= a.len() || idx + 3 >= b.len() || idx + 3 >= out.len() {
                    continue;
                }
                let mut max_channel_diff = 0u8;
                for channel in 0..3 {
                    let diff = a[idx + channel] as i16 - b[idx + channel] as i16;
                    let abs_diff = diff.unsigned_abs() as u8;
                    if abs_diff > max_channel_diff {
                        max_channel_diff = abs_diff;
                    }
                }
                let val = if max_channel_diff > ctx.eps { 255 } else { 0 };
                out[idx] = val;
                out[idx + 1] = 0;
                out[idx + 2] = 0;
                out[idx + 3] = 255; // red highlights
            }
        }
        out
    }

    fn extract_text_masks(dl: &DisplayList, width: u32, height: u32) -> Vec<TextMask> {
        let mut masks = Vec::new();
        for item in &dl.items {
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

    fn capture_chrome_png(tab: &Tab, path: &Path) -> Result<Vec<u8>> {
        let url = to_file_url(path)?;
        let url_string = url.as_str().to_owned();
        tab.navigate_to(&url_string)?;
        tab.wait_until_navigated()?;
        let _ = tab.evaluate(css_reset_injection_script(), false)?;
        // Full viewport screenshot
        let png = tab.capture_screenshot(CaptureScreenshotFormatOption::Png, None, None, true)?;
        Ok(png)
    }

    fn build_valor_display_list_for(
        path: &Path,
        viewport_w: u32,
        viewport_h: u32,
    ) -> Result<DisplayList> {
        // Drive Valor page to finished, then use the public display_list_retained_snapshot API
        let rt = tokio::runtime::Runtime::new()?;
        let url = to_file_url(path)?;
        let mut page = create_page(&rt, url)?;
        // Inject the same CSS reset used for Chromium to keep comparisons fair
        page.eval_js(css_reset_injection_script())?;
        let finished = update_until_finished_simple(&rt, &mut page)?;
        if !finished {
            return Err(anyhow!("Valor parsing did not finish"));
        }
        // One more update after parse finished to ensure late stylesheet/attr merges are applied
        // (mirrors the layout harness behavior before snapshotting geometry).
        let _ = rt.block_on(page.update());
        let dl = page.display_list_retained_snapshot()?;
        // Prepend a full-viewport background using the same logic as the app
        let cc = page.background_rgba();
        let mut items = Vec::with_capacity(dl.items.len() + 1);
        items.push(DisplayItem::Rect {
            x: 0.0,
            y: 0.0,
            width: viewport_w as f32,
            height: viewport_h as f32,
            color: cc,
        });
        items.extend(dl.items);
        Ok(DisplayList::from_items(items))
    }

    // Counter to track which fixture we're rendering
    static RENDER_COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    static RENDER_STATE: OnceLock<Mutex<RenderState>> = OnceLock::new();
    static WINDOW: OnceLock<Arc<Window>> = OnceLock::new();

    fn rasterize_display_list_to_rgba(
        dl: &DisplayList,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>> {
        use std::sync::atomic::Ordering;
        use winit::platform::windows::EventLoopBuilderExtWindows as _;

        let state_mutex = RENDER_STATE.get_or_init(|| {
            let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");

            let event_loop = winit::event_loop::EventLoop::builder()
                .with_any_thread(true)
                .build()
                .expect("failed to create event loop");

            let window = {
                struct WindowCreator {
                    window: Option<Window>,
                    width: u32,
                    height: u32,
                }

                impl WindowCreator {
                    fn create_window_if_needed(&mut self, event_loop: &ActiveEventLoop) {
                        if self.window.is_some() {
                            return;
                        }
                        let window = event_loop
                            .create_window(
                                Window::default_attributes()
                                    .with_title("Valor Test")
                                    .with_inner_size(LogicalSize::new(self.width, self.height))
                                    .with_visible(false),
                            )
                            .expect("failed to create window");
                        self.window = Some(window);
                    }
                }

                impl winit::application::ApplicationHandler for WindowCreator {
                    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
                        self.create_window_if_needed(event_loop);
                        event_loop.exit();
                    }

                    fn window_event(
                        &mut self,
                        _event_loop: &ActiveEventLoop,
                        _window_id: winit::window::WindowId,
                        _event: winit::event::WindowEvent,
                    ) {
                    }
                }

                let mut app = WindowCreator {
                    window: None,
                    width,
                    height,
                };

                event_loop
                    .run_app(&mut app)
                    .expect("failed to run event loop");
                app.window.expect("window not created")
            };

            let window = Arc::new(window);
            drop(WINDOW.set(Arc::clone(&window)));
            let state = rt
                .block_on(RenderState::new(window))
                .expect("failed to create render state");
            Mutex::new(state)
        });

        let mut state = state_mutex
            .lock()
            .map_err(|err| anyhow!("Failed to lock render state: {err}"))?;
        let render_num = RENDER_COUNTER.fetch_add(1, Ordering::SeqCst);

        info!(
            "=== Rendering fixture #{} with {} items ===",
            render_num + 1,
            dl.items.len()
        );

        // Log display list contents for debugging
        if render_num >= 25 {
            // Log details for fixtures around the problematic one
            info!("Display list #{} contents:", render_num + 1);
            for (idx, item) in dl.items.iter().enumerate() {
                info!("  Item {idx}: {item:?}");
            }
        }

        // Reset state to prevent corruption from previous renders
        // This flushes GPU operations, clears resources, and reinitializes text renderer
        state.reset_for_next_frame();
        // Ensure size matches current request
        state.resize(PhysicalSize::new(width, height));
        state.set_retained_display_list(dl.clone());

        // Render with comprehensive error handling
        let result = state.render_to_rgba();

        // If render failed, log detailed error information
        if let Err(render_err) = &result {
            error!("=== RENDER FAILED for fixture #{} ===", render_num + 1);
            error!("Error: {render_err:?}");
            error!("Display list had {} items", dl.items.len());
            error!("Display list items:");
            for (idx, item) in dl.items.iter().enumerate() {
                error!("  Item {idx}: {item:?}");
            }
        } else {
            info!("=== Fixture #{} rendered successfully ===", render_num + 1);
        }

        result
    }

    /// Run graphics comparison tests against Chromium.
    ///
    /// # Errors
    /// Returns an error if any test fails or setup fails.
    #[test]
    fn chromium_graphics_smoke_compare_png() -> Result<()> {
        use env_logger::Builder as LogBuilder;
        drop(LogBuilder::default().is_test(false).try_init());

        // Route artifacts to target/graphics_artifacts and keep them across runs.
        let out_dir = artifacts_subdir("graphics_artifacts");
        create_dir_all(&out_dir)?;
        let failing_dir = out_dir.join("failing");
        // Clear failing artifacts on each run so they don't accumulate across runs.
        if failing_dir.exists() {
            remove_dir_all(&failing_dir)?;
        }
        create_dir_all(&failing_dir)?;

        // Use the same fixtures as the layout comparer so this test always has inputs
        let fixtures = fixture_html_files()?;
        if fixtures.is_empty() {
            info!(
                "[GRAPHICS] No fixtures found — add files under any crate's tests/fixtures/graphics/ subfolders"
            );
            return Ok(());
        }

        // Lazily create the headless Chrome browser and a single tab only if a capture is required.
        let mut browser: Option<Browser> = None;
        let mut tab: Option<Arc<Tab>> = None;

        let mut any_failed = false;
        // Aggregate timings across all fixtures
        let mut agg_cache_io = Duration::ZERO; // read or write cached PNG
        let mut agg_chrome_capture = Duration::ZERO; // headless Chrome screenshot
        let mut agg_build_dl = Duration::ZERO; // build Valor display list
        let mut agg_batch_dbg = Duration::ZERO; // batch_display_list debug step
        let mut agg_raster = Duration::ZERO; // GPU rasterize to RGBA
        let mut agg_png_decode = Duration::ZERO; // decode Chrome PNG to RGBA
        let mut agg_equal_check = Duration::ZERO; // byte-for-byte equality check
        let mut agg_masked_diff = Duration::ZERO; // masked per-pixel diff and diff image
        let mut agg_fail_write = Duration::ZERO; // write failing artifacts

        for fixture in fixtures {
            let name = safe_stem(&fixture);
            // Compute current fixture content hash and cleanup older-hash artifacts.
            let canon = fixture.canonicalize().unwrap_or_else(|_| fixture.clone());
            let current_hash = file_content_hash(&canon);
            let hash_hex = format!("{current_hash:016x}");
            let path_hash = fnv1a64_bytes(canon.to_string_lossy().as_bytes());
            let path_hash_hex = format!("{path_hash:016x}");
            cleanup_artifacts_for_hash(&name, &path_hash_hex, &out_dir, &failing_dir, &hash_hex)?;

            // Cached/stable decoded RGBA path with fast zstd compression (PNG is not cached)
            let stable_chrome_rgba_zst =
                out_dir.join(format!("{name}_{path_hash_hex}_{hash_hex}_chrome.rgba.zst"));
            let t_cache_io_start = Instant::now();
            // Ensure Chrome is available only if we need to capture
            let chrome_img = if stable_chrome_rgba_zst.exists() {
                // Read cached RGBA (compressed) directly
                let zbytes = fs::read(&stable_chrome_rgba_zst)?;
                agg_cache_io += t_cache_io_start.elapsed();
                let expected = (784u32 * 453u32 * 4) as usize;
                // Decompress and validate length
                if let Ok(bytes) = zstd_decompress(&zbytes, expected) {
                    RgbaImage::from_raw(784, 453, bytes).ok_or_else(|| {
                        anyhow!("Failed to create RgbaImage from decompressed bytes")
                    })?
                } else {
                    // Corrupted cache; fall back to capture
                    if browser.is_none() {
                        let launch_opts = LaunchOptionsBuilder::default()
                            .headless(true)
                            .window_size(Some((800, 600)))
                            .idle_browser_timeout(Duration::from_secs(120))
                            .args(vec![
                                OsStr::new("--force-device-scale-factor=1"),
                                OsStr::new("--hide-scrollbars"),
                                OsStr::new("--blink-settings=imagesEnabled=false"),
                                OsStr::new("--disable-gpu"),
                                OsStr::new("--force-color-profile=sRGB"),
                            ])
                            .build()?;
                        let chrome_browser = Browser::new(launch_opts)?;
                        let chrome_tab = chrome_browser.new_tab()?;
                        tab = Some(chrome_tab);
                        browser = Some(chrome_browser);
                    }
                    let tab_ref = tab.as_ref();
                    let t_cap = Instant::now();
                    let png_bytes = capture_chrome_png(
                        tab_ref.ok_or_else(|| anyhow!("tab not initialized"))?,
                        &fixture,
                    )?;
                    agg_chrome_capture += t_cap.elapsed();
                    let t_decode = Instant::now();
                    let img = load_from_memory(&png_bytes)?.to_rgba8();
                    agg_png_decode += t_decode.elapsed();
                    debug!(
                        "Chrome image decoded: width={}, height={}, buffer_size={}, expected_size={}",
                        img.width(),
                        img.height(),
                        img.as_raw().len(),
                        (784u32 * 453u32 * 4) as usize
                    );
                    // Compress and write RGBA cache
                    let level = 1; // fast
                    let compressed = zstd_compress(img.as_raw(), level).unwrap_or_default();
                    drop(fs_write(&stable_chrome_rgba_zst, compressed));
                    img
                }
            } else {
                // Initialize browser on first cache miss
                if browser.is_none() {
                    let launch_opts = LaunchOptionsBuilder::default()
                        .headless(true)
                        .window_size(Some((800, 600)))
                        .idle_browser_timeout(Duration::from_secs(120))
                        .args(vec![
                            OsStr::new("--force-device-scale-factor=1"),
                            OsStr::new("--hide-scrollbars"),
                            OsStr::new("--blink-settings=imagesEnabled=false"),
                            OsStr::new("--disable-gpu"),
                            OsStr::new("--force-color-profile=sRGB"),
                        ])
                        .build()?;
                    let chrome_browser = Browser::new(launch_opts)?;
                    let chrome_tab = chrome_browser.new_tab()?;
                    tab = Some(chrome_tab);
                    browser = Some(chrome_browser);
                }
                let tab_ref = tab.as_ref();
                let t_cap = Instant::now();
                let png_bytes = capture_chrome_png(
                    tab_ref.ok_or_else(|| anyhow!("tab not initialized"))?,
                    &fixture,
                )?;
                agg_chrome_capture += t_cap.elapsed();
                let t_decode = Instant::now();
                let img = load_from_memory(&png_bytes)?.to_rgba8();
                agg_png_decode += t_decode.elapsed();
                debug!(
                    "Chrome image decoded: width={}, height={}, buffer_size={}, expected_size={}",
                    img.width(),
                    img.height(),
                    img.as_raw().len(),
                    (784u32 * 453u32 * 4) as usize
                );
                // Compress and write RGBA cache for future runs
                let level = 1; // fast
                let compressed = zstd_compress(img.as_raw(), level).unwrap_or_default();
                drop(fs_write(&stable_chrome_rgba_zst, compressed));
                img
            };

            // Known Chrome viewport size for our harness: 784x453.
            let (width, height) = (784u32, 453u32);

            // Build Valor display list
            let t_build = Instant::now();
            let display_list = build_valor_display_list_for(&fixture, width, height)?;
            agg_build_dl += t_build.elapsed();
            debug!(
                "[GRAPHICS][DEBUG] {}: DL items={} (first 5: {:?})",
                name,
                display_list.items.len(),
                display_list.items.iter().take(5).collect::<Vec<_>>()
            );

            // Batch debug (diagnostic)
            let t_batch = Instant::now();
            let dbg_batches = batch_display_list(&display_list, width, height);
            agg_batch_dbg += t_batch.elapsed();
            let dbg_quads: usize = dbg_batches.iter().map(|batch| batch.quads.len()).sum();
            debug!(
                "[GRAPHICS][DEBUG] {}: batches={} total_quads={}",
                name,
                dbg_batches.len(),
                dbg_quads
            );

            // Rasterize Valor
            let t_rast = Instant::now();
            let valor_img = rasterize_display_list_to_rgba(&display_list, width, height)?;
            agg_raster += t_rast.elapsed();

            // chrome_img is already prepared above as an RGBA image (possibly from cache)

            // Exact equality short-circuit
            let t_equal = Instant::now();
            let mut skipped_diff = false;
            agg_equal_check += t_equal.elapsed();
            if chrome_img.as_raw() == &valor_img {
                skipped_diff = true;
            } else {
                let eps: u8 = 3;
                // Ignore differences inside text bounds until GPU text capture is compared apples-to-apples
                let masks = extract_text_masks(&display_list, width, height);
                let ctx = DiffCtx {
                    width,
                    height,
                    eps,
                    masks: &masks,
                };
                let t_diff = Instant::now();
                let (over, total) = per_pixel_diff_masked(chrome_img.as_raw(), &valor_img, &ctx);
                agg_masked_diff += t_diff.elapsed();
                let diff_ratio = (over as f64) / (total as f64);
                let allowed = 0.0125; // 1.25%
                if diff_ratio > allowed {
                    let stamp = now_millis();
                    // Write failing artifacts under graphics_artifacts/failing with path+hash included for deduping.
                    let base =
                        failing_dir.join(format!("{name}_{path_hash_hex}_{hash_hex}_{stamp}"));
                    let chrome_path = base.with_extension("chrome.png");
                    let valor_path = base.with_extension("valor.png");
                    let diff_path = base.with_extension("diff.png");
                    let t_write_fail = Instant::now();
                    let _dir_result = fs::create_dir_all(failing_dir.clone());
                    write_png_rgba_if_changed(&chrome_path, chrome_img.as_raw(), width, height)?;
                    write_png_rgba_if_changed(&valor_path, &valor_img, width, height)?;
                    let diff_img = make_diff_image_masked(chrome_img.as_raw(), &valor_img, &ctx);
                    write_png_rgba_if_changed(&diff_path, &diff_img, width, height)?;
                    agg_fail_write += t_write_fail.elapsed();
                    error!(
                        "[GRAPHICS] {} — pixel diffs found ({} over {}, {:.4}%); wrote\n  {}\n  {}\n  {}",
                        name,
                        over,
                        total,
                        diff_ratio * 100.0,
                        chrome_path.display(),
                        valor_path.display(),
                        diff_path.display()
                    );
                    any_failed = true;
                } else if over > 0 {
                    info!(
                        "[GRAPHICS] {} — {} pixels over epsilon out of {} ({:.4}%)",
                        name,
                        over,
                        total,
                        diff_ratio * 100.0
                    );
                } else {
                    info!("[GRAPHICS] {name} — exact match within masked regions");
                }
            }

            trace!(
                "[GRAPHICS][TIMING] {name}: cache_io={agg_cache_io:?} chrome_capture={agg_chrome_capture:?} build_dl={agg_build_dl:?} batch_dbg={agg_batch_dbg:?} raster={agg_raster:?} png_decode={agg_png_decode:?} equal_check={agg_equal_check:?} masked_diff={agg_masked_diff:?} fail_write={agg_fail_write:?} skipped_diff={skipped_diff}"
            );
        }

        // Aggregate timing summary across all fixtures
        info!(
            "[GRAPHICS][TIMING][TOTALS] cache_io={agg_cache_io:?} chrome_capture={agg_chrome_capture:?} build_dl={agg_build_dl:?} batch_dbg={agg_batch_dbg:?} raster={agg_raster:?} png_decode={agg_png_decode:?} equal_check={agg_equal_check:?} masked_diff={agg_masked_diff:?} fail_write={agg_fail_write:?}"
        );

        if any_failed {
            return Err(anyhow!(
                "graphics comparison found differences — see artifacts under {}",
                failing_dir.display()
            ));
        }
        Ok(())
    }
}
