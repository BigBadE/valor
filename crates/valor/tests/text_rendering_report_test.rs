// Separate test file for text rendering report to avoid EventLoop conflicts
//
// This test must be run separately from chromium_tests because both create EventLoops
// which is not allowed on Windows.
//
// Run with: cargo test --package valor --test text_rendering_report_test

#[path = "chromium_compare"]
mod chromium_compare {
    pub mod cache_utils;
    pub mod chrome;
    pub mod common;
    pub mod comparison_framework;
    pub mod text_rendering_comparison;
    pub mod valor;
}

use anyhow::Result;
use chromium_compare::chrome::start_and_connect_chrome;
use chromium_compare::common::{init_test_logger, test_cache_dir};
use chromium_compare::comparison_framework::run_comparison_test;
use chromium_compare::text_rendering_comparison::TextRenderingComparison;
use chromiumoxide::page::Page;
use log::info;
use serde_json::to_string_pretty;
use std::env;
use std::fs::write;
use std::path::PathBuf;
use tokio::runtime::Handle;

/// Runs text rendering comparison between Chrome and Valor using the unified framework.
///
/// # Errors
///
/// Returns an error if comparison or artifact generation fails.
async fn run_text_rendering_comparison(page: &Page) -> Result<serde_json::Value> {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let fixture_path = workspace_root.join("crates/valor/tests/fixtures/text_render_matrix.html");

    info!("\n=== Text Rendering Comparison ===\n");

    let handle = Handle::current();
    let outcome =
        run_comparison_test::<TextRenderingComparison>(page, &handle, &fixture_path).await?;

    if let Some(result) = &outcome.result {
        info!("Text rendering comparison completed:");
        info!("  Total pixels: {}", result.total_pixels);
        info!(
            "  Text pixels: {} ({:.2}%)",
            result.text_pixels,
            (result.text_pixels as f64 / result.total_pixels as f64) * 100.0
        );
        info!(
            "  Text diff pixels: {} ({:.4}%)",
            result.text_diff_pixels,
            result.text_diff_ratio * 100.0
        );
        info!(
            "  Non-text diff pixels: {} ({:.4}%)",
            result.non_text_diff_pixels,
            result.non_text_diff_ratio * 100.0
        );
        info!("  Overall diff: {:.4}%", result.overall_diff_ratio * 100.0);
        info!("  Max channel diff: {}/255", result.max_channel_diff);
        info!("  Mean channel diff: {:.2}", result.mean_channel_diff);
        info!("  StdDev channel diff: {:.2}", result.stddev_channel_diff);
    }

    if let Some(error) = &outcome.error {
        info!("Comparison error: {error}");
    }

    Ok(serde_json::to_value(&outcome)?)
}

/// Generates and logs a text rendering comparison report.
///
/// # Errors
///
/// Returns an error if comparison fails or report generation fails.
async fn print_text_rendering_report(page: &Page) -> Result<()> {
    let results = run_text_rendering_comparison(page).await?;

    info!("\n=== TEXT RENDERING ANALYSIS REPORT ===\n");

    let out_dir = test_cache_dir("text_rendering")?;
    let report_path = out_dir.join("report.json");
    write(report_path, to_string_pretty(&results)?)?;

    info!("\nReport saved to: test_cache/text_rendering/");
    info!("  - report.json: Full comparison results");
    info!("\nFor detailed chrome/valor outputs, see:");
    info!("  - test_cache/text_rendering/failing/*.chrome.json");
    info!("  - test_cache/text_rendering/failing/*.valor.json");
    info!("  - test_cache/text_rendering/failing/*.metadata.json");

    // Assert that the test passed
    let passed = results
        .get("passed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !passed {
        let error_msg = results
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown comparison error");
        anyhow::bail!("Text rendering comparison failed:\n{}", error_msg);
    }

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
