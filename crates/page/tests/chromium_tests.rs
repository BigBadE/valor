//! Chromium comparison tests - compares Valor layout against Chrome reference.

mod chromium_compare;

use chromium_compare::{cache, chrome, common, json_compare, valor_serialization};
use futures::stream;
use rewrite_core::{Database, DomBroadcast, NodeId, Subscriber};
use rewrite_css::{Property, Styler};
use rewrite_page::Browser;
use rewrite_renderer::LayoutState;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Manages layout state and receives incremental updates from the
/// CSS subscriber pipeline. Nodes are resolved eagerly during property
/// changes; `resolve_dirty` finalizes any stale ancestors after load.
struct LayoutCollector {
    layout: Mutex<LayoutState>,
}

impl LayoutCollector {
    fn new(
        styler: Arc<Styler>,
        db: Arc<Database>,
        viewport_width: u32,
        viewport_height: u32,
    ) -> Self {
        Self {
            layout: Mutex::new(LayoutState::new(
                styler,
                db,
                viewport_width,
                viewport_height,
            )),
        }
    }
}

impl Subscriber for LayoutCollector {
    fn on_property(&self, node: NodeId, property: &Property<'static>) {
        self.layout
            .lock()
            .expect("lock poisoned")
            .on_property_change(node, property);
    }

    fn on_dom(&self, update: DomBroadcast) {
        match update {
            DomBroadcast::CreateNode { node, parent } => {
                self.layout
                    .lock()
                    .expect("lock poisoned")
                    .on_node_created(node, parent);
            }
        }
    }
}

/// Wrapper to implement Subscriber for Arc<LayoutCollector>.
struct CollectorSubscriber(Arc<LayoutCollector>);

impl Subscriber for CollectorSubscriber {
    fn on_property(&self, node: NodeId, property: &Property<'static>) {
        self.0.on_property(node, property);
    }

    fn on_dom(&self, update: DomBroadcast) {
        self.0.on_dom(update);
    }
}

enum TestResult {
    Pass,
    Fail(String),
    Skip(String),
}

/// Run all fixture tests.
fn run_all_fixtures(fixtures: &[&str]) {
    // Clean up old failure artifacts so only current failures remain.
    let failing_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test_cache/layout/failing");
    if failing_dir.exists() {
        let _ = fs::remove_dir_all(&failing_dir);
    }

    let has_chrome = chrome::chrome_available();

    // Start Chrome if available
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let chrome_handle = if has_chrome {
        match rt.block_on(chrome::start_and_connect()) {
            Ok(browser) => {
                eprintln!("Chrome connected for layout comparison");
                Some(browser)
            }
            Err(err) => {
                eprintln!("Warning: Chrome failed to start: {err}");
                None
            }
        }
    } else {
        eprintln!("Warning: Chrome not available, skipping comparison");
        None
    };

    let mut passed = 0;
    let mut failed = 0;
    let mut skipped = 0;

    for fixture_path in fixtures {
        let chrome_page = if let Some(ref handle) = chrome_handle {
            match rt.block_on(handle.browser.new_page("about:blank")) {
                Ok(page) => Some(page),
                Err(err) => {
                    eprintln!("Warning: Failed to create Chrome page: {err}");
                    None
                }
            }
        } else {
            None
        };

        let start = Instant::now();
        let result = run_fixture_test(fixture_path, chrome_page.as_ref(), &rt);
        let elapsed = start.elapsed();

        match result {
            TestResult::Pass => {
                passed += 1;
                println!("\u{2713} {fixture_path} ({elapsed:.2?})");
            }
            TestResult::Fail(err) => {
                failed += 1;
                eprintln!("\u{2717} {fixture_path} ({elapsed:.2?}):\n{err}");
            }
            TestResult::Skip(reason) => {
                skipped += 1;
                println!("- {fixture_path} (skipped: {reason})");
            }
        }
    }

    println!("\nResults: {passed} passed, {failed} failed, {skipped} skipped");

    // Tests MUST fail when there are comparison errors.
    assert!(
        failed == 0,
        "{failed} fixture(s) failed — see target/test_cache/layout/failing/ for details"
    );
}

/// Run a single fixture test with optional Chrome comparison.
fn run_fixture_test(
    fixture_path: &str,
    chrome_page: Option<&chromiumoxide::page::Page>,
    rt: &tokio::runtime::Runtime,
) -> TestResult {
    let is_long_test = fixture_path.contains("long_test");

    let html = match fs::read_to_string(fixture_path) {
        Ok(html) => html,
        Err(err) => return TestResult::Fail(format!("Failed to read fixture: {err}")),
    };

    let html_with_reset = common::prepend_css_reset(&html);

    // Valor side
    let t0 = Instant::now();
    let browser = Browser::default();
    let page = browser.new_page_headless();

    let collector = Arc::new(LayoutCollector::new(
        page.styler.clone(),
        page.db.clone(),
        common::VIEWPORT_WIDTH,
        common::VIEWPORT_HEIGHT,
    ));
    browser
        .subscriptions()
        .add_subscriber(Box::new(CollectorSubscriber(collector.clone())));
    let t1 = Instant::now();

    page.load_html(stream::iter(vec![html_with_reset]));
    let t2 = Instant::now();

    let valor_json = match valor_serialization::serialize_valor_layout(
        &page.tree,
        &page.db,
        &collector.layout,
    ) {
        Ok(json) => json,
        Err(err) => return TestResult::Fail(format!("Valor serialization failed: {err}")),
    };
    let t3 = Instant::now();

    // Chrome side
    let fixture = Path::new(fixture_path);
    let chrome_json = if let Some(chrome_page) = chrome_page {
        match rt.block_on(chrome::get_layout_cached(chrome_page, fixture)) {
            Ok(json) => json,
            Err(err) => return TestResult::Fail(format!("Chrome extraction failed: {err}")),
        }
    } else if let Some(cached) = cache::read_cached(fixture) {
        cached
    } else {
        return TestResult::Skip("no Chrome and no cache".to_string());
    };
    let t4 = Instant::now();

    // Compare
    let valor_layout = &valor_json["layout"];
    let chrome_layout = &chrome_json["layout"];

    let result = match json_compare::compare_json_with_epsilon(valor_layout, chrome_layout, 0.0) {
        Ok(()) => TestResult::Pass,
        Err(err) => {
            // Save failure artifacts
            let fixture_name = fixture
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("unknown");
            cache::write_failure_artifacts(fixture_name, &chrome_json, &valor_json, &err);
            TestResult::Fail(err)
        }
    };
    let t5 = Instant::now();

    if is_long_test {
        eprintln!("\n  [long_test timing breakdown]");
        eprintln!("    Browser/page setup:    {:>8.2?}", t1 - t0);
        eprintln!("    load_html (parse+layout): {:>8.2?}", t2 - t1);
        eprintln!("    Valor serialization:   {:>8.2?}", t3 - t2);
        eprintln!("    Chrome cache read:     {:>8.2?}", t4 - t3);
        eprintln!("    JSON compare:          {:>8.2?}", t5 - t4);
        eprintln!("    TOTAL:                 {:>8.2?}", t5 - t0);
    }

    result
}

// Include generated tests
include!(concat!(env!("OUT_DIR"), "/generated_fixture_tests.rs"));
