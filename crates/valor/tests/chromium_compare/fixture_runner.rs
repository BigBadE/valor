// Fixture test runner - only used by chromium_tests.rs (generated tests)
// Requires graphics_tests and layout_tests modules to be available

use super::chrome::start_and_connect_chrome;
use super::common::{init_test_logger, target_dir, test_cache_dir};
use super::{graphics_tests, layout_tests};
use anyhow::{Result, anyhow};
use chromiumoxide::Browser;
use log::warn;
use std::fs::remove_file;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

struct FixtureResult {
    path: PathBuf,
    layout_passed: bool,
    graphics_passed: bool,
    _layout_error: Option<String>,
    _graphics_error: Option<String>,
    _duration: Duration,
}

/// Processes a single fixture test with both layout and graphics tests.
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
            _layout_error: None,
            _graphics_error: None,
            _duration: fixture_start.elapsed(),
        };
    }

    // Create a new page for this fixture
    let page = match browser.new_page("about:blank").await {
        Ok(page) => page,
        Err(err) => {
            let error_msg = format!("Failed to create page: {err}");
            return FixtureResult {
                path: fixture_path.to_path_buf(),
                layout_passed: false,
                graphics_passed: false,
                _layout_error: Some(error_msg.clone()),
                _graphics_error: Some(error_msg),
                _duration: fixture_start.elapsed(),
            };
        }
    };

    // Run tests and ensure page cleanup even on error
    let result = async {
        // Run layout test
        let layout_result =
            layout_tests::run_single_layout_test_with_page(fixture_path, &page).await;
        let layout_passed = layout_result.is_ok();
        let layout_error = layout_result.err().map(|err| err.to_string());

        // Run graphics test
        let graphics_result =
            graphics_tests::run_single_graphics_test_with_page(fixture_path, &page).await;
        let graphics_passed = graphics_result.is_ok();
        let graphics_error = graphics_result.err().map(|err| err.to_string());

        let duration = fixture_start.elapsed();

        FixtureResult {
            path: fixture_path.to_path_buf(),
            layout_passed,
            graphics_passed,
            _layout_error: layout_error,
            _graphics_error: graphics_error,
            _duration: duration,
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
        .filter(|result| !result.layout_passed || !result.graphics_passed)
        .count();

    if failed > 0 {
        warn!("\n{failed} fixture(s) failed:");
        warn!("────────────────────────────────────────");

        for result in results {
            if !result.layout_passed || !result.graphics_passed {
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
        .filter(|result| !result.layout_passed || !result.graphics_passed)
        .count();

    if failed_count > 0 {
        return Err(anyhow!(
            "{failed_count} fixture(s) failed (see summary above)"
        ));
    }

    Ok(())
}
