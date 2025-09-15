#![allow(unnameable_test_items)]
#![allow(dead_code)]
use anyhow::{Result, anyhow};
use headless_chrome::{
    Browser, LaunchOptionsBuilder, protocol::cdp::Page::CaptureScreenshotFormatOption,
};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use wgpu_renderer::display_list::batch_display_list;

mod common;

fn target_artifacts_dir() -> PathBuf {
    common::artifacts_subdir("graphics_artifacts")
}

fn fixtures_graphics_dir() -> PathBuf {
    common::fixtures_dir().join("graphics")
}

fn collect_html_recursively(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = fs::read_dir(dir)?;
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_html_recursively(&p, out)?;
        } else if p
            .extension()
            .map(|ext| ext.eq_ignore_ascii_case("html"))
            .unwrap_or(false)
        {
            out.push(p);
        }
    }
    Ok(())
}

fn list_graphics_fixtures() -> Result<Vec<PathBuf>> {
    let dir = fixtures_graphics_dir();
    let mut v = Vec::new();
    if dir.exists() {
        collect_html_recursively(&dir, &mut v)?;
    }
    v.sort();
    Ok(v)
}

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
    format!("{}", ms)
}

fn per_pixel_diff(a: &[u8], b: &[u8], eps: u8) -> (u64, u64) {
    let mut total: u64 = 0;
    let mut over: u64 = 0;
    let n = a.len().min(b.len());
    let mut i = 0;
    while i + 3 < n {
        for c in 0..4 {
            let da = a[i + c] as i16 - b[i + c] as i16;
            let ad = da.unsigned_abs() as u8;
            total += 1;
            if ad > eps {
                over += 1;
            }
        }
        i += 4;
    }
    (over, total)
}

fn make_diff_image(a: &[u8], b: &[u8], width: u32, height: u32, eps: u8) -> Vec<u8> {
    let mut out = vec![0u8; (width * height * 4) as usize];
    let n = a.len().min(b.len());
    let mut i = 0;
    while i + 3 < n {
        let mut maxc = 0u8;
        for c in 0..3 {
            let d = a[i + c] as i16 - b[i + c] as i16;
            let ad = d.unsigned_abs() as u8;
            if ad > maxc {
                maxc = ad;
            }
        }
        let v = if maxc > eps { 255 } else { 0 };
        out[i] = v;
        out[i + 1] = 0;
        out[i + 2] = 0;
        out[i + 3] = 255; // red highlights
        i += 4;
    }
    out
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

fn build_valor_display_list_for(path: &Path) -> Result<wgpu_renderer::DisplayList> {
    // Drive Valor page to finished, then use the public display_list_retained_snapshot API
    let rt = tokio::runtime::Runtime::new()?;
    let url = common::to_file_url(path)?;
    let mut page = common::create_page(&rt, url)?;
    let finished = common::update_until_finished_simple(&rt, &mut page)?;
    if !finished {
        return Err(anyhow!("Valor parsing did not finish"));
    }
    let dl = page.display_list_retained_snapshot()?;
    Ok(dl)
}

fn rasterize_display_list_to_rgba(
    dl: &wgpu_renderer::DisplayList,
    width: u32,
    height: u32,
) -> Vec<u8> {
    // Simple CPU rasterizer for solid rects with alpha; text is currently ignored in DL batching
    let mut out = vec![255u8; (width * height * 4) as usize]; // white background
    let batches = batch_display_list(dl, width, height);
    for b in batches.into_iter() {
        // Determine scissor box
        let (sx, sy, sw, sh) = b.scissor.unwrap_or((0, 0, width, height));
        let sx1 = (sx + sw).min(width);
        let sy1 = (sy + sh).min(height);
        for q in b.quads.iter() {
            let x0 = q.x.max(sx as f32).floor().max(0.0) as u32;
            let y0 = q.y.max(sy as f32).floor().max(0.0) as u32;
            let x1 = (q.x + q.width).ceil().min(sx1 as f32).max(0.0) as u32;
            let y1 = (q.y + q.height).ceil().min(sy1 as f32).max(0.0) as u32;
            let sr = (q.color[0].clamp(0.0, 1.0) * 255.0).round() as u8;
            let sg = (q.color[1].clamp(0.0, 1.0) * 255.0).round() as u8;
            let sb = (q.color[2].clamp(0.0, 1.0) * 255.0).round() as u8;
            let sa = (q.color[3].clamp(0.0, 1.0) * 255.0).round() as u8;
            for y in y0..y1 {
                for x in x0..x1 {
                    let idx = ((y * width + x) * 4) as usize;
                    // Alpha blend src over dst in sRGB byte space (approximate)
                    let da = out[idx + 3];
                    let inv_a = 255u16.saturating_sub(sa as u16);
                    out[idx] =
                        (((sr as u16) * (sa as u16) + (out[idx] as u16) * inv_a) / 255) as u8;
                    out[idx + 1] =
                        (((sg as u16) * (sa as u16) + (out[idx + 1] as u16) * inv_a) / 255) as u8;
                    out[idx + 2] =
                        (((sb as u16) * (sa as u16) + (out[idx + 2] as u16) * inv_a) / 255) as u8;
                    out[idx + 3] = 255u8
                        .saturating_sub(((255u16 - da as u16) * (255u16 - sa as u16) / 255) as u8);
                }
            }
        }
    }
    out
}

//#[test]
fn chromium_graphics_smoke_compare_png() -> Result<()> {
    // Note: will likely fail (placeholder Valor) — saves artifacts for inspection.
    let _ = env_logger::builder().is_test(false).try_init();

    // Route layouter cache to target dir and ensure artifacts dir is clean
    let _ = common::route_layouter_cache_to_target();
    let out_dir = target_artifacts_dir();
    common::clear_dir(&out_dir)?;

    let fixtures = list_graphics_fixtures()?;
    if fixtures.is_empty() {
        eprintln!(
            "[GRAPHICS] No fixtures under {} — add files under crates/valor/tests/fixtures/graphics/",
            fixtures_graphics_dir().display()
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
        let chrome_png = capture_chrome_png(&browser, &fixture, 800, 600)?;
        // Always update a stable Chrome artifact only if contents changed
        let stable_chrome = out_dir.join(format!("{}_chrome.png", name));
        let _ = common::write_bytes_if_changed(&stable_chrome, &chrome_png)?;
        // Decode Chrome PNG to RGBA8
        let chrome_img = image::load_from_memory(&chrome_png)?.to_rgba8();
        let (w, h) = chrome_img.dimensions();
        // Build Valor display list and rasterize to RGBA
        let dl = build_valor_display_list_for(&fixture)?;
        let valor_img = rasterize_display_list_to_rgba(&dl, w, h);

        let eps: u8 = 3;
        let (over, total) = per_pixel_diff(chrome_img.as_raw(), &valor_img, eps);
        if over > 0 {
            any_failed = true;
            let stamp = now_millis();
            let base = out_dir.join(format!("{}_{}", name, stamp));
            let chrome_path = base.with_extension("chrome.png");
            let valor_path = base.with_extension("valor.png");
            let diff_path = base.with_extension("diff.png");
            let _ = fs::create_dir_all(out_dir.clone());
            common::write_png_rgba_if_changed(&chrome_path, chrome_img.as_raw(), w, h)?;
            common::write_png_rgba_if_changed(&valor_path, &valor_img, w, h)?;
            let diff_img = make_diff_image(chrome_img.as_raw(), &valor_img, w, h, eps);
            common::write_png_rgba_if_changed(&diff_path, &diff_img, w, h)?;
            eprintln!(
                "[GRAPHICS] {} — pixel diffs found ({} over {}); wrote\n  {}\n  {}\n  {}",
                name,
                over,
                total,
                chrome_path.display(),
                valor_path.display(),
                diff_path.display()
            );
        }
    }

    if any_failed {
        return Err(anyhow!(
            "graphics comparison found differences — see artifacts under {}",
            target_artifacts_dir().display()
        ));
    }
    Ok(())
}
