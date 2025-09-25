#![allow(dead_code)]
#![allow(
    clippy::excessive_nesting,
    reason = "diagnostic-only helper code in test harness"
)]
use anyhow::Error;
use css_core::LayoutRect;
use css_core::{LayoutNodeKind, Layouter};
use css_orchestrator::style_model::ComputedStyle;
use headless_chrome::{Browser, LaunchOptionsBuilder};
use js::DOMSubscriber;
use js::DOMUpdate::{EndOfDocument, InsertElement, SetAttr};
use js::NodeKey;
use log::{debug, error, info};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;
use tokio::runtime::Runtime;

mod common;

#[test]
fn run_chromium_layouts() -> Result<(), Error> {
    // Initialize logger honoring RUST_LOG; default to WARN if not set
    let _ = env_logger::Builder::from_env(env_logger::Env::default().filter_or("RUST_LOG", "warn"))
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
    // Optional: focus fixtures via substring match
    // Accepted forms (pass after `--` so cargo forwards to the test binary):
    // - run_chromium_layouts::<substr>  (leverages libtest name filtering without introducing unknown flags)
    // - --layout-filter=<substr>        (if not treated as an unknown flag by your invocation)
    // - --fixture=<substr>
    fn cli_layout_filter() -> Option<String> {
        let mut args = std::env::args();
        // Skip program name
        let _ = args.next();
        let mut pending_value_for: Option<String> = None;
        for arg in args {
            // Piggyback on libtest name filter: run_chromium_layouts::<substr>
            if let Some(rest) = arg.strip_prefix("run_chromium_layouts::")
                && !rest.is_empty()
            {
                return Some(rest.to_string());
            }
            // Accept positional key=value (no leading dashes) to avoid test harness flag errors
            if let Some(rest) = arg.strip_prefix("layout-filter=") {
                return Some(rest.to_string());
            }
            if let Some(rest) = arg.strip_prefix("fixture=") {
                return Some(rest.to_string());
            }
            // Support `--layout-filter=value` form
            if let Some(rest) = arg.strip_prefix("--layout-filter=") {
                return Some(rest.to_string());
            }
            if let Some(rest) = arg.strip_prefix("--fixture=") {
                return Some(rest.to_string());
            }
            // Support `--layout-filter value` form
            if arg == "--layout-filter" || arg == "--fixture" {
                pending_value_for = Some(arg);
                continue;
            }
            if pending_value_for.is_some() {
                return Some(arg);
            }
        }
        None
    }
    let focus = cli_layout_filter();
    if let Some(ref f) = focus {
        info!("[LAYOUT] focusing fixtures containing (CLI): {f}");
    }
    info!("[LAYOUT] discovered {} fixtures", all.len());
    let mut ran = 0;
    for input_path in all {
        if let Some(ref f) = focus {
            let display_name = input_path.display().to_string();
            if !display_name.contains(f) {
                continue;
            }
        }
        let display_name = input_path.display().to_string();
        // Build page and parse via HtmlPage
        let url = common::to_file_url(&input_path)?;
        let mut page = common::create_page(&rt, url)?;
        // Inject the same CSS reset into our engine that we inject into Chromium
        // to ensure both sides receive identical author CSS for fair comparison.
        page.eval_js(common::css_reset_injection_script())?;
        // Attach a Layouter mirror to the page's DOM stream
        let mut layouter_mirror = page.create_mirror(Layouter::new());

        // Drive parsing and mirroring until finished (with timeout), draining the layouter mirror per tick
        let finished = common::update_until_finished(&rt, &mut page, |_| {
            layouter_mirror.try_update_sync()?;
            Ok(())
        })?;
        if !finished {
            let msg = "Parsing did not finish".to_string();
            error!("[LAYOUT] {display_name} ... FAILED: {msg}");
            failed.push((display_name.clone(), msg));
            continue;
        }

        // One more update + drain after parse finished to ensure all mirrors are fully synchronized.
        // This helps when late stylesheet/attr merges happen right at end-of-document.
        let _ = rt.block_on(page.update());
        layouter_mirror.try_update_sync()?;

        // Rebuild the external Layouter mirror from the page's current structure to avoid missing early DOM updates.
        // Use a generic structure snapshot not tied to any internal layouter state.
        let (tags_by_key, element_children) = page.layout_structure_snapshot();
        let attrs_map = page.layouter_attrs_map();
        fn replay_into_layouter(
            lay: &mut Layouter,
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
                let _ = lay.apply_update(InsertElement {
                    parent,
                    node: *child,
                    tag,
                    pos: 0,
                });
                if let Some(map) = attrs.get(child) {
                    for key_name in ["id", "class", "style"] {
                        if let Some(val) = map.get(key_name) {
                            let _ = lay.apply_update(SetAttr {
                                node: *child,
                                name: key_name.to_owned(),
                                value: val.clone(),
                            });
                        }
                    }
                }
                replay_into_layouter(lay, tags_by_key, element_children, attrs, *child);
            }
        }
        {
            let lay = layouter_mirror.mirror_mut();
            replay_into_layouter(
                lay,
                &tags_by_key,
                &element_children,
                &attrs_map,
                NodeKey::ROOT,
            );
            let _ = lay.apply_update(EndOfDocument);
            // Diagnostics: ensure the external layouter has received updates and built a tree
            let updates_applied = lay.perf_updates_applied();
            let snap_tmp = lay.snapshot();
            let blocks_count = snap_tmp
                .iter()
                .filter(|(_, kind, _)| matches!(kind, LayoutNodeKind::Block { .. }))
                .count();
            debug!(
                "[LAYOUT][DIAG] external layouter after replay: updates_applied={updates_applied}, blocks_in_snapshot={blocks_count}"
            );
        }

        // Provide computed styles from the page's internal StyleEngine to Layouter
        let computed = page.computed_styles_snapshot()?;
        // Clone to retain for serialization while also passing into layouter
        let computed_for_serialization = computed.clone();
        let layouter = layouter_mirror.mirror_mut();
        // Also set the current stylesheet on this external layouter mirror so display/flow is consistent
        let sheet_for_layout = page.styles_snapshot()?;
        layouter.set_stylesheet(sheet_for_layout);
        layouter.set_computed_styles(computed);
        let _count = layouter.compute_layout();
        // Diagnostics: ensure the external layouter has received updates and built a tree
        let updates_applied = layouter.perf_updates_applied();
        let snap_tmp = layouter.snapshot();
        let blocks_count = snap_tmp
            .iter()
            .filter(|(_, kind, _)| matches!(kind, LayoutNodeKind::Block { .. }))
            .count();
        debug!(
            "[LAYOUT][DIAG] external layouter after replay: updates_applied={updates_applied}, blocks_in_snapshot={blocks_count}"
        );
        // Use the external Layouter geometry computed above for comparison
        let rects_external = layouter.compute_layout_geometry();

        // Build our full layout JSON starting from the first element child under #document (typically <html>)
        let our_json = our_layout_json(layouter, &rects_external, &computed_for_serialization);

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
                    let msg = format!("JS assertion failed: {name} - {details}");
                    error!("[LAYOUT] {display_name} ... FAILED: {msg}");
                    failed.push((display_name.clone(), msg));
                    continue;
                }
            }
        }
        // Compare layout using epsilon for floats
        let eps = f32::EPSILON as f64 * 3.0;
        match common::compare_json_with_epsilon(&our_json, &ch_layout_json, eps) {
            Ok(_) => {
                info!("[LAYOUT] {display_name} ... ok");
                ran += 1;
            }
            Err(msg) => {
                // Extra diagnostics to aid debugging snapshot/attrs mismatches
                let layouter_ref = layouter_mirror.mirror_mut();
                let snapshot_diag = layouter_ref.snapshot();
                let attrs_map = layouter_ref.attrs_map();
                let mut blocks = 0usize;
                let mut root_children = Vec::new();
                for (k, kind, children) in snapshot_diag.iter().cloned() {
                    if matches!(kind, LayoutNodeKind::Block { .. }) {
                        blocks += 1;
                    }
                    if k == NodeKey::ROOT {
                        root_children = children;
                    }
                }
                debug!(
                    "[LAYOUT][DIAG] blocks={}, root_children_count={}, attrs_nodes={}",
                    blocks,
                    root_children.len(),
                    attrs_map.len()
                );
                let mut first_elem: Option<NodeKey> = None;
                for child in &root_children {
                    let is_block = snapshot_diag.iter().any(|(k, kind, _)| {
                        *k == *child && matches!(kind, LayoutNodeKind::Block { .. })
                    });
                    if is_block {
                        first_elem = Some(*child);
                        break;
                    }
                }
                if let Some(elem) = first_elem {
                    let mut child_count = 0usize;
                    for (k, _kind, kids) in &snapshot_diag {
                        if *k == elem {
                            child_count = kids.len();
                            break;
                        }
                    }
                    debug!(
                        "[LAYOUT][DIAG] first element child under ROOT has {child_count} children"
                    );
                }
                // Defer logging to the consolidated error block below to avoid duplicate output.
                failed.push((display_name.clone(), msg));
            }
        }
    }
    if !failed.is_empty() {
        error!("==== LAYOUT FAILURES ({} total) ====", failed.len());
        for (name, msg) in &failed {
            error!("- {name}\n  {msg}\n");
        }
        panic!("{} layout fixture(s) failed; see log above.", failed.len());
    }
    info!("[LAYOUT] {ran} fixtures passed");
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
    // Choose the same outer root Chromium serializes: prefer <body>, else <html>, else first block.
    let mut body_key: Option<NodeKey> = None;
    let mut html_key: Option<NodeKey> = None;
    for (k, kind) in kind_by_key.iter() {
        if let LayoutNodeKind::Block { tag } = kind {
            if tag.eq_ignore_ascii_case("body") {
                body_key = Some(*k);
                break;
            }
            if tag.eq_ignore_ascii_case("html") && html_key.is_none() {
                html_key = Some(*k);
            }
        }
    }
    let mut root_elem: Option<NodeKey> = body_key.or(html_key);
    if root_elem.is_none() {
        // Fallback: first block under ROOT, then any block anywhere
        if let Some(children) = children_by_key.get(&NodeKey::ROOT) {
            for child in children {
                if matches!(kind_by_key.get(child), Some(LayoutNodeKind::Block { .. })) {
                    root_elem = Some(*child);
                    break;
                }
            }
        }
        if root_elem.is_none() {
            for (k, kind) in kind_by_key.iter() {
                if matches!(kind, LayoutNodeKind::Block { .. }) {
                    root_elem = Some(*k);
                    break;
                }
            }
        }
    }
    let root_key = root_elem.unwrap_or(NodeKey::ROOT);
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
    // NOTE: The rect serialized here is the border-box rect from the layouter.
    // Chromium's side uses getBoundingClientRect(), which is also border-box.
    fn is_non_rendering_tag(tag: &str) -> bool {
        matches!(
            tag,
            "head" | "meta" | "title" | "link" | "style" | "script" | "base"
        )
    }

    fn to_px_or_auto(sz: &SizeSpecified) -> String {
        use LengthOrAuto::*;
        match sz {
            Pixels(v) => format!("{v}px"),
            Auto => "auto".to_string(),
            Percent(p) => format!("{p}%"),
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
    fn box_sizing_to_str(b: &BoxSizing) -> &'static str {
        match b {
            BoxSizing::BorderBox => "border-box",
            BoxSizing::ContentBox => "content-box",
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
                    children.iter().for_each(|c| {
                        let mut include = false;
                        if let Some(LayoutNodeKind::Block { tag }) = ctx.kind_by_key.get(c) {
                            let not_non_rendering =
                                !is_non_rendering_tag(tag.to_lowercase().as_str());
                            let not_display_none = ctx
                                .computed
                                .get(c)
                                .map(|cs| !matches!(cs.display, Display::None))
                                .unwrap_or(true);
                            include = not_non_rendering && not_display_none;
                        }
                        if include {
                            children_json.push(recurse(ctx, *c));
                        }
                    });
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
                        "boxSizing": box_sizing_to_str(&cs.box_sizing),
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
            // Skip elements that do not generate a box
            try {
                var cs = window.getComputedStyle(el);
                if (cs && String(cs.display||'').toLowerCase() === 'none') return true;
            } catch (e) { /* ignore */ }
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
