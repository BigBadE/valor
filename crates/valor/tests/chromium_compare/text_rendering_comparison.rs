//! Text rendering comparison implementation using the unified framework.

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

/// Text rendering-specific metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextRenderingMetadata {
    pub width: u32,
    pub height: u32,
    #[serde(skip)]
    pub glyph_bounds: Vec<GlyphBounds>,
}

impl Default for TextRenderingMetadata {
    fn default() -> Self {
        Self {
            width: 1400,
            height: 2000,
            glyph_bounds: Vec::new(),
        }
    }
}

/// Text rendering comparison result with detailed metrics
#[derive(Debug, Clone, Serialize)]
pub struct TextRenderingCompareResult {
    pub total_pixels: u64,
    pub text_pixels: u64,
    pub non_text_pixels: u64,
    pub text_diff_pixels: u64,
    pub non_text_diff_pixels: u64,
    pub text_diff_ratio: f64,
    pub non_text_diff_ratio: f64,
    pub overall_diff_ratio: f64,
    pub max_channel_diff: u16,
    pub mean_channel_diff: f64,
    pub stddev_channel_diff: f64,
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

/// Text rendering comparison test implementation
pub struct TextRenderingComparison;

impl ComparisonTest for TextRenderingComparison {
    type ChromeOutput = RgbaImageData;
    type ValorOutput = RgbaImageData;
    type Metadata = TextRenderingMetadata;
    type CompareResult = TextRenderingCompareResult;

    fn test_name() -> &'static str {
        "text_rendering"
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

        // Compute comprehensive diff metrics
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
                if idx + 3 >= chrome.pixels.len() || idx + 3 >= valor.pixels.len() {
                    continue;
                }

                total_pixels += 1;
                let is_text = is_pixel_in_text_region(x, y, &metadata.glyph_bounds);

                if is_text {
                    text_pixels += 1;
                } else {
                    non_text_pixels += 1;
                }

                let mut pixel_differs = false;
                for channel_offset in 0..4 {
                    let chrome_value = chrome.pixels[idx + channel_offset];
                    let valor_value = valor.pixels[idx + channel_offset];
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

        let text_diff_ratio = if text_pixels > 0 {
            (text_diff_pixels as f64) / (text_pixels as f64)
        } else {
            0.0
        };

        let non_text_diff_ratio = if non_text_pixels > 0 {
            (non_text_diff_pixels as f64) / (non_text_pixels as f64)
        } else {
            0.0
        };

        let overall_diff_ratio =
            ((text_diff_pixels + non_text_diff_pixels) as f64) / (total_pixels as f64);

        let result = TextRenderingCompareResult {
            total_pixels,
            text_pixels,
            non_text_pixels,
            text_diff_pixels,
            non_text_diff_pixels,
            text_diff_ratio,
            non_text_diff_ratio,
            overall_diff_ratio,
            max_channel_diff,
            mean_channel_diff,
            stddev_channel_diff,
        };

        // Fail if there are significant differences
        if text_diff_pixels > 0 || non_text_diff_pixels > 0 {
            Err(format!(
                "Text rendering differences found:\n  \
                Text pixels: {}/{} differ ({:.4}%)\n  \
                Non-text pixels: {}/{} differ ({:.4}%)\n  \
                Overall: {:.4}% pixels differ\n  \
                Max channel diff: {}/255\n  \
                Mean channel diff: {:.2}\n  \
                StdDev channel diff: {:.2}",
                text_diff_pixels,
                text_pixels,
                text_diff_ratio * 100.0,
                non_text_diff_pixels,
                non_text_pixels,
                non_text_diff_ratio * 100.0,
                overall_diff_ratio * 100.0,
                max_channel_diff,
                mean_channel_diff,
                stddev_channel_diff
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
        let expected = (1400u32 * 2000u32 * 4) as usize;
        let decompressed = zstd::bulk::decompress(bytes, expected)?;

        Ok(RgbaImageData {
            width: 1400,
            height: 2000,
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
/// - Red pixels = pixel differs between Chrome and Valor
/// - Black pixels = pixel matches
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
            if idx + 3 >= chrome.pixels.len() || idx + 3 >= valor.pixels.len() {
                continue;
            }

            // Check if any channel differs
            let mut differs = false;
            for channel_offset in 0..4 {
                if chrome.pixels[idx + channel_offset] != valor.pixels[idx + channel_offset] {
                    differs = true;
                    break;
                }
            }

            if idx + 3 < diff_img.len() {
                diff_img[idx] = if differs { 255 } else { 0 }; // R
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
