use anyhow::{Result, anyhow};
use chromiumoxide::page::Page;
use image::RgbaImage;
use std::path::Path;
use wgpu_backend::GlyphBounds;
use zstd::bulk::{compress as zstd_compress, decompress as zstd_decompress};

use super::cache_utils::{CacheFetcher, read_or_fetch_cache, test_failing_dir};
use super::chrome::capture_screenshot_rgba;
use super::common::write_png_rgba_if_changed;
use super::valor::{build_display_list_for_fixture, rasterize_display_list_to_rgba};

fn safe_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("fixture")
        .to_string()
}

/// Check if a pixel coordinate is inside any glyph bounding box.
fn is_pixel_in_text_region(x: u32, y: u32, glyph_bounds: &[GlyphBounds]) -> bool {
    let x_f = x as f32;
    let y_f = y as f32;
    glyph_bounds.iter().any(|glyph| {
        x_f >= glyph.left && x_f < glyph.right && y_f >= glyph.top && y_f < glyph.bottom
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

/// Count pixels that fail comparison thresholds.
fn count_pixel_failures(
    pixels_a: &[u8],
    pixels_b: &[u8],
    width: u32,
    height: u32,
    glyph_bounds: &[GlyphBounds],
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
    glyph_bounds: &[GlyphBounds],
) -> Vec<u8> {
    let mut out = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * 4) as usize;
            let is_text = is_pixel_in_text_region(x, y, glyph_bounds);
            let matches = compare_pixel_with_thresholds(pixels_a, pixels_b, idx, is_text);

            if idx + 3 < out.len() {
                out[idx] = if matches { 0 } else { 255 };
                out[idx + 1] = 0;
                out[idx + 2] = 0;
                out[idx + 3] = 255;
            }
        }
    }
    out
}

/// Loads Chrome RGBA image data from cache or by capturing a screenshot.
///
/// # Errors
///
/// Returns an error if screenshot capture or image decoding fails.
async fn load_chrome_rgba(fixture: &Path, page: &Page) -> Result<RgbaImage> {
    read_or_fetch_cache(CacheFetcher {
        test_name: "graphics",
        fixture_path: fixture,
        cache_suffix: "_chrome.rgba.zst",
        fetch_fn: || async {
            let img = capture_screenshot_rgba(page, fixture, 784, 453).await?;
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

struct FixtureContext<'ctx> {
    fixture: &'ctx Path,
    failing_dir: &'ctx Path,
    page: &'ctx Page,
}

struct FailureArtifacts<'artifacts> {
    ctx: &'artifacts FixtureContext<'artifacts>,
    name: String,
    chrome_img: RgbaImage,
    valor_img: Vec<u8>,
    diff_img: Vec<u8>,
    width: u32,
    height: u32,
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
        write_png_rgba_if_changed(
            &chrome_path,
            self.chrome_img.as_raw(),
            self.width,
            self.height,
        )?;
        write_png_rgba_if_changed(&valor_path, &self.valor_img, self.width, self.height)?;
        write_png_rgba_if_changed(&diff_path, &self.diff_img, self.width, self.height)?;

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

    let chrome_img = load_chrome_rgba(ctx.fixture, ctx.page).await?;
    let (width, height) = (chrome_img.width(), chrome_img.height());

    let display_list = build_display_list_for_fixture(ctx.fixture, width, height).await?;
    let (valor_img, glyph_bounds) = rasterize_display_list_to_rgba(&display_list, width, height)?;

    if chrome_img.as_raw() == &valor_img {
        return Ok(false);
    }

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
        let artifacts = FailureArtifacts {
            ctx,
            name: name.clone(),
            chrome_img,
            valor_img,
            diff_img,
            width,
            height,
        };
        artifacts.write_to_disk()?;
        return Ok(true);
    }

    Ok(false)
}

/// Runs a single graphics test for a given fixture with a provided page.
///
/// # Errors
///
/// Returns an error if rendering or comparison fails.
pub async fn run_single_graphics_test_with_page(fixture_path: &Path, page: &Page) -> Result<()> {
    let failing_dir = test_failing_dir("graphics")?;

    let result = process_single_fixture(&FixtureContext {
        fixture: fixture_path,
        failing_dir: &failing_dir,
        page,
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
