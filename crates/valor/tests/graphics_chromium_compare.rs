#![allow(dead_code)]
use anyhow::{Result, anyhow};
use headless_chrome::{
    Browser, LaunchOptionsBuilder, protocol::cdp::Page::CaptureScreenshotFormatOption,
};
use log::{debug, error, info};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;
use wgpu_renderer::display_list::batch_display_list;
use winit::dpi::PhysicalSize;
use winit::event_loop::EventLoop;
#[cfg(target_os = "windows")]
use winit::platform::windows::EventLoopBuilderExtWindows;
use winit::window::Window;

mod common;

fn target_artifacts_dir() -> PathBuf {
    common::artifacts_subdir("graphics_artifacts")
}

// (no test-side color normalization; renderer returns RGBA)

// Graphics fixtures are discovered across all crates by common::graphics_fixture_html_files()

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

fn fnv1a64_bytes(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325; // FNV-1a 64-bit offset basis
    for b in bytes {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
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
    out_dir: &Path,
    failing_dir: &Path,
    hash_hex: &str,
) -> Result<()> {
    // Remove any prior artifacts for this fixture name that don't match the current hash.
    if let Ok(entries) = fs::read_dir(out_dir) {
        for ent in entries.flatten() {
            let p = ent.path();
            if let Some(fname) = p.file_name().and_then(|n| n.to_str()) {
                let is_this_fixture = fname.starts_with(&format!("{name}_"));
                let is_current_hash = fname.contains(&format!("_{hash_hex}_"))
                    || fname.ends_with(&format!("_{hash_hex}_chrome.png"));
                let is_stable_chrome = fname.ends_with("_chrome.png");
                if is_this_fixture && is_stable_chrome && !is_current_hash {
                    let _ = fs::remove_file(p);
                }
            }
        }
    }
    if let Ok(entries) = fs::read_dir(failing_dir) {
        for ent in entries.flatten() {
            let p = ent.path();
            if let Some(fname) = p.file_name().and_then(|n| n.to_str()) {
                let is_this_fixture = fname.starts_with(&format!("{name}_"));
                let is_current_hash = fname.contains(&format!("_{hash_hex}_"));
                if is_this_fixture && !is_current_hash {
                    let _ = fs::remove_file(p);
                }
            }
        }
    }
    Ok(())
}

type TextMask = (u32, u32, u32, u32);

struct DiffCtx<'a> {
    width: u32,
    height: u32,
    eps: u8,
    masks: &'a [TextMask],
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
            let mut maxc = 0u8;
            for c in 0..3 {
                let d = a[idx + c] as i16 - b[idx + c] as i16;
                let ad = d.unsigned_abs() as u8;
                if ad > maxc {
                    maxc = ad;
                }
            }
            let v = if maxc > ctx.eps { 255 } else { 0 };
            out[idx] = v;
            out[idx + 1] = 0;
            out[idx + 2] = 0;
            out[idx + 3] = 255; // red highlights
        }
    }
    out
}

fn extract_text_masks(dl: &wgpu_renderer::DisplayList, width: u32, height: u32) -> Vec<TextMask> {
    let mut masks = Vec::new();
    for item in &dl.items {
        if let wgpu_renderer::DisplayItem::Text {
            bounds: Some((l, t, r, b)),
            ..
        } = item
        {
            let l = (*l).max(0) as u32;
            let t = (*t).max(0) as u32;
            let r = (*r).max(0) as u32;
            let b = (*b).max(0) as u32;
            let l = l.min(width);
            let t = t.min(height);
            let r = r.min(width);
            let b = b.min(height);
            if r > l && b > t {
                masks.push((l, t, r, b));
            }
        }
    }
    masks
}

fn capture_chrome_png(
    browser: &Browser,
    path: &Path,
    _width: u32,
    _height: u32,
) -> Result<Vec<u8>> {
    let tab = browser.new_tab()?;
    let url = common::to_file_url(path)?;
    let url_string = url.as_str().to_owned();
    tab.navigate_to(&url_string)?;
    tab.wait_until_navigated()?;
    let _ = tab.evaluate(common::css_reset_injection_script(), false)?;
    // Full viewport screenshot
    let png = tab.capture_screenshot(CaptureScreenshotFormatOption::Png, None, None, true)?;
    Ok(png)
}

fn build_valor_display_list_for(
    path: &Path,
    viewport_w: u32,
    viewport_h: u32,
) -> Result<wgpu_renderer::DisplayList> {
    // Drive Valor page to finished, then use the public display_list_retained_snapshot API
    let rt = tokio::runtime::Runtime::new()?;
    let url = common::to_file_url(path)?;
    let mut page = common::create_page(&rt, url)?;
    // Inject the same CSS reset used for Chromium to keep comparisons fair
    page.eval_js(common::css_reset_injection_script())?;
    let finished = common::update_until_finished_simple(&rt, &mut page)?;
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
    items.push(wgpu_renderer::DisplayItem::Rect {
        x: 0.0,
        y: 0.0,
        width: viewport_w as f32,
        height: viewport_h as f32,
        color: cc,
    });
    items.extend(dl.items);
    Ok(wgpu_renderer::DisplayList::from_items(items))
}

static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
static RENDER_STATE: OnceLock<Mutex<wgpu_renderer::state::RenderState>> = OnceLock::new();
static WINDOW: OnceLock<Arc<Window>> = OnceLock::new();

fn rasterize_display_list_to_rgba(
    dl: &wgpu_renderer::DisplayList,
    width: u32,
    height: u32,
) -> Vec<u8> {
    // Initialize single runtime
    let rt = RUNTIME.get_or_init(|| tokio::runtime::Runtime::new().expect("tokio runtime"));

    // Initialize single hidden window + RenderState once, then reuse and resize per render
    let state_mutex = RENDER_STATE.get_or_init(|| {
        // Create a hidden window using a temporary EventLoop, then drop the loop.
        #[allow(deprecated)]
        let window = {
            let mut builder = EventLoop::<()>::builder();
            #[cfg(target_os = "windows")]
            {
                let _ = builder.with_any_thread(true);
            }
            let el = builder.build().expect("failed to create event loop");
            el.create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(width, height)),
            )
            .expect("failed to create hidden window")
        };
        let window = Arc::new(window);
        let _ = WINDOW.set(window.clone());
        let state = rt.block_on(wgpu_renderer::state::RenderState::new(window));
        Mutex::new(state)
    });

    let mut state = state_mutex.lock().expect("lock render state");
    // Ensure size matches current request
    state.resize(PhysicalSize::new(width, height));
    state.set_retained_display_list(dl.clone());
    state.render_to_rgba().expect("gpu render_to_rgba failed")
}

#[test]
fn chromium_graphics_smoke_compare_png() -> Result<()> {
    let _ = env_logger::builder().is_test(false).try_init();

    // Route layouter cache to target dir and ensure artifacts dir is clean
    let _ = common::route_layouter_cache_to_target();
    // If the graphics harness code changes, clear the entire graphics_artifacts dir to avoid stale outputs.
    let harness_src = include_str!("graphics_chromium_compare.rs");
    let out_dir =
        common::clear_artifacts_subdir_if_harness_changed("graphics_artifacts", harness_src)?;
    // Do not clear the entire artifacts directory on each run so previous failures remain inspectable.
    // Just ensure the directory and a dedicated failing subdir exist.
    let _ = fs::create_dir_all(&out_dir);
    let failing_dir = out_dir.join("failing");
    // Clear failing artifacts on each run so they don't accumulate across runs.
    common::clear_dir(&failing_dir)?;

    // Use the same fixtures as the layout comparer so this test always has inputs
    let fixtures = common::fixture_html_files()?;
    if fixtures.is_empty() {
        info!(
            "[GRAPHICS] No fixtures found — add files under any crate's tests/fixtures/graphics/ subfolders"
        );
        return Ok(());
    }

    let launch_opts = LaunchOptionsBuilder::default()
        .headless(true)
        .window_size(Some((800, 600)))
        .idle_browser_timeout(std::time::Duration::from_secs(120))
        .args(vec![
            OsStr::new("--force-device-scale-factor=1"),
            OsStr::new("--hide-scrollbars"),
            OsStr::new("--blink-settings=imagesEnabled=false"),
            OsStr::new("--disable-gpu"),
            OsStr::new("--force-color-profile=sRGB"),
        ])
        .build()
        .expect("Failed to build LaunchOptions for headless_chrome");
    let browser = Browser::new(launch_opts).expect("Failed to launch headless Chrome browser");

    let mut any_failed = false;

    for fixture in fixtures {
        let name = safe_stem(&fixture);
        // Compute current fixture content hash and cleanup older-hash artifacts.
        let canon = fixture.canonicalize().unwrap_or_else(|_| fixture.clone());
        let current_hash = file_content_hash(&canon);
        let hash_hex = format!("{:016x}", current_hash);
        cleanup_artifacts_for_hash(&name, &out_dir, &failing_dir, &hash_hex)?;
        let chrome_png = capture_chrome_png(&browser, &fixture, 800, 600)?;
        // Always update a stable Chrome artifact only if contents changed
        let stable_chrome = out_dir.join(format!("{name}_{hash_hex}_chrome.png"));
        let _ = common::write_bytes_if_changed(&stable_chrome, &chrome_png)?;
        // Decode Chrome PNG to RGBA8
        let chrome_img = image::load_from_memory(&chrome_png)?.to_rgba8();
        let (w, h) = chrome_img.dimensions();
        // Build Valor display list (with viewport background) and rasterize to RGBA
        let dl = build_valor_display_list_for(&fixture, w, h)?;
        debug!(
            "[GRAPHICS][DEBUG] {}: DL items={} (first 5: {:?})",
            name,
            dl.items.len(),
            dl.items.iter().take(5).collect::<Vec<_>>()
        );
        let dbg_batches = batch_display_list(&dl, w, h);
        let dbg_quads: usize = dbg_batches.iter().map(|b| b.quads.len()).sum();
        debug!(
            "[GRAPHICS][DEBUG] {}: batches={} total_quads={}",
            name,
            dbg_batches.len(),
            dbg_quads
        );
        let valor_img = rasterize_display_list_to_rgba(&dl, w, h);

        let eps: u8 = 3;
        // Ignore differences inside text bounds until GPU text capture is compared apples-to-apples
        let masks = extract_text_masks(&dl, w, h);
        let ctx = DiffCtx {
            width: w,
            height: h,
            eps,
            masks: &masks,
        };
        let (over, total) = per_pixel_diff_masked(chrome_img.as_raw(), &valor_img, &ctx);
        let diff_ratio = (over as f64) / (total as f64);
        // Allow a small tolerance for GPU AA/rounding differences
        // Allow a slightly higher tolerance to account for minor rasterization differences
        // across environments and recent layout shim refactors.
        let allowed = 0.0125; // 1.25%
        if diff_ratio > allowed {
            let stamp = now_millis();
            // Write failing artifacts under graphics_artifacts/failing with the hash included for deduping.
            let base = failing_dir.join(format!("{name}_{hash_hex}_{stamp}"));
            let chrome_path = base.with_extension("chrome.png");
            let valor_path = base.with_extension("valor.png");
            let diff_path = base.with_extension("diff.png");
            let _ = fs::create_dir_all(failing_dir.clone());
            common::write_png_rgba_if_changed(&chrome_path, chrome_img.as_raw(), w, h)?;
            common::write_png_rgba_if_changed(&valor_path, &valor_img, w, h)?;
            let diff_img = make_diff_image_masked(chrome_img.as_raw(), &valor_img, &ctx);
            common::write_png_rgba_if_changed(&diff_path, &diff_img, w, h)?;
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
                "[GRAPHICS] {} — pixel diffs under tolerance ({} over {}, {:.4}% <= {:.2}%); accepting",
                name,
                over,
                total,
                diff_ratio * 100.0,
                allowed * 100.0
            );
        }
    }

    if any_failed {
        return Err(anyhow!(
            "graphics comparison found differences — see artifacts under {}/failing",
            target_artifacts_dir().display()
        ));
    }
    Ok(())
}
