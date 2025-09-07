use anyhow::Error;
use layouter::{LayoutNodeKind, Layouter};
use html::dom::NodeKey;
use layouter::LayoutRect;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tokio::runtime::Runtime;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use headless_chrome::{Browser, LaunchOptionsBuilder};

mod common;

// Shared headless Chrome browser for all test files
static SHARED_BROWSER: Lazy<Mutex<Browser>> = Lazy::new(|| {
    let launch_opts = LaunchOptionsBuilder::default()
        .headless(true)
        .window_size(Some((800, 600)))
        .build()
        .expect("Failed to build LaunchOptions for headless_chrome");
    let browser = Browser::new(launch_opts)
        .expect("Failed to launch shared headless Chrome browser");
    Mutex::new(browser)
});

#[test]
fn chromium_layout_test() -> Result<(), Error> {
    let _ = env_logger::builder().is_test(true).try_init();
    // Iterate over all fixtures and compare against each expected file content.
    for expected_path in common::fixture_html_files()? {
        // Map expected layout fixture name to the corresponding input HTML under crates/valor/fixtures
        let file_name = expected_path
            .file_name()
            .expect("fixture file name");
        let input_path: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join(file_name);

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
        assert!(finished, "Parsing did not finish for {}", input_path.display());

        // Compute layout and geometry from the mirror's layouter
        let layouter = layouter_mirror.mirror_mut();
        let _count = layouter.compute_layout();
        let rects = layouter.compute_layout_geometry();

        // Build our full layout JSON starting from the first element child under #document (typically <html>)
        let our_json = our_layout_json(layouter, &rects);

        // Build Chromium's full layout JSON by evaluating JS in the page
        let ch_json = chromium_layout_json_for_path(&input_path)?;

        // Compare using epsilon for floats
        let eps = 8.0_f64;
        if let Err(msg) = common::compare_json_with_epsilon(&our_json, &ch_json, eps) {
            panic!(
                "Chromium layout JSON mismatch for {} (eps={}).\n{}\nOur: {}\nChromium: {}\n",
                input_path.display(),
                eps,
                msg,
                serde_json::to_string_pretty(&our_json).unwrap_or_default(),
                serde_json::to_string_pretty(&ch_json).unwrap_or_default()
            );
        }

        // Compare pretty-printed layout tree against expected fixture
        let printed = format!("{:?}", layouter);
        let expected = fs::read_to_string(&expected_path)?;

        assert_eq!(
            normalize(&printed),
            normalize(&expected),
            "Layout pretty-print differs from Chromium fixture ({}).\n--- Printed ---\n{}\n--- Expected ---\n{}\n",
            expected_path.display(),
            printed,
            expected
        );
    }
    Ok(())
}

fn normalize(s: &str) -> String {
    s.replace("\r\n", "\n")
        .lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
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
    serialize_element_subtree_with_maps(&kind_by_key, &children_by_key, rects, root_key)
}

fn serialize_element_subtree_with_maps(
    kind_by_key: &HashMap<NodeKey, LayoutNodeKind>,
    children_by_key: &HashMap<NodeKey, Vec<NodeKey>>,
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
                            let v = recurse(*child, kind_by_key, children_by_key, rects);
                            children_json.push(v);
                        }
                    }
                }
                json!({
                    "tag": tag.to_lowercase(),
                    "rect": rect_json,
                    "children": children_json,
                })
            }
            Some(LayoutNodeKind::Document) | Some(LayoutNodeKind::InlineText { .. }) | None => {
                // For document or text nodes, dive into children to find first element
                if let Some(children) = children_by_key.get(&key) {
                    for child in children {
                        if let Some(LayoutNodeKind::Block { .. }) = kind_by_key.get(child) {
                            return recurse(*child, kind_by_key, children_by_key, rects);
                        }
                    }
                }
                // Fallback empty
                json!({"tag": "", "rect": {"x":0.0,"y":0.0,"width":0.0,"height":0.0}, "children": []})
            }
        }
    }
    recurse(key, kind_by_key, children_by_key, rects)
}

fn chromium_layout_json_for_path(path: &Path) -> anyhow::Result<Value> {
    // Convert the file path to a file:// URL
    let url = common::to_file_url(path)?;

    // Reuse the shared headless Chrome browser; open a fresh tab per file
    let tab = {
        let browser_guard = SHARED_BROWSER
            .lock()
            .expect("Failed to lock shared headless Chrome browser");
        let tab = browser_guard.new_tab()?;
        drop(browser_guard); // release lock early
        tab
    };

    tab.navigate_to(url.as_str())?;
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
