use anyhow::Error;
use headless_chrome::{Browser, LaunchOptionsBuilder};
use js::NodeKey;
use layouter::LayoutRect;
use layouter::{LayoutNodeKind, Layouter};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use tokio::runtime::Runtime;
use std::ffi::OsStr;

mod common;

#[test]
fn chromium_layout_test() -> Result<(), Error> {
    // Initialize logger to show logs during tests (including JS console.* forwarded via log)
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(false)
        .try_init();

    // Launch a fresh headless Chrome instance for this test and drop it at the end to avoid hanging threads.
    let launch_opts = LaunchOptionsBuilder::default()
        .headless(true)
        .window_size(Some((800, 600)))
        .idle_browser_timeout(std::time::Duration::from_secs(300))
        .args(vec![
            OsStr::new("--force-device-scale-factor=1"),
            OsStr::new("--disable-features=OverlayScrollbar"),
            OsStr::new("--allow-file-access-from-files"),
            // Improve stability in CI/headless environments
            OsStr::new("--disable-gpu"),
            OsStr::new("--disable-dev-shm-usage"),
            OsStr::new("--no-sandbox"),
        ])
        .build()
        .expect("Failed to build LaunchOptions for headless_chrome");
    let browser = Browser::new(launch_opts).expect("Failed to launch headless Chrome browser");

    let mut failed: Vec<(String, String)> = Vec::new();
    // Iterate over all fixtures and compare against each expected file content.
    for input_path in common::fixture_html_files()? {
        let display_name = input_path.display().to_string();
        // Build page and parse via HtmlPage
        let url = common::to_file_url(&input_path)?;
        let rt = Runtime::new()?;
        let mut page = common::create_page(&rt, url)?;
        // Attach a Layouter mirror to the page's DOM stream
        let mut layouter_mirror = page.create_mirror(Layouter::new());

        // Drive parsing and mirroring until finished (with timeout), draining the layouter mirror per tick
        let finished = common::update_until_finished(&rt, &mut page, |_| {
            layouter_mirror.try_update_sync()?;
            Ok(())
        })?;
        if !finished {
            let msg = format!("Parsing did not finish");
            eprintln!("[LAYOUT] {} ... FAILED: {}", display_name, msg);
            failed.push((display_name.clone(), msg));
            continue;
        }

        // Provide computed styles from the page's internal StyleEngine to Layouter
        let computed = page.computed_styles_snapshot()?;
        let layouter = layouter_mirror.mirror_mut();
        layouter.set_computed_styles(computed);
        let _count = layouter.compute_layout();
        let rects = layouter.compute_layout_geometry();

        // Build our full layout JSON starting from the first element child under #document (typically <html>)
        let our_json = our_layout_json(layouter, &rects);

        // Build Chromium's full layout JSON by evaluating JS in the page
        let ch_json = match chromium_layout_json_for_path(&browser, &input_path) {
            Ok(v) => v,
            Err(e) => {
                let msg = format!("failed to get Chromium JSON: {}", e);
                eprintln!("[LAYOUT] {} ... FAILED: {}", display_name, msg);
                failed.push((display_name.clone(), msg));
                continue;
            }
        };

        // Compare using epsilon for floats
        let eps = 8.0_f64;
        match common::compare_json_with_epsilon(&our_json, &ch_json, eps) {
            Ok(_) => {
                println!("[LAYOUT] {} ... ok", display_name);
            }
            Err(msg) => {
                // The comparison message already contains the precise path and the element snippets for both sides.
                eprintln!("[LAYOUT] {} ... FAILED: {}", display_name, msg);
                failed.push((display_name.clone(), msg));
            }
        }
    }
    if !failed.is_empty() {
        eprintln!("\n==== LAYOUT FAILURES ({} total) ====", failed.len());
        for (name, msg) in &failed {
            eprintln!("- {}\n  {}\n", name, msg);
        }
        panic!("{} layout fixture(s) failed; see log above.", failed.len());
    }
    Ok(())
}

fn our_layout_json(layouter: &Layouter, rects: &HashMap<NodeKey, LayoutRect>) -> Value {
    // Build lookup maps once
    let snapshot = layouter.snapshot();
    let mut kind_by_key = HashMap::new();
    let mut children_by_key = HashMap::new();
    for (k, kind, children) in snapshot.into_iter() {
        kind_by_key.insert(k, kind);
        children_by_key.insert(k, children);
    }
    // Also build attributes lookup to access element ids
    let attrs_by_key = layouter.attrs_map();
    // Find first element child of ROOT (typically <html>)
    let mut root_elem: Option<NodeKey> = None;
    if let Some(children) = children_by_key.get(&NodeKey::ROOT) {
        for child in children {
            if let Some(LayoutNodeKind::Block { .. }) = kind_by_key.get(child) {
                root_elem = Some(*child);
                break;
            }
        }
    }
    // If the root is <html>, prefer serializing from its <body> child to avoid Chromium's root-margin quirks
    let mut root_key = root_elem.unwrap_or(NodeKey::ROOT);
    if let Some(LayoutNodeKind::Block { tag }) = kind_by_key.get(&root_key) {
        if tag.eq_ignore_ascii_case("html") {
            if let Some(children) = children_by_key.get(&root_key) {
                if let Some(body_child) = children.iter().find(|c| match kind_by_key.get(*c) {
                    Some(LayoutNodeKind::Block { tag }) if tag.eq_ignore_ascii_case("body") => true,
                    _ => false,
                }) {
                    root_key = *body_child;
                }
            }
        }
    }
    serialize_element_subtree_with_maps(&kind_by_key, &children_by_key, &attrs_by_key, rects, root_key)
}

fn serialize_element_subtree_with_maps(
    kind_by_key: &HashMap<NodeKey, LayoutNodeKind>,
    children_by_key: &HashMap<NodeKey, Vec<NodeKey>>,
    attrs_by_key: &HashMap<NodeKey, HashMap<String, String>>,
    rects: &HashMap<NodeKey, LayoutRect>,
    key: NodeKey,
) -> Value {
    fn is_non_rendering_tag(tag: &str) -> bool {
        matches!(tag, "head" | "meta" | "title" | "link" | "style" | "script" | "base")
    }
    fn recurse(
        key: NodeKey,
        kind_by_key: &HashMap<NodeKey, LayoutNodeKind>,
        children_by_key: &HashMap<NodeKey, Vec<NodeKey>>,
        attrs_by_key: &HashMap<NodeKey, HashMap<String, String>>,
        rects: &HashMap<NodeKey, LayoutRect>,
    ) -> Value {
        match kind_by_key.get(&key) {
            Some(LayoutNodeKind::Block { tag }) => {
                let mut rect_json = json!({"x": 0.0, "y": 0.0, "width": 0.0, "height": 0.0});
                if !is_non_rendering_tag(tag.to_lowercase().as_str()) {
                    if let Some(r) = rects.get(&key) {
                        rect_json = json!({
                            "x": r.x as f64,
                            "y": r.y as f64,
                            "width": r.width as f64,
                            "height": r.height as f64,
                        });
                    }
                }
                let mut children_json = Vec::new();
                if let Some(children) = children_by_key.get(&key) {
                    for child in children {
                        if let Some(LayoutNodeKind::Block { .. }) = kind_by_key.get(child) {
                            let v = recurse(*child, kind_by_key, children_by_key, attrs_by_key, rects);
                            children_json.push(v);
                        }
                    }
                }
                let mut obj = json!({
                    "tag": tag.to_lowercase(),
                    "rect": rect_json,
                    "children": children_json,
                });
                let id_val = attrs_by_key
                    .get(&key)
                    .and_then(|attrs| attrs.get("id").cloned())
                    .unwrap_or_else(|| String::new());
                obj["id"] = json!(id_val);
                obj
            }
            Some(LayoutNodeKind::Document) | Some(LayoutNodeKind::InlineText { .. }) | None => {
                // For document or text nodes, dive into children to find first element
                if let Some(children) = children_by_key.get(&key) {
                    for child in children {
                        if let Some(LayoutNodeKind::Block { .. }) = kind_by_key.get(child) {
                            return recurse(*child, kind_by_key, children_by_key, attrs_by_key, rects);
                        }
                    }
                }
                // Fallback empty
                json!({"tag": "", "rect": {"x":0.0,"y":0.0,"width":0.0,"height":0.0}, "children": []})
            }
        }
    }
    recurse(key, kind_by_key, children_by_key, attrs_by_key, rects)
}

fn chromium_layout_json_for_path(browser: &Browser, path: &Path) -> anyhow::Result<Value> {
    // Convert the file path to a file:// URL
    let url = common::to_file_url(path)?;

    // Open a fresh tab per file from the provided browser
    let tab = browser.new_tab()?;

    // Use an owned String to avoid any possibility of truncated/borrowed URL issues in downstream logging/transport.
    let url_string = url.as_str().to_owned();
    tab.navigate_to(&url_string)?;
    tab.wait_until_navigated()?;

    // Inject CSS Reset for consistent defaults
    let _ = tab.evaluate(common::css_reset_injection_script(), false)?;

    let script = r#"(function() {
        function shouldSkip(el) {
            if (!el || !el.tagName) return false;
            var tag = String(el.tagName).toLowerCase();
            // Ignore the test-injected reset style element to avoid mismatches
            if (tag === 'style' && el.getAttribute('data-valor-test-reset') === '1') return true;
            return false;
        }
        function ser(el) {
            var r = el.getBoundingClientRect();
            return {
                tag: String(el.tagName||'').toLowerCase(),
                id: String(el.id||''),
                rect: { x: r.x, y: r.y, width: r.width, height: r.height },
                children: Array.from(el.children).filter(function(c){ return !shouldSkip(c); }).map(ser)
            };
        }
        var root = document.body || document.documentElement;
        return JSON.stringify(ser(root));
    })()"#;
    let result = tab.evaluate(script, true)?;
    let value = result.value.ok_or_else(|| anyhow::anyhow!("No value returned from Chromium evaluate"))?;
    let s = value.as_str().ok_or_else(|| anyhow::anyhow!("Chromium returned non-string JSON for layout"))?;
    let parsed: Value = serde_json::from_str(s)?;
    Ok(parsed)
}
