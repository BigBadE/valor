use super::browser::navigate_and_prepare_page;
use super::common::{
    clear_valor_layout_cache_if_harness_changed, create_page, css_reset_injection_script,
    ensure_chrome_installed, get_filtered_fixtures, init_test_logger,
    read_cached_json_for_fixture, to_file_url, update_until_finished,
    write_cached_json_for_fixture, write_named_json_for_fixture,
};
use super::json_compare::compare_json_with_epsilon;
use anyhow::{Result, anyhow};
use chromiumoxide::page::Page;
use css::style_types::{AlignItems, BoxSizing, ComputedStyle, Display, Overflow};
use css_core::{LayoutNodeKind, LayoutRect, Layouter};
use js::DOMSubscriber as _;
use js::DOMUpdate::{EndOfDocument, InsertElement, SetAttr};
use js::NodeKey;
use log::{error, info};
use serde_json::{Map as JsonMap, Value as JsonValue, from_str, json};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

type LayouterWithStyles = (Layouter, HashMap<NodeKey, ComputedStyle>);

#[derive(Default, Clone, Debug)]
struct FixtureTiming {
    page_creation: Duration,
    navigation: Duration,
    script_evaluation: Duration,
    json_parsing: Duration,
    setup_layouter: Duration,
    compute_geometry: Duration,
    json_comparison: Duration,
    total: Duration,
}

impl FixtureTiming {
    fn chromium_total(&self) -> Duration {
        self.page_creation + self.navigation + self.script_evaluation + self.json_parsing
    }

    fn valor_total(&self) -> Duration {
        self.setup_layouter + self.compute_geometry
    }
}

fn replay_into_layouter(
    layouter: &mut Layouter,
    tags_by_key: &HashMap<NodeKey, String>,
    element_children: &HashMap<NodeKey, Vec<NodeKey>>,
    attrs: &HashMap<NodeKey, HashMap<String, String>>,
    parent: NodeKey,
) {
    let Some(children) = element_children.get(&parent) else {
        return;
    };
    for child in children {
        let tag = tags_by_key
            .get(child)
            .cloned()
            .unwrap_or_else(|| "div".to_owned());
        let _ignore_result = layouter.apply_update(InsertElement {
            parent,
            node: *child,
            tag,
            pos: 0,
        });
        if let Some(attr_map) = attrs.get(child) {
            apply_element_attrs(layouter, *child, attr_map);
        }
        replay_into_layouter(layouter, tags_by_key, element_children, attrs, *child);
    }
}

fn apply_element_attrs(layouter: &mut Layouter, node: NodeKey, attrs: &HashMap<String, String>) {
    for key_name in ["id", "class", "style"] {
        if let Some(val) = attrs.get(key_name) {
            let _ignore_result = layouter.apply_update(SetAttr {
                node,
                name: key_name.to_owned(),
                value: val.clone(),
            });
        }
    }
}

/// Sets up a layouter for a fixture by creating a page and processing it.
///
/// # Errors
///
/// Returns an error if page creation, parsing, or layout computation fails.
async fn setup_layouter_for_fixture(
    runtime: &Runtime,
    input_path: &Path,
) -> Result<LayouterWithStyles> {
    let url = to_file_url(input_path)?;
    let mut page = create_page(runtime.handle(), url).await?;
    page.eval_js(css_reset_injection_script())?;
    let mut layouter_mirror = page.create_mirror(Layouter::new());

    let finished = update_until_finished(&mut page, |_page| {
        layouter_mirror.try_update_sync()?;
        Ok(())
    })
    .await?;

    if !finished {
        return Err(anyhow!("Parsing did not finish"));
    }

    page.update().await?;
    layouter_mirror.try_update_sync()?;

    let (tags_by_key, element_children) = page.layout_structure_snapshot();
    let attrs_map = page.layouter_attrs_map();
    {
        let layouter = layouter_mirror.mirror_mut();
        replay_into_layouter(
            layouter,
            &tags_by_key,
            &element_children,
            &attrs_map,
            NodeKey::ROOT,
        );
        let _ignore_result = layouter.apply_update(EndOfDocument);
    }

    let computed = page.computed_styles_snapshot()?;
    {
        let layouter = layouter_mirror.mirror_mut();
        let sheet_for_layout = page.styles_snapshot()?;
        layouter.set_stylesheet(sheet_for_layout);
        layouter.set_computed_styles(computed.clone());
        let _count = layouter.compute_layout();
    }

    Ok((layouter_mirror.into_inner(), computed))
}

/// Sets up a layouter using the current tokio handle (for parallel execution).
///
/// # Errors
///
/// Returns an error if page creation, parsing, or layout computation fails.
async fn setup_layouter_for_fixture_current(input_path: &Path) -> Result<LayouterWithStyles> {
    use super::common::create_page_from_current;
    let url = to_file_url(input_path)?;
    let mut page = create_page_from_current(url).await?;
    page.eval_js(css_reset_injection_script())?;
    let mut layouter_mirror = page.create_mirror(Layouter::new());

    let finished = update_until_finished(&mut page, |_page| {
        layouter_mirror.try_update_sync()?;
        Ok(())
    })
    .await?;

    if !finished {
        return Err(anyhow!("Parsing did not finish"));
    }

    page.update().await?;
    layouter_mirror.try_update_sync()?;

    let (tags_by_key, element_children) = page.layout_structure_snapshot();
    let attrs_map = page.layouter_attrs_map();
    {
        let layouter = layouter_mirror.mirror_mut();
        replay_into_layouter(
            layouter,
            &tags_by_key,
            &element_children,
            &attrs_map,
            NodeKey::ROOT,
        );
        let _ignore_result = layouter.apply_update(EndOfDocument);
    }

    let computed = page.computed_styles_snapshot()?;
    {
        let layouter = layouter_mirror.mirror_mut();
        let sheet_for_layout = page.styles_snapshot()?;
        layouter.set_stylesheet(sheet_for_layout);
        layouter.set_computed_styles(computed.clone());
        let _count = layouter.compute_layout();
    }

    Ok((layouter_mirror.into_inner(), computed))
}

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
        error!("[LAYOUT] {display_name} ... FAILED: {msg}");
        failed.push((display_name.to_string(), msg));
    }
}

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

/// Processes a single layout fixture for parallel execution using a provided page from pool.
///
/// # Errors
///
/// Returns an error if fixture processing, layouter setup, or JSON operations fail.
async fn process_layout_fixture_parallel_with_page(
    input_path: &Path,
    _browser: &Arc<chromiumoxide::Browser>,
    page: &chromiumoxide::Page,
    harness_src: &str,
    failed: &mut Vec<(String, String)>,
    timing: &mut FixtureTiming,
) -> Result<bool> {
    let display_name = input_path.display().to_string();
    let fixture_start = Instant::now();

    // Setup layouter
    let setup_start = Instant::now();
    let (mut layouter, computed_for_serialization) =
        match setup_layouter_for_fixture_current(input_path).await {
            Ok(result) => result,
            Err(err) => {
                let msg = format!("Setup failed: {err}");
                error!("[LAYOUT] {display_name} ... FAILED: {msg}");
                failed.push((display_name.clone(), msg));
                return Ok(false);
            }
        };
    timing.setup_layouter = setup_start.elapsed();

    // Compute geometry
    let geometry_start = Instant::now();
    let rects_external = layouter.compute_layout_geometry();
    let our_json = our_layout_json(&layouter, &rects_external, &computed_for_serialization);
    timing.compute_geometry = geometry_start.elapsed();

    // Fetch or retrieve Chromium JSON
    let ch_json = if let Some(cached_value) = read_cached_json_for_fixture(input_path, harness_src)
    {
        // Cache hit - no chromium timings to record
        cached_value
    } else {
        // Extract with detailed timing
        let (chromium_value, chrome_timing) = chromium_layout_json_in_page_with_timing(page, input_path).await?;
        timing.navigation = chrome_timing.navigation;
        timing.script_evaluation = chrome_timing.script_evaluation;
        timing.json_parsing = chrome_timing.json_parsing;
        write_cached_json_for_fixture(input_path, harness_src, &chromium_value)?;
        chromium_value
    };

    // Write JSON files and compare
    let comparison_start = Instant::now();
    write_named_json_for_fixture(input_path, harness_src, "chromium", &ch_json)?;
    write_named_json_for_fixture(input_path, harness_src, "valor", &our_json)?;
    check_js_assertions(&ch_json, &display_name, failed);

    let ch_layout_json = if ch_json.get("layout").is_some() || ch_json.get("asserts").is_some() {
        ch_json.get("layout").cloned().unwrap_or_else(|| json!({}))
    } else {
        ch_json.clone()
    };

    let eps = f64::from(f32::EPSILON) * 3.0;
    let result = match compare_json_with_epsilon(&our_json, &ch_layout_json, eps) {
        Ok(()) => {
            info!("[LAYOUT] {display_name} ... ok");
            Ok(true)
        }
        Err(msg) => {
            failed.push((display_name.clone(), msg));
            Ok(false)
        }
    };
    timing.json_comparison = comparison_start.elapsed();
    timing.total = fixture_start.elapsed();

    result
}

/// Processes a single layout fixture for parallel execution (uses current tokio handle).
///
/// # Errors
///
/// Returns an error if fixture processing, layouter setup, or JSON operations fail.
async fn process_layout_fixture_parallel(
    input_path: &Path,
    browser: &Arc<chromiumoxide::Browser>,
    harness_src: &str,
    failed: &mut Vec<(String, String)>,
    timing: &mut FixtureTiming,
) -> Result<bool> {
    let display_name = input_path.display().to_string();
    let fixture_start = Instant::now();

    // Setup layouter
    let setup_start = Instant::now();
    let (mut layouter, computed_for_serialization) =
        match setup_layouter_for_fixture_current(input_path).await {
            Ok(result) => result,
            Err(err) => {
                let msg = format!("Setup failed: {err}");
                error!("[LAYOUT] {display_name} ... FAILED: {msg}");
                failed.push((display_name.clone(), msg));
                return Ok(false);
            }
        };
    timing.setup_layouter = setup_start.elapsed();

    // Compute geometry
    let geometry_start = Instant::now();
    let rects_external = layouter.compute_layout_geometry();
    let our_json = our_layout_json(&layouter, &rects_external, &computed_for_serialization);
    timing.compute_geometry = geometry_start.elapsed();

    // Fetch or retrieve Chromium JSON (timing not tracked in detail for this function)
    let ch_json = if let Some(cached_value) = read_cached_json_for_fixture(input_path, harness_src)
    {
        cached_value
    } else {
        // Await directly - no block_on() to avoid blocking the event handler
        let page = browser.as_ref().new_page("about:blank").await?;
        let chromium_value = chromium_layout_json_in_page(&page, input_path).await?;
        page.close().await?;
        write_cached_json_for_fixture(input_path, harness_src, &chromium_value)?;
        chromium_value
    };

    // Write JSON files and compare
    let comparison_start = Instant::now();
    write_named_json_for_fixture(input_path, harness_src, "chromium", &ch_json)?;
    write_named_json_for_fixture(input_path, harness_src, "valor", &our_json)?;
    check_js_assertions(&ch_json, &display_name, failed);

    let ch_layout_json = if ch_json.get("layout").is_some() || ch_json.get("asserts").is_some() {
        ch_json.get("layout").cloned().unwrap_or_else(|| json!({}))
    } else {
        ch_json.clone()
    };

    let eps = f64::from(f32::EPSILON) * 3.0;
    let result = match compare_json_with_epsilon(&our_json, &ch_layout_json, eps) {
        Ok(()) => {
            info!("[LAYOUT] {display_name} ... ok");
            Ok(true)
        }
        Err(msg) => {
            failed.push((display_name.clone(), msg));
            Ok(false)
        }
    };
    timing.json_comparison = comparison_start.elapsed();
    timing.total = fixture_start.elapsed();

    result
}

/// Processes a single layout fixture and compares it against Chromium (sequential version).
///
/// # Errors
///
/// Returns an error if fixture processing, layouter setup, or JSON operations fail.
async fn process_layout_fixture(
    input_path: &Path,
    runtime: &Runtime,
    browser: &Arc<chromiumoxide::Browser>,
    harness_src: &str,
    failed: &mut Vec<(String, String)>,
    timing: &mut FixtureTiming,
) -> Result<bool> {
    let display_name = input_path.display().to_string();
    let fixture_start = Instant::now();

    // Setup layouter
    let setup_start = Instant::now();
    let (mut layouter, computed_for_serialization) =
        match setup_layouter_for_fixture(runtime, input_path).await {
            Ok(result) => result,
            Err(err) => {
                let msg = format!("Setup failed: {err}");
                error!("[LAYOUT] {display_name} ... FAILED: {msg}");
                failed.push((display_name.clone(), msg));
                return Ok(false);
            }
        };
    timing.setup_layouter = setup_start.elapsed();

    // Compute geometry
    let geometry_start = Instant::now();
    let rects_external = layouter.compute_layout_geometry();
    let our_json = our_layout_json(&layouter, &rects_external, &computed_for_serialization);
    timing.compute_geometry = geometry_start.elapsed();

    // Fetch or retrieve Chromium JSON (timing not tracked in detail for this function)
    let ch_json = if let Some(cached_value) = read_cached_json_for_fixture(input_path, harness_src)
    {
        cached_value
    } else {
        // Await directly - no block_on() to avoid blocking the event handler
        let page = browser.as_ref().new_page("about:blank").await?;
        let chromium_value = chromium_layout_json_in_page(&page, input_path).await?;
        page.close().await?;
        write_cached_json_for_fixture(input_path, harness_src, &chromium_value)?;
        chromium_value
    };

    // Write JSON files and compare
    let comparison_start = Instant::now();
    write_named_json_for_fixture(input_path, harness_src, "chromium", &ch_json)?;
    write_named_json_for_fixture(input_path, harness_src, "valor", &our_json)?;
    check_js_assertions(&ch_json, &display_name, failed);

    let ch_layout_json = if ch_json.get("layout").is_some() || ch_json.get("asserts").is_some() {
        ch_json.get("layout").cloned().unwrap_or_else(|| json!({}))
    } else {
        ch_json.clone()
    };

    let eps = f64::from(f32::EPSILON) * 3.0;
    let result = match compare_json_with_epsilon(&our_json, &ch_layout_json, eps) {
        Ok(()) => {
            info!("[LAYOUT] {display_name} ... ok");
            Ok(true)
        }
        Err(msg) => {
            failed.push((display_name.clone(), msg));
            Ok(false)
        }
    };
    timing.json_comparison = comparison_start.elapsed();
    timing.total = fixture_start.elapsed();

    result
}

/// Runs a single layout test for a given fixture path.
///
/// # Errors
///
/// Returns an error if browser setup, layout computation, or comparison fails.
pub fn run_single_layout_test(input_path: &Path) -> Result<()> {
    init_test_logger();
    let harness_src = concat!(
        include_str!("layout_tests.rs"),
        include_str!("common.rs"),
        include_str!("json_compare.rs"),
        include_str!("browser.rs"),
    );
    clear_valor_layout_cache_if_harness_changed(harness_src)?;

    // Single runtime for all async operations
    let runtime = Runtime::new()?;

    // Create browser for this test
    use chromiumoxide::browser::{Browser, BrowserConfig};
    use futures::StreamExt;

    // Ensure Chrome is installed (auto-downloads if needed)
    let chrome_path = ensure_chrome_installed()?;
    let config = BrowserConfig::builder()
        .chrome_executable(chrome_path)
        .no_sandbox()
        .window_size(800, 600)
        .arg("--force-device-scale-factor=1")
        .arg("--hide-scrollbars")
        .arg("--blink-settings=imagesEnabled=false")
        .arg("--disable-gpu")
        .arg("--disable-features=OverlayScrollbar")
        .arg("--allow-file-access-from-files")
        .arg("--disable-dev-shm-usage")
        .arg("--disable-extensions")
        .arg("--disable-background-networking")
        .arg("--disable-sync")
        .build()
        .map_err(|e| anyhow!("Browser config error: {}", e))?;

    let (browser, mut handler) = runtime.block_on(Browser::launch(config))?;
    let browser = Arc::new(browser);

    // Spawn handler task
    let _handler_task = runtime.spawn(async move {
        while let Some(_event) = handler.next().await {
            // Silently consume events
        }
    });

    // Run in a single block_on - all operations are async and await naturally
    let failed = runtime.block_on(async {
        let mut failed: Vec<(String, String)> = Vec::new();
        let mut timing = FixtureTiming::default();
        process_layout_fixture(
            input_path,
            &runtime,
            &browser,
            harness_src,
            &mut failed,
            &mut timing,
        )
        .await?;
        info!(
            "Timing: setup={:?}, geom={:?}, chrome={:?}, cmp={:?}, total={:?}",
            timing.setup_layouter,
            timing.compute_geometry,
            timing.chromium_total(),
            timing.json_comparison,
            timing.total
        );
        Ok::<_, anyhow::Error>(failed)
    })?;

    // Let browser drop naturally to clean up resources
    drop(browser);

    if failed.is_empty() {
        Ok(())
    } else {
        let (name, msg) = &failed[0];
        Err(anyhow!("{name}: {msg}"))
    }
}

/// Tests layout computation by comparing Valor layout with Chromium layout.
///
/// # Errors
///
/// Returns an error if browser setup fails or any layout comparisons fail.
pub fn run_chromium_layouts() -> Result<()> {
    init_test_logger();
    let harness_src = concat!(
        include_str!("layout_tests.rs"),
        include_str!("common.rs"),
        include_str!("json_compare.rs"),
        include_str!("browser.rs"),
    );
    clear_valor_layout_cache_if_harness_changed(harness_src)?;

    // Single runtime for all async operations
    let runtime = Runtime::new()?;

    // Run everything in a single block_on to avoid interfering with the handler task
    let overall_start = Instant::now();

    // Ensure Chrome is installed before starting async block (auto-downloads if needed)
    let chrome_path = ensure_chrome_installed()?;

    let (ran, failed, timing_stats) = runtime.block_on(async {
        use chromiumoxide::browser::{Browser, BrowserConfig};
        use futures::StreamExt;

        let config = BrowserConfig::builder()
            .chrome_executable(chrome_path)
            .no_sandbox()
            .window_size(800, 600)
            .arg("--force-device-scale-factor=1")
            .arg("--hide-scrollbars")
            .arg("--blink-settings=imagesEnabled=false")
            .arg("--disable-gpu")
            .arg("--disable-features=OverlayScrollbar")
            .arg("--allow-file-access-from-files")
            .arg("--disable-dev-shm-usage")
            .arg("--disable-extensions")
            .arg("--disable-background-networking")
            .arg("--disable-sync")
            .build()
            .map_err(|e| anyhow!("Browser config error: {}", e))?;

        let fixtures = get_filtered_fixtures("LAYOUT")?;
        let fixture_count = fixtures.len();

        // Now that Handler bug is fixed, use a single browser instance for all fixtures
        // Create fresh pages for each fixture to avoid page state issues from timeouts
        error!(
            "[LAYOUT] Running {} fixtures with shared browser instance",
            fixture_count
        );

        let mut failed_vec: Vec<(String, String)> = Vec::new();
        let mut timing_vec: Vec<(String, FixtureTiming)> = Vec::new();
        let mut ran = 0;

        // Launch single browser instance for all fixtures (major performance improvement)
        error!("[LAYOUT] Launching shared browser instance");
        let (browser, mut handler) = Browser::launch(config.clone()).await?;
        let browser = Arc::new(browser);

        // Spawn handler task - CRITICAL: must poll handler or CDP commands timeout
        let handler_task = tokio::spawn(async move {
            use futures::StreamExt;

            log::error!("[HANDLER] Handler task started - polling chromiumoxide CDP events");
            let mut event_count = 0;

            // Simple loop matching chromiumoxide examples - process events as fast as possible
            while let Some(event_result) = handler.next().await {
                event_count += 1;
                match event_result {
                    Ok(_) => {
                        if event_count <= 10 || event_count % 100 == 0 {
                            log::error!("[HANDLER] Event #{}: Ok", event_count);
                        }
                    }
                    Err(e) => {
                        log::error!("[HANDLER] Event #{} error: {:?}", event_count, e);
                    }
                }
            }
            log::error!("[HANDLER] Stream ended after {} events", event_count);
        });

        for (i, input_path) in fixtures.into_iter().enumerate() {
            let display_name = input_path.display().to_string();
            let mut timing = FixtureTiming::default();
            let mut local_failed: Vec<(String, String)> = Vec::new();

            if (i + 1) % 10 == 0 {
                error!("[LAYOUT] Progress: {}/{} fixtures completed", i + 1, fixture_count);
            }

            // Create fresh page for each fixture to avoid page state issues
            // Still much faster than restarting entire browser every 30 fixtures
            let page_start = Instant::now();
            let page = match browser.new_page("about:blank").await {
                Ok(p) => p,
                Err(e) => {
                    error!(
                        "[LAYOUT] {} ... ERROR: Failed to create page: {}",
                        display_name, e
                    );
                    failed_vec.push((display_name.clone(), format!("Failed to create page: {}", e)));
                    continue;
                }
            };
            timing.page_creation = page_start.elapsed();

            let result = process_layout_fixture_parallel_with_page(
                &input_path,
                &browser,
                &page,
                harness_src,
                &mut local_failed,
                &mut timing,
            )
            .await;

            // Close page immediately after use to free resources
            let _ = page.close().await;

            // Record results
            timing_vec.push((display_name.clone(), timing));
            failed_vec.extend(local_failed);

            match result {
                Ok(true) => ran += 1,
                Ok(false) => {} // Already added to failed_vec
                Err(e) => {
                    let msg = format!("ERROR: {}", e);
                    error!("[LAYOUT] {} ... {}", display_name, msg);
                    failed_vec.push((display_name.clone(), msg));
                }
            }
        }

        // Clean up shared browser
        error!("[LAYOUT] Shutting down browser instance");
        drop(browser);
        handler_task.abort();

        Ok::<_, anyhow::Error>((ran, failed_vec, timing_vec))
    })?;

    let overall_elapsed = overall_start.elapsed();

    // Print timing statistics (using eprintln to ensure visibility)
    eprintln!("\n╔══════════════════════════════════════════════════════════════");
    eprintln!("║ TIMING BREAKDOWN");
    eprintln!("╠══════════════════════════════════════════════════════════════");
    eprintln!("║ Total wall time: {:?}", overall_elapsed);
    eprintln!("║ Fixtures processed: {}", timing_stats.len());
    eprintln!("╠══════════════════════════════════════════════════════════════");

    // Calculate aggregates
    let mut total_page_creation = Duration::ZERO;
    let mut total_navigation = Duration::ZERO;
    let mut total_script_eval = Duration::ZERO;
    let mut total_json_parse = Duration::ZERO;
    let mut total_setup = Duration::ZERO;
    let mut total_geometry = Duration::ZERO;
    let mut total_comparison = Duration::ZERO;
    let mut total_fixture_time = Duration::ZERO;

    for (_, timing) in &timing_stats {
        total_page_creation += timing.page_creation;
        total_navigation += timing.navigation;
        total_script_eval += timing.script_evaluation;
        total_json_parse += timing.json_parsing;
        total_setup += timing.setup_layouter;
        total_geometry += timing.compute_geometry;
        total_comparison += timing.json_comparison;
        total_fixture_time += timing.total;
    }

    let total_chromium = total_page_creation + total_navigation + total_script_eval + total_json_parse;
    let total_valor = total_setup + total_geometry;

    eprintln!("║ Total time in phases:");
    eprintln!("║   CHROMIUM EXTRACTION:");
    eprintln!("║     Page creation:       {:?}", total_page_creation);
    eprintln!("║     Navigation:          {:?}", total_navigation);
    eprintln!("║     Script evaluation:   {:?}", total_script_eval);
    eprintln!("║     JSON parsing:        {:?}", total_json_parse);
    eprintln!("║     ─── Chromium total:  {:?}", total_chromium);
    eprintln!("║");
    eprintln!("║   VALOR COMPUTATION:");
    eprintln!("║     Setup layouter:      {:?}", total_setup);
    eprintln!("║     Compute geometry:    {:?}", total_geometry);
    eprintln!("║     ─── Valor total:     {:?}", total_valor);
    eprintln!("║");
    eprintln!("║   JSON comparison:       {:?}", total_comparison);
    eprintln!("║   ═══════════════════════════════");
    eprintln!("║   Sum of fixtures:       {:?}", total_fixture_time);
    eprintln!("║");
    eprintln!("║ Average per fixture:");
    let n = timing_stats.len() as u32;
    if n > 0 {
        eprintln!("║   Page creation:         {:?}", total_page_creation / n);
        eprintln!("║   Navigation:            {:?}", total_navigation / n);
        eprintln!("║   Script evaluation:     {:?}", total_script_eval / n);
        eprintln!("║   JSON parsing:          {:?}", total_json_parse / n);
        eprintln!("║   Setup layouter:        {:?}", total_setup / n);
        eprintln!("║   Compute geometry:      {:?}", total_geometry / n);
        eprintln!("║   JSON comparison:       {:?}", total_comparison / n);
        eprintln!("║   Total:                 {:?}", total_fixture_time / n);
    }
    eprintln!("║");
    eprintln!("║ Parallelization efficiency:");
    eprintln!("║   Serial time (estimated): {:?}", total_fixture_time);
    eprintln!("║   Actual time:             {:?}", overall_elapsed);
    if !total_fixture_time.is_zero() {
        let speedup = total_fixture_time.as_secs_f64() / overall_elapsed.as_secs_f64();
        eprintln!("║   Speedup:                 {:.2}x", speedup);
    }
    eprintln!("╚══════════════════════════════════════════════════════════════\n");

    // Print slowest fixtures
    let mut sorted_timing = timing_stats.clone();
    sorted_timing.sort_by_key(|(_, t)| std::cmp::Reverse(t.total));
    eprintln!("Top 10 slowest fixtures:");
    for (name, timing) in sorted_timing.iter().take(10) {
        eprintln!("  {:?} - {}", timing.total, name);
    }

    if failed.is_empty() {
        info!("[LAYOUT] {ran} fixtures passed");
        Ok(())
    } else {
        error!("==== LAYOUT FAILURES ({} total) ====", failed.len());
        for (name, msg) in &failed {
            error!("- {name}\n  {msg}\n");
        }
        Err(anyhow!(
            "{} layout fixture(s) failed; see log above.",
            failed.len()
        ))
    }
}

struct LayoutCtx<'ctx> {
    kind_by_key: &'ctx HashMap<NodeKey, LayoutNodeKind>,
    children_by_key: &'ctx HashMap<NodeKey, Vec<NodeKey>>,
    attrs_by_key: &'ctx HashMap<NodeKey, HashMap<String, String>>,
    rects: &'ctx HashMap<NodeKey, LayoutRect>,
    computed: &'ctx HashMap<NodeKey, ComputedStyle>,
}

fn is_non_rendering_tag(tag: &str) -> bool {
    matches!(
        tag,
        "head" | "meta" | "title" | "link" | "style" | "script" | "base"
    )
}

const FLEX_BASIS: &str = "auto";

const fn effective_display(display: Display) -> &'static str {
    match display {
        Display::Inline => "inline",
        Display::Block | Display::Contents => "block",
        Display::Flex => "flex",
        Display::InlineFlex => "inline-flex",
        Display::InlineBlock => "inline-block",
        Display::None => "none",
    }
}

fn build_style_json(computed: &ComputedStyle) -> JsonValue {
    json!({
        "display": effective_display(computed.display),
        "boxSizing": match computed.box_sizing { BoxSizing::BorderBox => "border-box", BoxSizing::ContentBox => "content-box" },
        "flexBasis": FLEX_BASIS,
        "flexGrow": f64::from(computed.flex_grow),
        "flexShrink": f64::from(computed.flex_shrink),
        "alignItems": match computed.align_items {
            AlignItems::FlexStart => "flex-start",
            AlignItems::Center => "center",
            AlignItems::FlexEnd => "flex-end",
            AlignItems::Stretch => "normal",
        },
        "overflow": match computed.overflow { Overflow::Visible => "visible", _ => "hidden" },
        "margin": {
            "top": format!("{}px", computed.margin.top),
            "right": format!("{}px", computed.margin.right),
            "bottom": format!("{}px", computed.margin.bottom),
            "left": format!("{}px", computed.margin.left),
        },
        "padding": {
            "top": format!("{}px", computed.padding.top),
            "right": format!("{}px", computed.padding.right),
            "bottom": format!("{}px", computed.padding.bottom),
            "left": format!("{}px", computed.padding.left),
        },
        "borderWidth": {
            "top": format!("{}px", computed.border_width.top),
            "right": format!("{}px", computed.border_width.right),
            "bottom": format!("{}px", computed.border_width.bottom),
            "left": format!("{}px", computed.border_width.left),
        }
    })
}

fn collect_children_json(ctx: &LayoutCtx<'_>, key: NodeKey) -> Vec<JsonValue> {
    let mut kids_json: Vec<JsonValue> = Vec::new();
    if let Some(children) = ctx.children_by_key.get(&key) {
        for child in children {
            if matches!(
                ctx.kind_by_key.get(child),
                Some(LayoutNodeKind::Block { .. })
            ) {
                // Skip elements with display:none
                if let Some(computed) = ctx.computed.get(child)
                    && computed.display == Display::None
                {
                    continue;
                }

                let child_json = serialize_element_subtree(ctx, *child);
                // Skip empty JSON objects (filtered elements)
                if !child_json.is_null() && !child_json.as_object().is_some_and(JsonMap::is_empty) {
                    kids_json.push(child_json);
                }
            }
        }
    }
    kids_json
}

fn serialize_element_subtree(ctx: &LayoutCtx<'_>, key: NodeKey) -> JsonValue {
    let mut out = json!({});
    if let Some(LayoutNodeKind::Block { tag }) = ctx.kind_by_key.get(&key) {
        if is_non_rendering_tag(tag) || tag.is_empty() {
            return json!({});
        }
        let rect = ctx.rects.get(&key).copied().unwrap_or_default();
        let display_tag = tag.clone();
        let id = ctx
            .attrs_by_key
            .get(&key)
            .and_then(|attr_map| attr_map.get("id"))
            .cloned()
            .unwrap_or_default();
        let computed = ctx.computed.get(&key).cloned().unwrap_or_default();
        out = json!({
            "tag": display_tag,
            "id": id,
            "rect": {
                "x": f64::from(rect.x),
                "y": f64::from(rect.y),
                "width": f64::from(rect.width),
                "height": f64::from(rect.height),
            },
            "style": build_style_json(&computed)
        });
        let kids_json = collect_children_json(ctx, key);
        if let Some(obj) = out.as_object_mut() {
            obj.insert("children".to_owned(), JsonValue::Array(kids_json));
        }
    }
    out
}

fn find_root_element(
    body_key: Option<NodeKey>,
    html_key: Option<NodeKey>,
    kind_by_key: &HashMap<NodeKey, LayoutNodeKind>,
    children_by_key: &HashMap<NodeKey, Vec<NodeKey>>,
) -> Option<NodeKey> {
    if let Some(key) = body_key.or(html_key) {
        return Some(key);
    }

    if let Some(children) = children_by_key.get(&NodeKey::ROOT) {
        for child in children {
            if matches!(kind_by_key.get(child), Some(LayoutNodeKind::Block { .. })) {
                return Some(*child);
            }
        }
    }

    for (node_key, kind) in kind_by_key {
        if matches!(kind, LayoutNodeKind::Block { .. }) {
            return Some(*node_key);
        }
    }

    None
}

fn our_layout_json(
    layouter: &Layouter,
    rects: &HashMap<NodeKey, LayoutRect>,
    computed: &HashMap<NodeKey, ComputedStyle>,
) -> JsonValue {
    let snapshot = layouter.snapshot();
    let mut kind_by_key = HashMap::new();
    let mut children_by_key = HashMap::new();
    for (node_key, kind, children) in snapshot {
        kind_by_key.insert(node_key, kind);
        children_by_key.insert(node_key, children);
    }
    let attrs_by_key = layouter.attrs_map();
    let mut body_key: Option<NodeKey> = None;
    let mut html_key: Option<NodeKey> = None;
    for (node_key, kind) in &kind_by_key {
        if let LayoutNodeKind::Block { tag } = kind {
            if tag.eq_ignore_ascii_case("body") {
                body_key = Some(*node_key);
                break;
            }
            if tag.eq_ignore_ascii_case("html") && html_key.is_none() {
                html_key = Some(*node_key);
            }
        }
    }

    let root_key = find_root_element(body_key, html_key, &kind_by_key, &children_by_key)
        .unwrap_or(NodeKey::ROOT);
    let ctx = LayoutCtx {
        kind_by_key: &kind_by_key,
        children_by_key: &children_by_key,
        attrs_by_key: &attrs_by_key,
        rects,
        computed,
    };
    serialize_element_subtree(&ctx, root_key)
}

fn chromium_layout_extraction_script() -> &'static str {
    "(function() {
        function shouldSkip(el) {
            if (!el || !el.tagName) return false;
            var tag = String(el.tagName).toLowerCase();
            if (tag === 'style' && el.getAttribute('data-valor-test-reset') === '1') return true;
            try {
                var cs = window.getComputedStyle(el);
                if (cs && String(cs.display||'').toLowerCase() === 'none') return true;
            } catch (e) { /* ignore */ }
            return false;
        }
        function pickStyle(el, cs) {
            var d = String(cs.display || '').toLowerCase();
            var display = (d === 'flex') ? 'flex' : 'block';
            function pickEdges(prefix) {
                return {
                    top: cs[prefix + 'Top'] || '',
                    right: cs[prefix + 'Right'] || '',
                    bottom: cs[prefix + 'Bottom'] || '',
                    left: cs[prefix + 'Left'] || ''
                };
            }
            return {
                display: display,
                boxSizing: (cs.boxSizing || '').toLowerCase(),
                flexBasis: cs.flexBasis || '',
                flexGrow: Number(cs.flexGrow || 0),
                flexShrink: Number(cs.flexShrink || 0),
                margin: pickEdges('margin'),
                padding: pickEdges('padding'),
                borderWidth: {
                    top: cs.borderTopWidth || '',
                    right: cs.borderRightWidth || '',
                    bottom: cs.borderBottomWidth || '',
                    left: cs.borderLeftWidth || '',
                },
                alignItems: (cs.alignItems || '').toLowerCase(),
                overflow: (cs.overflow || '').toLowerCase(),
            };
        }
        function ser(el) {
            var r = el.getBoundingClientRect();
            var cs = window.getComputedStyle(el);
            return {
                tag: String(el.tagName||'').toLowerCase(),
                id: String(el.id||''),
                rect: { x: r.x, y: r.y, width: r.width, height: r.height },
                style: pickStyle(el, cs),
                children: Array.from(el.children).filter(function(c){ return !shouldSkip(c); }).map(ser)
            };
        }
        if (!window._valorResults) { window._valorResults = []; }
        if (typeof window._valorAssert !== 'function') {
            window._valorAssert = function(name, cond, details) {
                window._valorResults.push({ name: String(name||''), ok: !!cond, details: String(details||'') });
            };
        }
        if (typeof window._valorRun === 'function') {
            try { window._valorRun(); } catch (e) {
                window._valorResults.push({ name: '_valorRun', ok: false, details: String(e && e.stack || e) });
            }
        }
        var root = document.body || document.documentElement;
        var layout = ser(root);
        var asserts = Array.isArray(window._valorResults) ? window._valorResults : [];
        return JSON.stringify({ layout: layout, asserts: asserts });
    })()"
}

/// Extracts layout JSON from Chromium by evaluating JavaScript in a page.
///
/// # Errors
///
/// Returns an error if navigation, script evaluation, or JSON parsing fails.
struct ChromiumExtractTiming {
    navigation: Duration,
    script_evaluation: Duration,
    json_parsing: Duration,
}

async fn chromium_layout_json_in_page_with_timing(
    page: &Page,
    path: &Path,
) -> Result<(JsonValue, ChromiumExtractTiming)> {
    use tokio::time::{Duration, timeout, Instant};

    let nav_start = Instant::now();
    navigate_and_prepare_page(page, path).await?;
    let navigation_time = nav_start.elapsed();

    let script = chromium_layout_extraction_script();
    let eval_start = Instant::now();

    // Add 10 second timeout to script evaluation
    let result = match timeout(Duration::from_secs(10), page.evaluate(script)).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            return Err(anyhow!("Script evaluation failed for {}: {}", path.display(), e));
        }
        Err(_) => {
            return Err(anyhow!("Script evaluation timeout after 10s for {}", path.display()));
        }
    };
    let script_eval_time = eval_start.elapsed();

    let parse_start = Instant::now();
    let json_string = result
        .value()
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Chromium returned non-string JSON for layout"))?;
    let parsed: JsonValue = from_str(json_string)?;
    let json_parse_time = parse_start.elapsed();

    Ok((
        parsed,
        ChromiumExtractTiming {
            navigation: navigation_time,
            script_evaluation: script_eval_time,
            json_parsing: json_parse_time,
        },
    ))
}

async fn chromium_layout_json_in_page(page: &Page, path: &Path) -> Result<JsonValue> {
    let (json, _timing) = chromium_layout_json_in_page_with_timing(page, path).await?;
    Ok(json)
}
