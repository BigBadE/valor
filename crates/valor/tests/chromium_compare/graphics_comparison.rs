//! Graphics comparison implementation using the unified framework.

use super::chrome::capture_screenshot_rgba;
use super::common::write_png_rgba_if_changed;
use super::comparison_framework::ComparisonTest;
use super::valor::{build_display_list_for_fixture, rasterize_display_list_to_rgba};
use anyhow::Result;
use chromiumoxide::page::Page;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::runtime::Handle;
use wgpu_backend::GlyphBounds;

/// Graphics-specific metadata (dimensions, glyph bounds)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphicsMetadata {
    pub width: u32,
    pub height: u32,
    #[serde(skip)]
    pub glyph_bounds: Vec<GlyphBounds>,
}

impl Default for GraphicsMetadata {
    fn default() -> Self {
        Self {
            width: 784,
            height: 453,
            glyph_bounds: Vec::new(),
        }
    }
}

/// Graphics comparison result
#[derive(Debug, Clone, Serialize)]
pub struct GraphicsCompareResult {
    pub total_pixels: u64,
    pub text_pixels: u64,
    pub non_text_pixels: u64,
    pub failed_text_pixels: u64,
    pub failed_non_text_pixels: u64,
    pub text_fail_ratio: f64,
    pub non_text_fail_ratio: f64,
    pub overall_fail_ratio: f64,
}

/// Raw RGBA image data with dimensions
#[derive(Debug, Clone)]
pub struct RgbaImageData {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl From<image::RgbaImage> for RgbaImageData {
    fn from(img: image::RgbaImage) -> Self {
        Self {
            width: img.width(),
            height: img.height(),
            pixels: img.into_raw(),
        }
    }
}

/// Graphics comparison test implementation
pub struct GraphicsComparison;

impl ComparisonTest for GraphicsComparison {
    type ChromeOutput = RgbaImageData;
    type ValorOutput = RgbaImageData;
    type Metadata = GraphicsMetadata;
    type CompareResult = GraphicsCompareResult;

    fn test_name() -> &'static str {
        "graphics"
    }

    async fn fetch_chrome_output(
        page: &Page,
        fixture: &Path,
        metadata: &Self::Metadata,
    ) -> Result<Self::ChromeOutput> {
        let img = capture_screenshot_rgba(page, fixture, metadata.width, metadata.height).await?;
        Ok(img.into())
    }

    async fn generate_valor_output(
        _handle: &Handle,
        fixture: &Path,
        metadata: &mut Self::Metadata,
    ) -> Result<Self::ValorOutput> {
        let display_list =
            build_display_list_for_fixture(fixture, metadata.width, metadata.height).await?;
        let (pixels, glyph_bounds) =
            rasterize_display_list_to_rgba(&display_list, metadata.width, metadata.height)?;

        // Store glyph bounds in metadata for comparison
        metadata.glyph_bounds = glyph_bounds;

        Ok(RgbaImageData {
            width: metadata.width,
            height: metadata.height,
            pixels,
        })
    }

    fn compare(
        chrome: &Self::ChromeOutput,
        valor: &Self::ValorOutput,
        metadata: &Self::Metadata,
    ) -> Result<Self::CompareResult, String> {
        if chrome.width != valor.width || chrome.height != valor.height {
            return Err(format!(
                "Image dimensions mismatch: Chrome {}x{}, Valor {}x{}",
                chrome.width, chrome.height, valor.width, valor.height
            ));
        }

        let width = chrome.width;
        let height = chrome.height;

        // Count pixel failures
        let mut total_pixels = 0u64;
        let mut text_pixels = 0u64;
        let mut non_text_pixels = 0u64;
        let mut failed_text_pixels = 0u64;
        let mut failed_non_text_pixels = 0u64;

        for y in 0..height {
            for x in 0..width {
                let idx = ((y * width + x) * 4) as usize;
                total_pixels += 1;

                let is_text = is_pixel_in_text_region(x, y, &metadata.glyph_bounds);

                if is_text {
                    text_pixels += 1;
                } else {
                    non_text_pixels += 1;
                }

                let matches =
                    compare_pixel_with_thresholds(&chrome.pixels, &valor.pixels, idx, is_text);

                if !matches {
                    if is_text {
                        failed_text_pixels += 1;
                    } else {
                        failed_non_text_pixels += 1;
                    }
                }
            }
        }

        let text_fail_ratio = if text_pixels > 0 {
            (failed_text_pixels as f64) / (text_pixels as f64)
        } else {
            0.0
        };

        let non_text_fail_ratio = if non_text_pixels > 0 {
            (failed_non_text_pixels as f64) / (non_text_pixels as f64)
        } else {
            0.0
        };

        let overall_fail_ratio =
            ((failed_text_pixels + failed_non_text_pixels) as f64) / (total_pixels as f64);

        let result = GraphicsCompareResult {
            total_pixels,
            text_pixels,
            non_text_pixels,
            failed_text_pixels,
            failed_non_text_pixels,
            text_fail_ratio,
            non_text_fail_ratio,
            overall_fail_ratio,
        };

        // Fail if any pixels differ
        if failed_text_pixels > 0 || failed_non_text_pixels > 0 {
            Err(format!(
                "Pixel differences found:\n  \
                Text pixels: {}/{} failed ({:.2}%)\n  \
                Non-text pixels: {}/{} failed ({:.2}%)\n  \
                Overall: {:.2}% pixels differ",
                failed_text_pixels,
                text_pixels,
                text_fail_ratio * 100.0,
                failed_non_text_pixels,
                non_text_pixels,
                non_text_fail_ratio * 100.0,
                overall_fail_ratio * 100.0
            ))
        } else {
            Ok(result)
        }
    }

    fn serialize_chrome(output: &Self::ChromeOutput) -> Result<Vec<u8>> {
        // Use zstd compression for RGBA data
        let compressed = zstd::bulk::compress(&output.pixels, 1)?;
        Ok(compressed)
    }

    fn deserialize_chrome(bytes: &[u8]) -> Result<Self::ChromeOutput> {
        // Decompress zstd data
        let expected = (784u32 * 453u32 * 4) as usize;
        let decompressed = zstd::bulk::decompress(bytes, expected)?;

        Ok(RgbaImageData {
            width: 784,
            height: 453,
            pixels: decompressed,
        })
    }

    fn write_chrome_output(output: &Self::ChromeOutput, base_path: &Path) -> Result<()> {
        let path = base_path.with_extension("chrome.png");
        write_png_rgba_if_changed(&path, &output.pixels, output.width, output.height)?;
        Ok(())
    }

    fn write_valor_output(output: &Self::ValorOutput, base_path: &Path) -> Result<()> {
        let path = base_path.with_extension("valor.png");
        write_png_rgba_if_changed(&path, &output.pixels, output.width, output.height)?;
        Ok(())
    }

    fn write_diff(
        chrome: &Self::ChromeOutput,
        valor: &Self::ValorOutput,
        metadata: &Self::Metadata,
        base_path: &Path,
    ) -> Result<()> {
        let path = base_path.with_extension("diff.png");
        write_diff_image(chrome, valor, &metadata.glyph_bounds, &path)?;
        Ok(())
    }
}

/// Generate a visual diff image showing pixel differences.
///
/// Creates a diff image where:
/// - Red pixels = failed comparison (text or non-text)
/// - Black pixels = passed comparison
pub fn write_diff_image(
    chrome: &RgbaImageData,
    valor: &RgbaImageData,
    glyph_bounds: &[GlyphBounds],
    path: &Path,
) -> Result<()> {
    if chrome.width != valor.width || chrome.height != valor.height {
        return Err(anyhow::anyhow!("Image dimensions mismatch"));
    }

    let width = chrome.width;
    let height = chrome.height;
    let mut diff_img = vec![0u8; (width * height * 4) as usize];

    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * 4) as usize;
            let is_text = is_pixel_in_text_region(x, y, glyph_bounds);
            let matches = compare_pixel_with_thresholds(&chrome.pixels, &valor.pixels, idx, is_text);

            if idx + 3 < diff_img.len() {
                diff_img[idx] = if matches { 0 } else { 255 }; // R
                diff_img[idx + 1] = 0; // G
                diff_img[idx + 2] = 0; // B
                diff_img[idx + 3] = 255; // A
            }
        }
    }

    write_png_rgba_if_changed(path, &diff_img, width, height)?;
    Ok(())
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
