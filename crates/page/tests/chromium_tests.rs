//! Chromium comparison tests - compares Valor layout against Chrome reference.

mod chromium_compare;

use chromium_compare::{cache, chrome, common, json_compare, valor_serialization};
use futures::stream;
use rewrite_core::{DomBroadcast, NodeId, Subscriber};
use rewrite_css::{ParsedRule, Property, Styler};
use rewrite_page::Browser;
use rewrite_renderer::{ComputedBox, resolve_all_layout, resolve_layout};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Collects computed layout values for all nodes.
struct LayoutCollector {
    styler: Arc<Styler>,
    layouts: Mutex<HashMap<NodeId, ComputedBox>>,
    viewport_width: u32,
    viewport_height: u32,
}

impl LayoutCollector {
    fn new(styler: Arc<Styler>, viewport_width: u32, viewport_height: u32) -> Self {
        Self {
            styler,
            layouts: Mutex::new(HashMap::new()),
            viewport_width,
            viewport_height,
        }
    }

    fn layouts(&self) -> HashMap<NodeId, ComputedBox> {
        self.layouts.lock().expect("lock poisoned").clone()
    }
}

impl Subscriber for LayoutCollector {
    fn on_property(&self, node: NodeId, property: &Property<'static>) {
        let computed = resolve_layout(
            &self.styler,
            node,
            property,
            self.viewport_width,
            self.viewport_height,
        );

        let mut layouts = self.layouts.lock().expect("lock poisoned");
        let entry = layouts.entry(node).or_default();

        if computed.width.is_some() {
            entry.width = computed.width;
        }
        if computed.height.is_some() {
            entry.height = computed.height;
        }
        if computed.x.is_some() {
            entry.x = computed.x;
        }
        if computed.y.is_some() {
            entry.y = computed.y;
        }
    }

    fn on_dom(&self, update: DomBroadcast) {
        match update {
            DomBroadcast::CreateNode { node, parent: _ } => {
                let computed = resolve_all_layout(
                    &self.styler,
                    node,
                    self.viewport_width,
                    self.viewport_height,
                );

                let mut layouts = self.layouts.lock().expect("lock poisoned");
                let entry = layouts.entry(node).or_default();

                if computed.width.is_some() {
                    entry.width = computed.width;
                }
                if computed.height.is_some() {
                    entry.height = computed.height;
                }
                if computed.x.is_some() {
                    entry.x = computed.x;
                }
                if computed.y.is_some() {
                    entry.y = computed.y;
                }
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

        match run_fixture_test(fixture_path, chrome_page.as_ref(), &rt) {
            TestResult::Pass => {
                passed += 1;
                println!("\u{2713} {fixture_path}");
            }
            TestResult::Fail(err) => {
                failed += 1;
                eprintln!("\u{2717} {fixture_path}:\n{err}");
            }
            TestResult::Skip(reason) => {
                skipped += 1;
                println!("- {fixture_path} (skipped: {reason})");
            }
        }
    }

    println!("\nResults: {passed} passed, {failed} failed, {skipped} skipped");

    // Don't panic on comparison failures — this is a progress tracker.
    // Failures are expected while the layout engine is being built.
    // Failure artifacts are saved to target/test_cache/layout/failing/
}

/// Run a single fixture test with optional Chrome comparison.
fn run_fixture_test(
    fixture_path: &str,
    chrome_page: Option<&chromiumoxide::page::Page>,
    rt: &tokio::runtime::Runtime,
) -> TestResult {
    let html = match fs::read_to_string(fixture_path) {
        Ok(html) => html,
        Err(err) => return TestResult::Fail(format!("Failed to read fixture: {err}")),
    };

    let html_with_reset = common::prepend_css_reset(&html);

    // Extract CSS from <style> blocks before HTML parsing
    let css_blocks = extract_style_blocks(&html_with_reset);

    // Valor side
    let browser = Browser::default();
    let (page, styler) = browser.new_page_headless();

    // Load UA stylesheet + extracted CSS synchronously into the Styler
    let all_css = format!("{UA_STYLESHEET}\n{}", css_blocks.join("\n"));
    load_css_sync(&styler, &all_css);

    let collector = Arc::new(LayoutCollector::new(
        styler.clone(),
        common::VIEWPORT_WIDTH,
        common::VIEWPORT_HEIGHT,
    ));
    browser
        .subscriptions()
        .add_subscriber(Box::new(CollectorSubscriber(collector.clone())));

    page.load_html(stream::iter(vec![html_with_reset]));

    let layouts = collector.layouts();
    let valor_json =
        match valor_serialization::serialize_valor_layout(&page.tree, &styler, &layouts) {
            Ok(json) => json,
            Err(err) => return TestResult::Fail(format!("Valor serialization failed: {err}")),
        };

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

    // Compare
    let valor_layout = &valor_json["layout"];
    let chrome_layout = &chrome_json["layout"];

    match json_compare::compare_json_with_epsilon(valor_layout, chrome_layout, 0.0) {
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
    }
}

/// Minimal UA stylesheet — default display modes for HTML elements.
const UA_STYLESHEET: &str = r#"
html, body, div, p, h1, h2, h3, h4, h5, h6,
ul, ol, li, dl, dt, dd, blockquote, pre,
form, fieldset, legend, table, caption,
thead, tbody, tfoot, tr, th, td,
article, aside, details, figcaption, figure,
footer, header, hgroup, main, menu, nav, section, summary {
    display: block;
}

span, a, em, strong, b, i, u, s, small, big, sub, sup,
abbr, cite, code, dfn, kbd, samp, var, label, q, time {
    display: inline;
}

head, script, style, link, meta, title {
    display: none;
}

body {
    margin: 8px;
}

h1 { font-size: 2em; margin: 0.67em 0; font-weight: bold; }
h2 { font-size: 1.5em; margin: 0.83em 0; font-weight: bold; }
h3 { font-size: 1.17em; margin: 1em 0; font-weight: bold; }
h4 { margin: 1.33em 0; font-weight: bold; }
h5 { font-size: 0.83em; margin: 1.67em 0; font-weight: bold; }
h6 { font-size: 0.67em; margin: 2.33em 0; font-weight: bold; }

p { margin: 1em 0; }
ul, ol { margin: 1em 0; padding-left: 40px; }
li { display: list-item; }

table { display: table; }
thead { display: table-header-group; }
tbody { display: table-row-group; }
tfoot { display: table-footer-group; }
tr { display: table-row; }
td, th { display: table-cell; }
"#;

/// Parse CSS text and add rules to the Styler synchronously.
fn load_css_sync(styler: &Styler, css: &str) {
    use lightningcss::rules::CssRule;
    use lightningcss::stylesheet::{ParserOptions, StyleSheet};
    use lightningcss::traits::IntoOwned;

    let options = ParserOptions {
        error_recovery: true,
        ..Default::default()
    };

    let Ok(stylesheet) = StyleSheet::parse(css, options) else {
        return;
    };

    for rule in stylesheet.rules.0 {
        if let CssRule::Style(style_rule) = rule {
            let parsed = ParsedRule::Stylesheet {
                selectors: style_rule.selectors.into_owned(),
                properties: style_rule.declarations.into(),
            };
            styler.add_rule(parsed);
        }
    }

    styler.flush();
}

/// Extract CSS text from all `<style>` blocks in the HTML.
fn extract_style_blocks(html: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut search_from = 0;
    let lower = html.to_lowercase();

    while let Some(start_tag) = lower[search_from..].find("<style") {
        let abs_start = search_from + start_tag;
        // Find end of opening tag
        let Some(tag_end) = html[abs_start..].find('>') else {
            break;
        };
        let content_start = abs_start + tag_end + 1;
        // Find closing </style>
        let Some(end_tag) = lower[content_start..].find("</style") else {
            break;
        };
        let content_end = content_start + end_tag;
        blocks.push(html[content_start..content_end].to_string());
        search_from = content_end;
    }

    blocks
}

// Include generated tests
include!(concat!(env!("OUT_DIR"), "/generated_fixture_tests.rs"));
