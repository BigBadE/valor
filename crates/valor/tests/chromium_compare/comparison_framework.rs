//! Unified trait-based comparison framework for Chrome vs Valor testing.
//!
//! This module provides a generic system for comparing rendering outputs across
//! layout, graphics, and text rendering tests, with structured failure reporting.

use anyhow::Result;
use chromiumoxide::page::Page;
use serde::Serialize;
use std::fmt::Debug;
use std::fs::{create_dir_all, write};
use std::path::{Path, PathBuf};
use tokio::runtime::Handle;

use super::cache_utils::{CacheFetcher, read_or_fetch_cache};
use super::common::target_dir;

/// Core trait for comparison test implementations.
///
/// Implementors define how to:
/// - Fetch/generate Chrome output
/// - Generate Valor output
/// - Compare the outputs
/// - Write debug artifacts in the appropriate format (JSON vs PNG)
pub trait ComparisonTest: Sized {
    /// The type of Chrome's output (e.g., `JsonValue`, `RgbaImage`)
    type ChromeOutput: Clone + Debug;

    /// The type of Valor's output (e.g., `JsonValue`, `Vec<u8>`)
    type ValorOutput: Clone + Debug;

    /// Metadata attached to the comparison (e.g., viewport dimensions, glyph bounds)
    type Metadata: Default + Clone + Debug;

    /// The result type for comparison (e.g., diff percentage, pixel counts)
    type CompareResult: Debug + Serialize;

    /// The name of this test type (e.g., "layout", "graphics", "text_rendering")
    fn test_name() -> &'static str;

    /// Fetches or generates Chrome output for the fixture.
    ///
    /// # Errors
    ///
    /// Returns an error if Chrome interaction or output generation fails.
    async fn fetch_chrome_output(
        page: &Page,
        fixture: &Path,
        metadata: &Self::Metadata,
    ) -> Result<Self::ChromeOutput>;

    /// Generates Valor output for the fixture.
    ///
    /// # Errors
    ///
    /// Returns an error if Valor rendering or serialization fails.
    async fn generate_valor_output(
        handle: &Handle,
        fixture: &Path,
        metadata: &mut Self::Metadata,
    ) -> Result<Self::ValorOutput>;

    /// Compares Chrome and Valor outputs.
    ///
    /// # Errors
    ///
    /// Returns `Ok(result)` with comparison metrics on success.
    /// Returns `Err(msg)` if comparison fails.
    fn compare(
        chrome: &Self::ChromeOutput,
        valor: &Self::ValorOutput,
        metadata: &Self::Metadata,
    ) -> Result<Self::CompareResult, String>;

    /// Serializes Chrome output to bytes for caching.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    fn serialize_chrome(output: &Self::ChromeOutput) -> Result<Vec<u8>>;

    /// Deserializes Chrome output from cached bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization fails.
    fn deserialize_chrome(bytes: &[u8]) -> Result<Self::ChromeOutput>;

    /// Writes Chrome output to a debug file.
    ///
    /// # Errors
    ///
    /// Returns an error if file writing fails.
    fn write_chrome_output(output: &Self::ChromeOutput, path: &Path) -> Result<()>;

    /// Writes Valor output to a debug file.
    ///
    /// # Errors
    ///
    /// Returns an error if file writing fails.
    fn write_valor_output(output: &Self::ValorOutput, path: &Path) -> Result<()>;

    /// Optionally writes a diff visualization (e.g., diff.png for image tests).
    ///
    /// Default implementation does nothing (for JSON-based tests).
    ///
    /// # Errors
    ///
    /// Returns an error if file writing fails.
    fn write_diff(
        _chrome: &Self::ChromeOutput,
        _valor: &Self::ValorOutput,
        _metadata: &Self::Metadata,
        _base_path: &Path,
    ) -> Result<()> {
        Ok(()) // Default: no diff file
    }
}

/// Result of running a comparison test.
#[derive(Debug, Clone, Serialize)]
pub struct ComparisonOutcome<R> {
    /// Whether the test passed
    pub passed: bool,
    /// Comparison result metrics (if comparison succeeded)
    pub result: Option<R>,
    /// Error message (if comparison failed)
    pub error: Option<String>,
}

/// Manages test failure artifacts (separate Chrome/Valor outputs).
pub struct FailureArtifacts<T: ComparisonTest> {
    fixture_name: String,
    chrome_output: T::ChromeOutput,
    valor_output: T::ValorOutput,
    metadata: T::Metadata,
    error_msg: String,
}

impl<T: ComparisonTest> FailureArtifacts<T> {
    /// Creates a failing artifacts directory for this test type.
    ///
    /// # Errors
    ///
    /// Returns an error if directory creation fails.
    fn failing_dir() -> Result<PathBuf> {
        let dir = target_dir()
            .join("test_cache")
            .join(T::test_name())
            .join("failing");
        create_dir_all(&dir)?;
        Ok(dir)
    }

    /// Writes all failure artifacts to disk.
    ///
    /// Creates separate files using test-specific format:
    /// - Chrome output (format determined by trait implementation)
    /// - Valor output (format determined by trait implementation)
    /// - Diff visualization (optional, for image tests)
    /// - `{fixture}.error.txt` - Error message (only for non-image tests)
    ///
    /// # Errors
    ///
    /// Returns an error if file I/O fails.
    pub fn write(&self) -> Result<PathBuf> {
        let dir = Self::failing_dir()?;
        let base = dir.join(&self.fixture_name);

        // Write Chrome output using trait method
        T::write_chrome_output(&self.chrome_output, &base)?;

        // Write Valor output using trait method
        T::write_valor_output(&self.valor_output, &base)?;

        // Write diff visualization if provided by the test type
        T::write_diff(
            &self.chrome_output,
            &self.valor_output,
            &self.metadata,
            &base,
        )?;

        // Write error message only if no diff was written
        // (Check if a diff.png was created to determine this)
        let diff_path = base.with_extension("diff.png");
        if !diff_path.exists() {
            write(base.with_extension("error.txt"), &self.error_msg)?;
        }

        Ok(dir)
    }
}

/// Runs a single comparison test for a fixture.
///
/// This is the main entry point for the unified comparison framework.
///
/// # Workflow
///
/// 1. Fetch/cache Chrome output
/// 2. Generate Valor output
/// 3. Compare outputs
/// 4. On failure: save separate Chrome/Valor JSON files for debugging
///
/// # Errors
///
/// Returns an error if test infrastructure fails (not if comparison fails).
pub async fn run_comparison_test<T: ComparisonTest>(
    page: &Page,
    handle: &Handle,
    fixture: &Path,
) -> Result<ComparisonOutcome<T::CompareResult>> {
    let fixture_name = fixture
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    let mut metadata = T::Metadata::default();

    // Fetch Chrome output (with caching)
    let chrome_output = read_or_fetch_cache(CacheFetcher {
        test_name: T::test_name(),
        fixture_path: fixture,
        cache_suffix: "_chrome.cache",
        fetch_fn: || T::fetch_chrome_output(page, fixture, &metadata),
        deserialize_fn: T::deserialize_chrome,
        serialize_fn: T::serialize_chrome,
    })
    .await?;

    // Generate Valor output
    let valor_output = T::generate_valor_output(handle, fixture, &mut metadata).await?;

    // Compare outputs
    match T::compare(&chrome_output, &valor_output, &metadata) {
        Ok(result) => Ok(ComparisonOutcome {
            passed: true,
            result: Some(result),
            error: None,
        }),
        Err(error_msg) => {
            // Save failure artifacts
            let artifacts = FailureArtifacts::<T> {
                fixture_name: fixture_name.clone(),
                chrome_output,
                valor_output,
                metadata,
                error_msg: error_msg.clone(),
            };

            let dir = artifacts.write()?;

            let full_error = format!(
                "{}\n\nDebug artifacts saved to:\n  {}",
                error_msg,
                dir.display()
            );

            Ok(ComparisonOutcome {
                passed: false,
                result: None,
                error: Some(full_error),
            })
        }
    }
}

/// Runs a comparison test and returns a simple pass/fail result.
///
/// # Errors
///
/// Returns an error if the test fails or infrastructure errors occur.
pub async fn run_comparison_test_simple<T: ComparisonTest>(
    page: &Page,
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
