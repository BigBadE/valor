// Fixture test runner - only used by chromium_tests.rs (generated tests)
// Uses the unified comparison framework for all test types

use super::chrome::start_and_connect_chrome;
use super::common::{init_test_logger, target_dir};
use super::comparison_framework::{ComparisonTest, run_comparison_test};
use super::graphics_comparison::GraphicsComparison;
use super::layout_comparison::LayoutComparison;
use anyhow::{Result, anyhow};
use chromiumoxide::{Browser, Page};
use log::warn;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::runtime::Handle;

/// Runs a comparison test and returns a simple pass/fail result.
///
/// # Errors
///
/// Returns an error if the test fails or infrastructure errors occur.
async fn run_comparison_test_simple<T: ComparisonTest>(
    page: Option<&Page>,
    handle: &Handle,
    fixture: &Path,
) -> Result<()> {
    let outcome = run_comparison_test::<T>(page, handle, fixture).await?;

    if outcome.passed {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "Comparison test failed:\n{}",
            outcome.error.unwrap_or_default()
        ))
    }
}

struct FixtureResult {
    path: PathBuf,
    layout_passed: bool,
    graphics_passed: bool,
    layout_error: Option<String>,
    graphics_error: Option<String>,
    _duration: Duration,
}

/// Creates a failure result for page creation errors.
fn create_page_error_result(
    fixture_path: &Path,
    error_msg: String,
    duration: Duration,
) -> FixtureResult {
    FixtureResult {
        path: fixture_path.to_path_buf(),
        layout_passed: false,
        graphics_passed: false,
        layout_error: Some(error_msg.clone()),
        graphics_error: Some(error_msg),
        _duration: duration,
    }
}

/// Runs layout and graphics comparison tests for a fixture.
async fn run_layout_and_graphics_tests(
    page: Option<&Page>,
    fixture_path: &Path,
    fixture_start: Instant,
) -> Result<FixtureResult> {
    let handle = Handle::current();

    let layout_result =
        run_comparison_test_simple::<LayoutComparison>(page, &handle, fixture_path).await;
    let layout_passed = layout_result.is_ok();
    let layout_error = layout_result.as_ref().err().map(|err| err.to_string());

    // If layout hit an infrastructure error, fail immediately
    if let Err(e) = layout_result {
        if !e.to_string().contains("Debug artifacts saved to:") {
            return Err(e.context(format!(
                "Layout infrastructure error for {}",
                fixture_path.display()
            )));
        }
    }

    let graphics_result =
        run_comparison_test_simple::<GraphicsComparison>(page, &handle, fixture_path).await;
    let graphics_passed = graphics_result.is_ok();
    let graphics_error = graphics_result.as_ref().err().map(|err| err.to_string());

    // If graphics hit an infrastructure error, fail immediately
    if let Err(e) = graphics_result {
        if !e.to_string().contains("Debug artifacts saved to:") {
            return Err(e.context(format!(
                "Graphics infrastructure error for {}",
                fixture_path.display()
            )));
        }
    }

    Ok(FixtureResult {
        path: fixture_path.to_path_buf(),
        layout_passed,
        graphics_passed,
        layout_error,
        graphics_error,
        _duration: fixture_start.elapsed(),
    })
}

/// Processes a single fixture test with both layout and graphics tests using the unified framework.
///
/// # Errors
///
/// Returns a result with pass/fail status and any error messages.
async fn process_fixture(
    idx: usize,
    _total_fixtures: usize,
    fixture_path: &Path,
) -> Result<FixtureResult> {
    let fixture_start = Instant::now();

    // Benchmark: Log start for profiling
    if idx % 20 == 0 {
        eprintln!("[{}] Starting fixture {}", idx, fixture_path.display());
    }

    // Skip known-failing fixtures
    if should_skip_fixture(fixture_path) {
        return Ok(FixtureResult {
            path: fixture_path.to_path_buf(),
            layout_passed: true,
            graphics_passed: true,
            layout_error: None,
            graphics_error: None,
            _duration: fixture_start.elapsed(),
        });
    }

    // All caches exist, so we don't need Chrome at all
    // Run tests without any browser page
    let result = run_layout_and_graphics_tests(None, fixture_path, fixture_start).await;

    // Benchmark: Log completion for profiling
    if idx % 20 == 0 {
        eprintln!("[{}] Completed in {:?}", idx, fixture_start.elapsed());
    }

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

                // Show infrastructure errors
                if let Some(ref error) = result.layout_error {
                    if error.contains("Failed to") || error.contains("error:") {
                        warn!(
                            "      layout error: {}",
                            error.lines().next().unwrap_or(error)
                        );
                    }
                }
                if let Some(ref error) = result.graphics_error {
                    if error.contains("Failed to") || error.contains("error:") {
                        warn!(
                            "      graphics error: {}",
                            error.lines().next().unwrap_or(error)
                        );
                    }
                }
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

    // Check if all fixtures have cached Chrome results
    use super::cache_utils::cache_exists;
    let mut cached_count = 0;
    let mut missing_count = 0;

    for fixture in fixtures {
        let layout_cached = cache_exists("layout", fixture, "_chrome.cache").unwrap_or(false);
        let graphics_cached = cache_exists("graphics", fixture, "_chrome.cache").unwrap_or(false);
        if layout_cached && graphics_cached {
            cached_count += 1;
        } else {
            missing_count += 1;
            if missing_count <= 3 {
                eprintln!(
                    "Missing cache for: {} (layout={}, graphics={})",
                    fixture.display(),
                    layout_cached,
                    graphics_cached
                );
            }
        }
    }

    let all_cached = missing_count == 0;
    eprintln!(
        "=== CACHE STATUS: {}/{} cached, {} missing ===",
        cached_count,
        fixtures.len(),
        missing_count
    );

    if all_cached {
        eprintln!("=== ALL CACHED - SKIPPING CHROME ===");
    } else {
        eprintln!("=== STARTING CHROME (caches incomplete) ===");
    }

    // Start and connect to Chrome only if needed
    // Wrap in Arc so it can be shared across parallel tasks
    use std::sync::Arc;
    let browser_with_handler = if !all_cached {
        Some(Arc::new(start_and_connect_chrome().await?))
    } else {
        None
    };
    let _connect_time = total_start.elapsed();

    let total_fixtures = fixtures.len();

    // PARALLEL EXECUTION NOW ENABLED!
    //
    // Now using WGPU's headless rendering which doesn't require windows or event loops.
    // Tests can run in parallel across all CPU cores with no synchronization bottlenecks.
    //
    // ARCHITECTURAL IMPROVEMENTS COMPLETED:
    // ✓ Removed global RENDER_STATE mutex that was forcing all tests to share one render state
    // ✓ Each test creates its own isolated headless GPU context
    // ✓ No window creation needed - pure offscreen rendering
    // ✓ No X11/Wayland constraints - works on all platforms
    // ✓ True parallel execution across all CPU cores
    //
    // PERFORMANCE EXPECTATIONS:
    // - Before refactoring: ~163s sequential (1 core)
    // - After headless + parallel: ~10-20s on 16-core machine (expected 8-16x speedup)
    // - Each test creates its own GPU context in parallel
    // - GPU work can be dispatched concurrently

    // TRUE PARALLEL EXECUTION WITH TOKIO::SPAWN
    // Now that tracing spans are fixed, we can use tokio::spawn for true parallelism
    // Each test runs on its own async task, scheduled across all tokio worker threads
    // GPU rendering uses spawn_blocking (in graphics_comparison) for CPU-bound work

    // Run tests with buffer_unordered for cooperative concurrency
    // Use 3x CPU count to maximize throughput since tests involve IO and blocking ops
    use futures::stream::{self, StreamExt as _};

    let concurrency = num_cpus::get() * 3;
    eprintln!(
        "=== Running {} tests with concurrency={} ===",
        total_fixtures, concurrency
    );

    let results: Vec<FixtureResult> = stream::iter(fixtures.iter().enumerate())
        .map(|(idx, fixture_path)| async move {
            process_fixture(idx, total_fixtures, fixture_path).await
        })
        .buffer_unordered(concurrency)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>>>()?;

    let total_duration = total_start.elapsed();

    // Count results
    let layout_passed = results.iter().filter(|r| r.layout_passed).count();
    let graphics_passed = results.iter().filter(|r| r.graphics_passed).count();
    let total = results.len();

    eprintln!("\n=== TEST SUMMARY ===");
    eprintln!("Total fixtures: {}", total);
    eprintln!("Layout passed: {}/{}", layout_passed, total);
    eprintln!("Graphics passed: {}/{}", graphics_passed, total);
    eprintln!("Time: {:?}", total_duration);

    print_summary(&results, total_duration);

    // Chrome will be automatically stopped when browser_with_handler is dropped

    // Fail the test if any fixtures failed
    let failed_count = results
        .iter()
        .filter(|result| !result.layout_passed || !result.graphics_passed)
        .count();

    if failed_count > 0 {
        eprintln!("Failed count: {}", failed_count);
        return Err(anyhow!(
            "{failed_count} fixture(s) failed (see summary above)"
        ));
    }

    Ok(())
}
