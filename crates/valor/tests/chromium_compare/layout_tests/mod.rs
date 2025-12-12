//! Layout comparison tests against Chromium.

mod chromium_extraction;
mod serialization;
mod setup;

use super::cache_utils::{CacheFetcher, read_or_fetch_cache, test_failing_dir};
use super::chrome;
use super::common;
use super::json_compare::compare_json_with_epsilon;
use anyhow::Result;
use chromiumoxide::page::Page;
use log::info;
use serde_json::{Value as JsonValue, from_str, to_string};
use std::fs::write;
use std::path::Path;
use std::str::from_utf8;
use std::time::Instant;
use tokio::runtime::Handle;

use chromium_extraction::chromium_layout_json_in_page;
use serialization::serialize_valor_layout;
use setup::setup_page_for_fixture;

/// Process a single JS assertion entry from Chromium's test output.
fn process_assertion_entry(
    entry: &JsonValue,
    display_name: &str,
    failed: &mut Vec<(String, String)>,
) {
    let assert_name = entry.get("name").and_then(JsonValue::as_str).unwrap_or("");
    let assertion_passed = entry
        .get("ok")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    let assert_details = entry
        .get("details")
        .and_then(JsonValue::as_str)
        .unwrap_or("");
    if !assertion_passed {
        let msg = format!("JS assertion failed: {assert_name} - {assert_details}");
        failed.push((display_name.to_string(), msg));
    }
}

/// Check JavaScript assertions from Chromium test output.
fn check_js_assertions(
    ch_json: &JsonValue,
    display_name: &str,
    failed: &mut Vec<(String, String)>,
) {
    let Some(asserts) = ch_json.get("asserts") else {
        return;
    };
    let Some(arr) = asserts.as_array() else {
        return;
    };
    for entry in arr {
        process_assertion_entry(entry, display_name, failed);
    }
}

/// Fetches Chromium layout JSON for a given fixture.
///
/// # Errors
///
/// Returns an error if cache fetching or JSON parsing fails.
async fn fetch_chromium_layout(page: &Page, input_path: &Path) -> Result<JsonValue> {
    read_or_fetch_cache(CacheFetcher {
        test_name: "layout",
        fixture_path: input_path,
        cache_suffix: "_chromium.json",
        fetch_fn: || chromium_layout_json_in_page(page, input_path),
        deserialize_fn: |bytes| {
            let string = from_utf8(bytes)?;
            Ok(from_str(string)?)
        },
        serialize_fn: |value| Ok(to_string(value)?.into_bytes()),
    })
    .await
}

/// Save JSON output to failing test directory for debugging.
fn save_failing_json(_fixture_path: &Path, ch_json: &JsonValue, valor_json: &JsonValue) {
    if let Ok(failing_dir) = test_failing_dir("layout") {
        drop(write(
            failing_dir.join("chromium.json"),
            to_string(ch_json).unwrap_or_default(),
        ));
        drop(write(
            failing_dir.join("valor.json"),
            to_string(valor_json).unwrap_or_default(),
        ));
    }
}

/// Run a single layout test with a provided Chrome page.
///
/// # Errors
///
/// Returns an error if the layout test fails.
pub async fn run_single_layout_test_with_page(fixture_path: &Path, page: &Page) -> Result<()> {
    let handle = Handle::current();
    let display_name = fixture_path
        .file_name()
        .and_then(|file_name| file_name.to_str())
        .unwrap_or("unknown");

    let failed = compare_layout_single(page, &handle, fixture_path, display_name).await?;

    if failed.is_empty() {
        Ok(())
    } else {
        let errors: Vec<String> = failed.iter().map(|(_, msg)| msg.clone()).collect();
        Err(anyhow::anyhow!(
            "Layout test failed:\n{}",
            errors.join("\n")
        ))
    }
}

/// Compares Chromium and Valor layout for a single fixture.
///
/// # Errors
///
/// Returns an error if page setup, layout extraction, or comparison fails.
pub async fn compare_layout_single(
    chrome_page: &Page,
    handle: &Handle,
    fixture_path: &Path,
    display_name: &str,
) -> Result<Vec<(String, String)>> {
    let start = Instant::now();
    let mut failed: Vec<(String, String)> = Vec::new();

    // Setup Valor page
    let mut valor_page = setup_page_for_fixture(handle, fixture_path).await?;

    // Get Chromium layout
    let ch_json = fetch_chromium_layout(chrome_page, fixture_path).await?;

    // Check JS assertions first
    check_js_assertions(&ch_json, display_name, &mut failed);

    // Serialize Valor layout
    let valor_json = serialize_valor_layout(&mut valor_page)?;

    // Compare layouts
    let layout_diff = compare_json_with_epsilon(&ch_json, &valor_json, 0.1);

    if let Err(diff_msg) = layout_diff {
        failed.push((
            display_name.to_string(),
            format!("Layout mismatch:\n{diff_msg}"),
        ));
        save_failing_json(fixture_path, &ch_json, &valor_json);
    }

    let elapsed = start.elapsed();
    if failed.is_empty() {
        info!("  PASS {display_name} ({elapsed:?})");
    } else {
        info!("  FAIL {display_name} ({elapsed:?})");
        for (_name, msg) in &failed {
            info!("    {msg}");
        }
    }

    Ok(failed)
}

/// Macro to batch layout comparison tests.
#[macro_export]
macro_rules! layout_test_batch {
    ($test_name:ident: [$($fixture:expr),+ $(,)?]) => {
        #[tokio::test(flavor = "multi_thread")]
        async fn $test_name() -> Result<()> {
            use $crate::chromium_compare::chrome::setup_browser;
            use $crate::chromium_compare::layout_tests::compare_layout_single;
            use tokio::runtime::Handle;

            let handle = Handle::current();
            let (browser, mut chrome_pages) = setup_browser().await?;

            let mut all_failed: Vec<(String, String)> = Vec::new();
            let fixtures = vec![$($fixture),+];

            for fixture_path in fixtures {
                let display_name = fixture_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(fixture_path.to_str().unwrap_or("unknown"));

                let page = chrome_pages
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("No Chrome page available"))?;

                let failed = compare_layout_single(&page, &handle, fixture_path, display_name).await?;
                all_failed.extend(failed);

                chrome_pages.push(page);
            }

            drop(chrome_pages);
            drop(browser);

            if !all_failed.is_empty() {
                info!("\nFailed tests:");
                for (name, msg) in &all_failed {
                    info!("  {name}: {msg}");
                }
                anyhow::bail!("{} layout tests failed", all_failed.len());
            }

            Ok(())
        }
    };
}
