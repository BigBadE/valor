// Separate test file for text rendering report to avoid EventLoop conflicts
//
// This test must be run separately from chromium_tests because both create EventLoops
// which is not allowed on Windows.
//
// Run with: cargo test --package valor --test text_rendering_report_test

#[path = "chromium_compare"]
mod chromium_compare {
    pub mod chrome;
    pub mod common;
    pub mod valor;
}

use anyhow::Result;
use chromium_compare::chrome::{capture_screenshot_rgba, start_and_connect_chrome};
use chromium_compare::common::{init_test_logger, test_cache_dir, write_png_rgba_if_changed};
use chromium_compare::valor::{build_display_list_for_fixture, rasterize_display_list_to_rgba};
use chromiumoxide::page::Page;
use log::info;
use serde_json::to_string_pretty;
use std::collections::HashMap;
use std::env;
use std::fs::write;
use std::path::PathBuf;
use wgpu_backend::GlyphBounds;

#[derive(Debug, Clone)]
struct TextDiffMetrics {
    total_pixels: u64,
    text_pixels: u64,
    non_text_pixels: u64,
    text_diff_pixels: u64,
    non_text_diff_pixels: u64,
    max_channel_diff: u16,
    mean_channel_diff: f64,
    stddev_channel_diff: f64,
}

impl TextDiffMetrics {
    fn text_diff_ratio(&self) -> f64 {
        if self.text_pixels == 0 {
            0.0
        } else {
            (self.text_diff_pixels as f64) / (self.text_pixels as f64)
        }
    }

    fn non_text_diff_ratio(&self) -> f64 {
        if self.non_text_pixels == 0 {
            0.0
        } else {
            (self.non_text_diff_pixels as f64) / (self.non_text_pixels as f64)
        }
    }

    fn overall_diff_ratio(&self) -> f64 {
        if self.total_pixels == 0 {
            0.0
        } else {
            ((self.text_diff_pixels + self.non_text_diff_pixels) as f64)
                / (self.total_pixels as f64)
        }
    }
}

fn is_pixel_in_text_region(x_pos: u32, y_pos: u32, glyph_bounds: &[GlyphBounds]) -> bool {
    let x_float = x_pos as f32;
    let y_float = y_pos as f32;
    glyph_bounds.iter().any(|glyph| {
        x_float >= glyph.left
            && x_float < glyph.right
            && y_float >= glyph.top
            && y_float < glyph.bottom
    })
}

/// Compare two pixels with per-channel and per-pixel thresholds.
///
/// For text regions: max 50% per-channel diff, max 20% total pixel diff.
/// For non-text: exact match required.
///
/// # Returns
///
/// Returns true if pixel is acceptable (within thresholds).
fn compare_pixel_with_thresholds(
    pixel_a: &[u8],
    pixel_b: &[u8],
    idx: usize,
    is_text: bool,
) -> bool {
    const CHANNEL_THRESHOLD: u16 = 128;
    const PIXEL_THRESHOLD: u32 = 153;

    if idx + 3 >= pixel_a.len() || idx + 3 >= pixel_b.len() {
        return true;
    }

    if !is_text {
        return pixel_a[idx..idx + 4] == pixel_b[idx..idx + 4];
    }

    let mut total_diff = 0u32;
    for channel_offset in 0..3 {
        let channel_a = pixel_a[idx + channel_offset];
        let channel_b = pixel_b[idx + channel_offset];
        let diff = (i16::from(channel_a) - i16::from(channel_b)).unsigned_abs();

        if diff > CHANNEL_THRESHOLD {
            return false;
        }

        total_diff += u32::from(diff);
    }

    total_diff <= PIXEL_THRESHOLD
}

fn compute_text_diff_metrics(
    chrome_pixels: &[u8],
    valor_pixels: &[u8],
    width: u32,
    height: u32,
    glyph_bounds: &[GlyphBounds],
) -> TextDiffMetrics {
    let mut total_pixels = 0u64;
    let mut text_pixels = 0u64;
    let mut non_text_pixels = 0u64;
    let mut text_diff_pixels = 0u64;
    let mut non_text_diff_pixels = 0u64;
    let mut max_channel_diff = 0u16;
    let mut channel_diffs = Vec::new();

    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * 4) as usize;
            if idx + 3 >= chrome_pixels.len() || idx + 3 >= valor_pixels.len() {
                continue;
            }

            total_pixels += 1;
            let is_text = is_pixel_in_text_region(x, y, glyph_bounds);

            if is_text {
                text_pixels += 1;
            } else {
                non_text_pixels += 1;
            }

            let mut pixel_differs = false;
            for channel_offset in 0..4 {
                let chrome_value = chrome_pixels[idx + channel_offset];
                let valor_value = valor_pixels[idx + channel_offset];
                let diff = (i16::from(chrome_value) - i16::from(valor_value)).unsigned_abs();

                if diff > 0 {
                    pixel_differs = true;
                }

                max_channel_diff = max_channel_diff.max(diff);
                channel_diffs.push(f64::from(diff));
            }

            if pixel_differs {
                if is_text {
                    text_diff_pixels += 1;
                } else {
                    non_text_diff_pixels += 1;
                }
            }
        }
    }

    let mean_channel_diff = if channel_diffs.is_empty() {
        0.0
    } else {
        channel_diffs.iter().sum::<f64>() / (channel_diffs.len() as f64)
    };

    let variance = if channel_diffs.is_empty() {
        0.0
    } else {
        channel_diffs
            .iter()
            .map(|&x| {
                let diff = x - mean_channel_diff;
                diff * diff
            })
            .sum::<f64>()
            / (channel_diffs.len() as f64)
    };
    let stddev_channel_diff = variance.sqrt();

    TextDiffMetrics {
        total_pixels,
        text_pixels,
        non_text_pixels,
        text_diff_pixels,
        non_text_diff_pixels,
        max_channel_diff,
        mean_channel_diff,
        stddev_channel_diff,
    }
}

/// Runs text rendering comparison between Chrome and Valor.
///
/// # Errors
///
/// Returns an error if screenshot capture, display list building, or diff computation fails.
async fn run_text_rendering_comparison(page: &Page) -> Result<HashMap<String, TextDiffMetrics>> {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let fixture_path = workspace_root.join("crates/valor/tests/fixtures/text_render_matrix.html");
    let width = 1400u32;
    let height = 2000u32;

    info!("\n=== Text Rendering Comparison ===\n");
    info!("Loading Chrome render...");
    let chrome_img = capture_screenshot_rgba(page, &fixture_path, width, height).await?;

    info!("Building Valor display list...");
    let display_list = build_display_list_for_fixture(&fixture_path, width, height).await?;

    info!("Rendering Valor output...");
    let (valor_img, glyph_bounds) = rasterize_display_list_to_rgba(&display_list, width, height)?;

    info!("Computing diff metrics...");
    let metrics = compute_text_diff_metrics(
        chrome_img.as_raw(),
        &valor_img,
        width,
        height,
        &glyph_bounds,
    );

    let out_dir = test_cache_dir("text_rendering")?;

    write_png_rgba_if_changed(
        &out_dir.join("chrome.png"),
        chrome_img.as_raw(),
        width,
        height,
    )?;
    write_png_rgba_if_changed(&out_dir.join("valor.png"), &valor_img, width, height)?;

    // Create diff image highlighting failed pixels (red = fail, black = pass)
    let mut diff_img = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * 4) as usize;
            let is_text = is_pixel_in_text_region(x, y, &glyph_bounds);
            let matches = compare_pixel_with_thresholds(chrome_img.as_raw(), &valor_img, idx, is_text);

            if idx + 3 < diff_img.len() {
                diff_img[idx] = if matches { 0 } else { 255 };
                diff_img[idx + 1] = 0;
                diff_img[idx + 2] = 0;
                diff_img[idx + 3] = 255;
            }
        }
    }
    write_png_rgba_if_changed(&out_dir.join("diff.png"), &diff_img, width, height)?;

    let mut results = HashMap::new();
    results.insert("overall".to_string(), metrics);

    Ok(results)
}

fn log_sample_metrics(name: &str, metrics: &TextDiffMetrics) {
    info!("Sample: {name}");
    info!("  Total pixels: {}", metrics.total_pixels);
    info!(
        "  Text pixels: {} ({:.2}%)",
        metrics.text_pixels,
        (metrics.text_pixels as f64 / metrics.total_pixels as f64) * 100.0
    );
    info!(
        "  Non-text pixels: {} ({:.2}%)",
        metrics.non_text_pixels,
        (metrics.non_text_pixels as f64 / metrics.total_pixels as f64) * 100.0
    );
    info!("\n  Differences:");
    info!(
        "    Text diff pixels: {} ({:.4}% of text)",
        metrics.text_diff_pixels,
        metrics.text_diff_ratio() * 100.0
    );
    info!(
        "    Non-text diff pixels: {} ({:.4}% of non-text)",
        metrics.non_text_diff_pixels,
        metrics.non_text_diff_ratio() * 100.0
    );
    info!(
        "    Overall diff: {:.4}%",
        metrics.overall_diff_ratio() * 100.0
    );
    info!("\n  Channel statistics:");
    info!("    Max channel diff: {}/255", metrics.max_channel_diff);
    info!("    Mean channel diff: {:.2}", metrics.mean_channel_diff);
    info!(
        "    StdDev channel diff: {:.2}",
        metrics.stddev_channel_diff
    );
    info!("");
}

/// Generates and logs a text rendering comparison report.
///
/// # Errors
///
/// Returns an error if comparison, file I/O, or JSON serialization fails.
async fn print_text_rendering_report(page: &Page) -> Result<()> {
    let results = run_text_rendering_comparison(page).await?;

    info!("\n=== TEXT RENDERING ANALYSIS REPORT ===\n");

    for (name, metrics) in &results {
        log_sample_metrics(name, metrics);
    }

    let overall = results
        .get("overall")
        .ok_or_else(|| anyhow::anyhow!("Missing overall results"))?;

    info!("\n=== SUMMARY ===");
    info!(
        "Text rendering difference: {:.2}% of text pixels differ",
        overall.text_diff_ratio() * 100.0
    );
    info!(
        "Non-text rendering difference: {:.2}% of non-text pixels differ",
        overall.non_text_diff_ratio() * 100.0
    );
    info!("\nImages saved to: test_cache/text_rendering/");
    info!("  - chrome.png: Chrome render");
    info!("  - valor.png: Valor render");
    info!("  - diff.png: Differences (red=fail, black=pass)");

    let report_json = serde_json::json!({
        "results": results.iter().map(|(key, val)| {
            (key, serde_json::json!({
                "total_pixels": val.total_pixels,
                "text_pixels": val.text_pixels,
                "non_text_pixels": val.non_text_pixels,
                "text_diff_pixels": val.text_diff_pixels,
                "non_text_diff_pixels": val.non_text_diff_pixels,
                "text_diff_ratio": val.text_diff_ratio(),
                "non_text_diff_ratio": val.non_text_diff_ratio(),
                "overall_diff_ratio": val.overall_diff_ratio(),
                "max_channel_diff": val.max_channel_diff,
                "mean_channel_diff": val.mean_channel_diff,
                "stddev_channel_diff": val.stddev_channel_diff,
            }))
        }).collect::<HashMap<_, _>>(),
    });

    let out_dir = test_cache_dir("text_rendering")?;
    let report_path = out_dir.join("report.json");
    write(report_path, to_string_pretty(&report_json)?)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generates and logs a text rendering comparison report.
    ///
    /// # Errors
    ///
    /// Returns an error if Chrome fails to start, page creation fails, or report generation fails.
    ///
    /// # Panics
    ///
    /// May panic if the async runtime fails.
    #[tokio::test]
    async fn generate_text_rendering_report() -> Result<()> {
        init_test_logger();

        let browser_with_handler = start_and_connect_chrome().await?;

        let page = browser_with_handler.browser.new_page("about:blank").await?;

        print_text_rendering_report(&page).await?;

        Ok(())
    }
}
