// Fixture test runner - only used by chromium_tests.rs (generated tests)
// Uses the unified comparison framework for all test types

use super::chrome::start_and_connect_chrome;
use super::common::{init_test_logger, target_dir, test_cache_dir};
use super::comparison_framework::run_comparison_test_simple;
use super::graphics_comparison::GraphicsComparison;
use super::layout_comparison::LayoutComparison;
use super::text_rendering_comparison::TextRenderingComparison;
use anyhow::{Result, anyhow};
use chromiumoxide::Browser;
use log::warn;
use std::fs::remove_file;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::runtime::Handle;

struct FixtureResult {
    path: PathBuf,
    layout_passed: bool,
    graphics_passed: bool,
    text_rendering_passed: bool,
    _layout_error: Option<String>,
    _graphics_error: Option<String>,
    _text_rendering_error: Option<String>,
    _duration: Duration,
}

/// Processes a single fixture test with both layout and graphics tests using the unified framework.
///
/// # Errors
///
/// Returns a result with pass/fail status and any error messages.
async fn process_fixture(
    _idx: usize,
    _total_fixtures: usize,
    fixture_path: &Path,
    browser: &Browser,
) -> FixtureResult {
    let fixture_start = Instant::now();

    // Skip known-failing fixtures
    if should_skip_fixture(fixture_path) {
        return FixtureResult {
            path: fixture_path.to_path_buf(),
            layout_passed: true,
            graphics_passed: true,
            text_rendering_passed: true,
            _layout_error: None,
            _graphics_error: None,
            _text_rendering_error: None,
            _duration: fixture_start.elapsed(),
        };
    }

    // Text rendering fixtures are NOT run as part of chromium_tests
    // Only text_render_matrix.html runs as a separate test (text_rendering_report_test)
    // The fixtures/text/ directory contains layout tests, not text rendering tests
    let is_text_rendering_fixture = false;

    // Create a new page for this fixture
    let page = match browser.new_page("about:blank").await {
        Ok(page) => page,
        Err(err) => {
            let error_msg = format!("Failed to create page: {err}");
            return FixtureResult {
                path: fixture_path.to_path_buf(),
                layout_passed: false,
                graphics_passed: false,
                text_rendering_passed: false,
                _layout_error: Some(error_msg.clone()),
                _graphics_error: Some(error_msg.clone()),
                _text_rendering_error: Some(error_msg),
                _duration: fixture_start.elapsed(),
            };
        }
    };

    // Run tests and ensure page cleanup even on error
    let result = async {
        let handle = Handle::current();

        // Run appropriate tests based on fixture type
        if is_text_rendering_fixture {
            // For text rendering fixtures, run only the text rendering test
            let text_rendering_result =
                run_comparison_test_simple::<TextRenderingComparison>(&page, &handle, fixture_path).await;
            let text_rendering_passed = text_rendering_result.is_ok();
            let text_rendering_error = text_rendering_result.err().map(|err| err.to_string());

            let duration = fixture_start.elapsed();

            FixtureResult {
                path: fixture_path.to_path_buf(),
                layout_passed: true, // Not tested for text rendering fixtures
                graphics_passed: true, // Not tested for text rendering fixtures
                text_rendering_passed,
                _layout_error: None,
                _graphics_error: None,
                _text_rendering_error: text_rendering_error,
                _duration: duration,
            }
        } else {
            // For regular fixtures, run layout and graphics tests
            let layout_result =
                run_comparison_test_simple::<LayoutComparison>(&page, &handle, fixture_path).await;
            let layout_passed = layout_result.is_ok();
            let layout_error = layout_result.err().map(|err| err.to_string());

            let graphics_result =
                run_comparison_test_simple::<GraphicsComparison>(&page, &handle, fixture_path).await;
            let graphics_passed = graphics_result.is_ok();
            let graphics_error = graphics_result.err().map(|err| err.to_string());

            let duration = fixture_start.elapsed();

            FixtureResult {
                path: fixture_path.to_path_buf(),
                layout_passed,
                graphics_passed,
                text_rendering_passed: true, // Not tested for regular fixtures
                _layout_error: layout_error,
                _graphics_error: graphics_error,
                _text_rendering_error: None,
                _duration: duration,
            }
        }
    }
    .await;

    // Always close the page to prevent Chrome tab accumulation
    let _ignore_close_error = page.close().await;

    result
}

/// Logs summary statistics for fixture test results.
fn print_summary(results: &[FixtureResult], _total_duration: Duration) {
    let failed = results
        .iter()
        .filter(|result| !result.layout_passed || !result.graphics_passed || !result.text_rendering_passed)
        .count();

    if failed > 0 {
        warn!("\n{failed} fixture(s) failed:");
        warn!("────────────────────────────────────────");

        for result in results {
            if !result.layout_passed || !result.graphics_passed || !result.text_rendering_passed {
                let name = result
                    .path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("unknown");

                let mut details = Vec::new();
                if !result.layout_passed {
                    details.push("layout");
                }
                if !result.graphics_passed {
                    details.push("graphics");
                }
                if !result.text_rendering_passed {
                    details.push("text_rendering");
                }

                let details_str = details.join(" ");
                warn!("  ✗ {name} {details_str}");
            }
        }
        warn!("────────────────────────────────────────");
    }
}

/// Checks if a fixture should be skipped based on known failures.
fn should_skip_fixture(_path: &Path) -> bool {
    // All layout tests should now pass with the CSS reset and epsilon fixes
    false
}

/// Cleans up old test artifacts before running tests.
///
/// # Errors
///
/// Returns an error if directory cleanup fails.
fn cleanup_old_artifacts() -> Result<()> {
    use std::fs::remove_dir_all;

    let base_dir = target_dir().join("test_cache");

    // Clean up graphics test diffs
    let graphics_failing_dir = base_dir.join("graphics").join("failing");
    if graphics_failing_dir.exists() {
        remove_dir_all(&graphics_failing_dir)?;
    }

    // Clean up layout test diffs
    let layout_failing_dir = base_dir.join("layout").join("failing");
    if layout_failing_dir.exists() {
        remove_dir_all(&layout_failing_dir)?;
    }

    // Clean up text rendering test artifacts
    let text_rendering_dir = test_cache_dir("text_rendering")?;
    if text_rendering_dir.exists() {
        // Remove old report and diff images
        let _ignore_report_remove = remove_file(text_rendering_dir.join("report.json"));
        let _ignore_diff_remove = remove_file(text_rendering_dir.join("diff.png"));
    }

    Ok(())
}

/// Runs all fixtures in a single test with a managed Chrome instance.
///
/// # Errors
///
/// Returns an error only if the test infrastructure fails, not if individual fixtures fail.
pub async fn run_all_fixtures(fixtures: &[PathBuf]) -> Result<()> {
    init_test_logger();

    // Clean up old test artifacts
    cleanup_old_artifacts()?;

    let total_start = Instant::now();

    // Start and connect to Chrome
    let browser_with_handler = start_and_connect_chrome().await?;
    let browser = &browser_with_handler.browser;
    let _connect_time = total_start.elapsed();

    let mut results = Vec::new();
    let total_fixtures = fixtures.len();

    for (idx, fixture_path) in fixtures.iter().enumerate() {
        let result = process_fixture(idx, total_fixtures, fixture_path, browser).await;
        results.push(result);
    }

    let total_duration = total_start.elapsed();
    print_summary(&results, total_duration);

    // Chrome will be automatically stopped when browser_with_handler is dropped

    // Fail the test if any fixtures failed
    let failed_count = results
        .iter()
        .filter(|result| !result.layout_passed || !result.graphics_passed || !result.text_rendering_passed)
        .count();

    if failed_count > 0 {
        return Err(anyhow!(
            "{failed_count} fixture(s) failed (see summary above)"
        ));
    }

    Ok(())
}
