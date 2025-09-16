#![allow(dead_code)]
use anyhow::Error;
use headless_chrome::{Browser, LaunchOptionsBuilder};
use js::NodeKey;
use layouter::LayoutRect;
use layouter::{LayoutNodeKind, Layouter};
use log::info;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use style_engine::{
    AlignItems, ComputedStyle, Display, Edges, LengthOrAuto, Overflow, SizeSpecified,
};
use tokio::runtime::Runtime;

mod common;

#[test]
fn run_chromium_layouts() -> Result<(), Error> {
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
            // Speed/consistency tweaks
            OsStr::new("--disable-extensions"),
            OsStr::new("--disable-background-networking"),
            OsStr::new("--disable-sync"),
            OsStr::new("--hide-scrollbars"),
            OsStr::new("--blink-settings=imagesEnabled=false"),
        ])
        .build()
        .expect("Failed to build LaunchOptions for headless_chrome");
    let browser = Browser::new(launch_opts).expect("Failed to launch headless Chrome browser");
    // Create a single tab and reuse it for all fixtures to avoid per-tab overhead.
    let tab = browser.new_tab()?;

    let mut failed: Vec<(String, String)> = Vec::new();
    // Reuse a single Tokio runtime for all fixtures to reduce per-file overhead.
    let rt = Runtime::new()?;
    // Iterate over filtered fixtures and compare against each expected file content.
    let all = common::fixture_html_files()?;
    info!("[LAYOUT] discovered {} fixtures", all.len());

    for input_path in all {
        let display_name = input_path.display().to_string();
        // XFAIL mechanism: skip any fixture that contains the marker
        if let Ok(text) = fs::read_to_string(&input_path)
            && (text.contains("VALOR_XFAIL") || text.contains("valor-xfail"))
        {
            info!("[LAYOUT] {} ... XFAIL (skipped)", display_name);
            continue;
        }
        // Build page and parse via HtmlPage
        let url = common::to_file_url(&input_path)?;
        let mut page = common::create_page(&rt, url)?;
        // Attach a Layouter mirror to the page's DOM stream
        let mut layouter_mirror = page.create_mirror(Layouter::new());

        // Drive parsing and mirroring until finished (with timeout), draining the layouter mirror per tick
        let finished = common::update_until_finished(&rt, &mut page, |_| {
            layouter_mirror.try_update_sync()?;
            Ok(())
        })?;
        if !finished {
            let msg = "Parsing did not finish".to_string();
            eprintln!("[LAYOUT] {} ... FAILED: {}", display_name, msg);
            failed.push((display_name.clone(), msg));
            continue;
        }

        // Provide computed styles from the page's internal StyleEngine to Layouter
        let computed = page.computed_styles_snapshot()?;
        // Clone to retain for serialization while also passing into layouter
        let computed_for_serialization = computed.clone();
        let layouter = layouter_mirror.mirror_mut();
        layouter.set_computed_styles(computed);
        let _count = layouter.compute_layout();
        let rects = layouter.compute_layout_geometry();

        // Build our full layout JSON starting from the first element child under #document (typically <html>)
        let our_json = our_layout_json(layouter, &rects, &computed_for_serialization);

        // Build or load Chromium's full layout JSON by evaluating JS in the page using the shared tab
        let harness_src = include_str!("layouter_chromium_compare.rs");
        let ch_json =
            if let Some(v) = common::read_cached_json_for_fixture(&input_path, harness_src) {
                v
            } else {
                let value = chromium_layout_json_in_tab(&tab, &input_path)?;
                common::write_cached_json_for_fixture(&input_path, harness_src, &value)?;
                value
            };

        // Persist both JSONs for inspection
        common::write_named_json_for_fixture(&input_path, harness_src, "chromium", &ch_json)?;
        common::write_named_json_for_fixture(&input_path, harness_src, "valor", &our_json)?;

        // If Chromium JSON includes JS assertion results, evaluate them first.
        let (ch_layout_json, js_asserts_opt) =
            if ch_json.get("layout").is_some() || ch_json.get("asserts").is_some() {
                (
                    ch_json.get("layout").cloned().unwrap_or_else(|| json!({})),
                    ch_json.get("asserts").cloned(),
                )
            } else {
                (ch_json.clone(), None)
            };
        if let Some(asserts) = js_asserts_opt
            && let Some(arr) = asserts.as_array()
        {
            for entry in arr {
                let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let ok = entry.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                let details = entry.get("details").and_then(|v| v.as_str()).unwrap_or("");
                if !ok {
                    let msg = format!("JS assertion failed: {} - {}", name, details);
                    eprintln!("[LAYOUT] {} ... FAILED: {}", display_name, msg);
                    failed.push((display_name.clone(), msg));
                    continue;
                }
            }
        }
        // Compare layout using epsilon for floats
        let eps = f32::EPSILON as f64 * 3.0;
        match common::compare_json_with_epsilon(&our_json, &ch_layout_json, eps) {
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

#[allow(dead_code)]
fn our_layout_json(
    layouter: &Layouter,
    rects: &HashMap<NodeKey, LayoutRect>,
    computed: &HashMap<NodeKey, ComputedStyle>,
) -> Value {
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
            if matches!(kind_by_key.get(child), Some(LayoutNodeKind::Block { .. })) {
                root_elem = Some(*child);
                break;
            }
        }
    }
    // If the root is <html>, prefer serializing from its <body> child to avoid Chromium's root-margin quirks
    let mut root_key = root_elem.unwrap_or(NodeKey::ROOT);
    if let Some(LayoutNodeKind::Block { tag }) = kind_by_key.get(&root_key)
        && tag.eq_ignore_ascii_case("html")
        && let Some(children) = children_by_key.get(&root_key)
        && let Some(body_child) = children.iter().find(|c| matches!(kind_by_key.get(*c), Some(LayoutNodeKind::Block { tag }) if tag.eq_ignore_ascii_case("body")))
    { root_key = *body_child; }
    {
        let ctx = LayoutCtx {
            kind_by_key: &kind_by_key,
            children_by_key: &children_by_key,
            attrs_by_key: &attrs_by_key,
            rects,
            computed,
        };
        serialize_element_subtree(&ctx, root_key)
    }
}

#[allow(dead_code)]
struct LayoutCtx<'a> {
    kind_by_key: &'a HashMap<NodeKey, LayoutNodeKind>,
    children_by_key: &'a HashMap<NodeKey, Vec<NodeKey>>,
    attrs_by_key: &'a HashMap<NodeKey, HashMap<String, String>>,
    rects: &'a HashMap<NodeKey, LayoutRect>,
    computed: &'a HashMap<NodeKey, ComputedStyle>,
}

#[allow(dead_code)]
fn serialize_element_subtree(ctx: &LayoutCtx<'_>, key: NodeKey) -> Value {
    fn is_non_rendering_tag(tag: &str) -> bool {
        matches!(
            tag,
            "head" | "meta" | "title" | "link" | "style" | "script" | "base"
        )
    }

    fn to_px_or_auto(sz: &SizeSpecified) -> String {
        use LengthOrAuto::*;
        match sz {
            Pixels(v) => format!("{}px", v),
            Auto => "auto".to_string(),
            Percent(p) => format!("{}%", p),
        }
    }

    fn effective_display(d: &Display) -> &'static str {
        use Display::*;
        // Our layouter snapshot only contains block-level boxes; default to 'block' unless explicitly Flex.
        match d {
            Flex | InlineFlex => "flex",
            _ => "block",
        }
    }

    fn edges_to_map_px(e: &Edges) -> Value {
        json!({
            "top": format!("{}px", e.top as f64),
            "right": format!("{}px", e.right as f64),
            "bottom": format!("{}px", e.bottom as f64),
            "left": format!("{}px", e.left as f64),
        })
    }
    fn align_items_to_str(a: &AlignItems) -> &'static str {
        use AlignItems::*;
        match a {
            Stretch => "stretch",
            FlexStart => "flex-start",
            FlexEnd => "flex-end",
            Center => "center",
            Baseline => "baseline",
        }
    }
    fn overflow_to_str(o: &Overflow) -> &'static str {
        use Overflow::*;
        match o {
            Visible => "visible",
            Hidden => "hidden",
            Scroll => "scroll",
            Auto => "auto",
        }
    }
    fn recurse(ctx: &LayoutCtx<'_>, key: NodeKey) -> Value {
        match ctx.kind_by_key.get(&key) {
            Some(LayoutNodeKind::Block { tag }) => {
                let mut rect_json = json!({"x": 0.0, "y": 0.0, "width": 0.0, "height": 0.0});
                if !is_non_rendering_tag(tag.to_lowercase().as_str())
                    && let Some(r) = ctx.rects.get(&key)
                {
                    rect_json = json!({
                        "x": r.x as f64,
                        "y": r.y as f64,
                        "width": r.width as f64,
                        "height": r.height as f64,
                    });
                }
                let mut children_json = Vec::new();
                if let Some(children) = ctx.children_by_key.get(&key) {
                    children
                        .iter()
                        .filter(|c| {
                            matches!(ctx.kind_by_key.get(*c), Some(LayoutNodeKind::Block { .. }))
                        })
                        .for_each(|c| children_json.push(recurse(ctx, *c)));
                }
                let mut obj = json!({
                    "tag": tag.to_lowercase(),
                    "rect": rect_json,
                    "children": children_json,
                });
                let id_val = ctx
                    .attrs_by_key
                    .get(&key)
                    .and_then(|attrs| attrs.get("id").cloned())
                    .unwrap_or_default();
                obj["id"] = json!(id_val);
                // Attach a subset of computed styles for deeper comparison
                if let Some(cs) = ctx.computed.get(&key) {
                    let eff_disp = effective_display(&cs.display);
                    let margin_json =
                        if tag.eq_ignore_ascii_case("html") || tag.eq_ignore_ascii_case("body") {
                            json!({"top":"0px","right":"0px","bottom":"0px","left":"0px"})
                        } else {
                            edges_to_map_px(&cs.margin)
                        };
                    let style_json = json!({
                        "display": eff_disp,
                        "flexBasis": to_px_or_auto(&cs.flex_basis),
                        "flexGrow": cs.flex_grow as f64,
                        "flexShrink": cs.flex_shrink as f64,
                        "margin": margin_json,
                        "padding": edges_to_map_px(&cs.padding),
                        "borderWidth": edges_to_map_px(&cs.border_width),
                        "alignItems": if eff_disp == "flex" {
                            // Chromium serializes default align-items as 'normal' rather than 'stretch'
                            match cs.align_items { AlignItems::Stretch => "normal", _ => align_items_to_str(&cs.align_items) }
                        } else { "normal" },
                        "overflow": overflow_to_str(&cs.overflow),
                    });
                    obj["style"] = style_json;
                }
                obj
            }
            Some(LayoutNodeKind::Document) | Some(LayoutNodeKind::InlineText { .. }) | None => {
                // For document or text nodes, dive into children to find first element
                if let Some(children) = ctx.children_by_key.get(&key)
                    && let Some(first_block) = children.iter().find(|c| {
                        matches!(ctx.kind_by_key.get(*c), Some(LayoutNodeKind::Block { .. }))
                    })
                {
                    return recurse(ctx, *first_block);
                }
                // Fallback empty
                json!({"tag": "", "rect": {"x":0.0,"y":0.0,"width":0.0,"height":0.0}, "children": []})
            }
        }
    }
    recurse(ctx, key)
}

#[allow(dead_code)]
fn chromium_layout_json_in_tab(tab: &headless_chrome::Tab, path: &Path) -> anyhow::Result<Value> {
    // Convert the file path to a file:// URL
    let url = common::to_file_url(path)?;

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
        function pickStyle(el, cs) {
            // Return only the subset that matches our layouter's serialization.
            // For display, mirror our 'effective' behavior: treat non-flex as 'block'.
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
        // Ensure assertion channel exists and optionally execute a user-provided runner
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
    })()"#;
    let result = tab.evaluate(script, true)?;
    let value = result
        .value
        .ok_or_else(|| anyhow::anyhow!("No value returned from Chromium evaluate"))?;
    let s = value
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Chromium returned non-string JSON for layout"))?;
    let parsed: Value = serde_json::from_str(s)?;
    Ok(parsed)
}
