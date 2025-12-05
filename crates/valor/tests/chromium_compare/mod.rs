pub mod browser;
pub mod common;
pub mod graphics_tests;
pub mod json_compare;
pub mod layout_tests;

use anyhow::Result;

/// Runs layout tests first, then graphics tests regardless of layout test results.
///
/// # Errors
///
/// Returns an error if layout tests fail or if graphics tests fail.
/// Both test suites always run to completion, but layout failures are reported first.
pub fn run_chromium_tests() -> Result<()> {
    // Run layout tests first
    let result = layout_tests::run_chromium_layouts();

    // Run graphics tests (always runs, even if layout tests failed)
    let second = graphics_tests::chromium_graphics_smoke_compare_png();

    // Report layout failures first if they occurred
    result?;

    // Then report graphics failures if they occurred
    second
}
