//! Layout comparison implementation using the unified framework.

use super::comparison_framework::ComparisonTest;
use super::json_compare::compare_json_with_epsilon;
use super::layout_tests::{
    chromium_extraction::chromium_layout_json_in_page, serialization::serialize_valor_layout,
    setup::setup_page_for_fixture,
};
use anyhow::Result;
use chromiumoxide::page::Page;
use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue, from_slice, to_string_pretty, to_vec_pretty};
use std::fs::write;
use std::path::Path;
use tokio::runtime::Handle;

// Import Value methods for use with method syntax
use serde_json::Value;

/// Layout-specific metadata (viewport dimensions, epsilon tolerance)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutMetadata {
    pub epsilon: f64,
}

impl Default for LayoutMetadata {
    fn default() -> Self {
        Self { epsilon: 0.1 }
    }
}

/// Layout comparison result
#[derive(Debug, Clone, Serialize)]
pub struct LayoutCompareResult {
    pub js_assertions_passed: usize,
    pub js_assertions_failed: Vec<String>,
}

/// Layout comparison test implementation
pub struct LayoutComparison;

impl ComparisonTest for LayoutComparison {
    type ChromeOutput = JsonValue;
    type ValorOutput = JsonValue;
    type Metadata = LayoutMetadata;
    type CompareResult = LayoutCompareResult;

    fn test_name() -> &'static str {
        "layout"
    }

    async fn fetch_chrome_output(
        page: &Page,
        fixture: &Path,
        _metadata: &Self::Metadata,
    ) -> Result<Self::ChromeOutput> {
        chromium_layout_json_in_page(page, fixture).await
    }

    async fn generate_valor_output(
        handle: &Handle,
        fixture: &Path,
        _metadata: &mut Self::Metadata,
    ) -> Result<Self::ValorOutput> {
        let mut valor_page = setup_page_for_fixture(handle, fixture).await?;
        // Inject CSS reset AFTER parsing completes to ensure correct source order
        super::common::inject_css_reset_after_parsing(&mut valor_page).await?;
        serialize_valor_layout(&mut valor_page)
    }

    fn compare(
        chrome: &Self::ChromeOutput,
        valor: &Self::ValorOutput,
        metadata: &Self::Metadata,
    ) -> Result<Self::CompareResult, String> {
        // Check JavaScript assertions first
        let mut js_failed = Vec::new();
        let mut js_passed = 0usize;

        if let Some(asserts) = chrome.get("asserts").and_then(|value| value.as_array()) {
            for entry in asserts {
                let assert_name = entry
                    .get("name")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                let assertion_passed = entry.get("ok").and_then(Value::as_bool).unwrap_or(false);
                let assert_details = entry
                    .get("details")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");

                if assertion_passed {
                    js_passed += 1;
                } else {
                    js_failed.push(format!("{assert_name}: {assert_details}"));
                }
            }
        }

        // Compare layout JSON (actual=valor, expected=chrome)
        compare_json_with_epsilon(valor, chrome, metadata.epsilon)?;

        Ok(LayoutCompareResult {
            js_assertions_passed: js_passed,
            js_assertions_failed: js_failed,
        })
    }

    fn serialize_chrome(output: &Self::ChromeOutput) -> Result<Vec<u8>> {
        Ok(to_vec_pretty(output)?)
    }

    fn deserialize_chrome(bytes: &[u8]) -> Result<Self::ChromeOutput> {
        Ok(from_slice(bytes)?)
    }

    fn write_chrome_output(output: &Self::ChromeOutput, path: &Path) -> Result<()> {
        let chrome_path = path.with_extension("chrome.json");
        write(chrome_path, to_string_pretty(output)?)?;
        Ok(())
    }

    fn write_valor_output(output: &Self::ValorOutput, path: &Path) -> Result<()> {
        let valor_path = path.with_extension("valor.json");
        write(valor_path, to_string_pretty(output)?)?;
        Ok(())
    }
}
