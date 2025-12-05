pub mod browser;
pub mod common;
pub mod graphics_tests;
pub mod json_compare;
pub mod layout_tests;

use anyhow::Result;

/// Runs layout tests first, then graphics tests only if layout passes.
///
/// # Errors
///
/// Returns an error if layout tests fail or if graphics tests fail (when layout passes).
pub fn run_chromium_tests() -> Result<()> {
    // Run layout tests first
    let result = layout_tests::run_chromium_layouts();

    // Run graphics tests
    let second = graphics_tests::chromium_graphics_smoke_compare_png();
    result?;
    second
}
