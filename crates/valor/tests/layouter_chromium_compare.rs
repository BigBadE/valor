use anyhow::{Error, Result};
use css::style_types::{AlignItems, BoxSizing, ComputedStyle, Display, Overflow};
use css_core::LayoutRect;
use css_core::{LayoutNodeKind, Layouter};
use env_logger::Builder;
use env_logger::Env as EnvLoggerEnv;
use headless_chrome::{Browser, LaunchOptionsBuilder, Tab};
use js::DOMSubscriber as _;
use js::DOMUpdate::{EndOfDocument, InsertElement, SetAttr};
use js::NodeKey;
use log::{error, info};
use serde_json::{Value, from_str, json};
use std::collections::HashMap;
use std::env;
use std::ffi::OsStr;
use std::path::Path;
use std::time::Duration;
use tokio::runtime::Runtime;
use valor::test_support::{
    create_page, read_cached_json_for_fixture, to_file_url, update_until_finished,
    write_cached_json_for_fixture, write_named_json_for_fixture,
};

mod common;

#[cfg(test)]
mod tests {
    use super::*;

    type LayouterWithStyles = (Layouter, HashMap<NodeKey, ComputedStyle>);

    /// Recursively replay DOM tree structure into a layouter.
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
            drop(layouter.apply_update(InsertElement {
                parent,
                node: *child,
                tag,
                pos: 0,
            }));
            if let Some(attr_map) = attrs.get(child) {
                apply_element_attrs(layouter, *child, attr_map);
            }
            replay_into_layouter(layouter, tags_by_key, element_children, attrs, *child);
        }
    }

    /// Helper function to apply element attributes (id, class, style) to a layouter node
    fn apply_element_attrs(
        layouter: &mut Layouter,
        node: NodeKey,
        attrs: &HashMap<String, String>,
    ) {
        for key_name in ["id", "class", "style"] {
            if let Some(val) = attrs.get(key_name) {
                drop(layouter.apply_update(SetAttr {
                    node,
                    name: key_name.to_owned(),
                    value: val.clone(),
                }));
            }
        }
    }

    /// Parse CLI arguments for layout filter substring.
    fn cli_layout_filter() -> Option<String> {
        let mut args = env::args();
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

    /// Set up a headless Chrome browser for layout testing.
    ///
    /// # Errors
    /// Returns an error if the browser fails to launch.
    fn setup_chrome_browser() -> Result<Browser> {
        let launch_opts = LaunchOptionsBuilder::default()
            .headless(true)
            .window_size(Some((800, 600)))
            .idle_browser_timeout(Duration::from_secs(300))
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
            .build()?;
        Browser::new(launch_opts)
    }

    /// Set up a layouter for a fixture HTML file.
    ///
    /// # Errors
    /// Returns an error if the page fails to load or layout computation fails.
    fn setup_layouter_for_fixture(
        runtime: &Runtime,
        input_path: &Path,
    ) -> Result<LayouterWithStyles> {
        let url = to_file_url(input_path)?;
        let mut page = create_page(runtime, url)?;
        page.eval_js(common::css_reset_injection_script())?;
        let mut layouter_mirror = page.create_mirror(Layouter::new());

        let finished = update_until_finished(runtime, &mut page, |_page| {
            layouter_mirror.try_update_sync()?;
            Ok(())
        })?;

        if !finished {
            return Err(anyhow::anyhow!("Parsing did not finish"));
        }

        drop(runtime.block_on(page.update()));
        drop(layouter_mirror.try_update_sync());

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
            drop(layouter.apply_update(EndOfDocument));
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

    fn check_js_assertions(
        ch_json: &Value,
        display_name: &str,
        failed: &mut Vec<(String, String)>,
    ) {
        if let Some(asserts) = ch_json.get("asserts")
            && let Some(arr) = asserts.as_array()
        {
            for entry in arr {
                let assert_name = entry.get("name").and_then(Value::as_str).unwrap_or("");
                let assertion_passed = entry.get("ok").and_then(Value::as_bool).unwrap_or(false);
                let assert_details = entry.get("details").and_then(Value::as_str).unwrap_or("");
                if !assertion_passed {
                    let msg = format!("JS assertion failed: {assert_name} - {assert_details}");
                    error!("[LAYOUT] {display_name} ... FAILED: {msg}");
                    failed.push((display_name.to_string(), msg));
                }
            }
        }
    }

    /// Run layout comparison tests against Chromium.
    ///
    /// # Errors
    /// Returns an error if any test fails or if setup fails.
    #[test]
    fn run_chromium_layouts() -> Result<(), Error> {
        // Initialize logger honoring RUST_LOG; default to WARN if not set
        drop(
            Builder::from_env(EnvLoggerEnv::default().filter_or("RUST_LOG", "warn"))
                .is_test(false)
                .try_init(),
        );

        let harness_src = include_str!("layouter_chromium_compare.rs");
        drop(common::clear_valor_layout_cache_if_harness_changed(
            harness_src,
        ));

        let browser = setup_chrome_browser()?;
        let tab = browser.new_tab()?;

        let mut failed: Vec<(String, String)> = Vec::new();
        let runtime = Runtime::new()?;
        let all = common::fixture_html_files()?;
        let focus = cli_layout_filter();
        if let Some(f) = &focus {
            info!("[LAYOUT] focusing fixtures containing (CLI): {f}");
        }
        info!("[LAYOUT] discovered {} fixtures", all.len());
        let mut ran = 0;

        for input_path in all {
            let display_name = input_path.display().to_string();

            let (mut layouter, computed_for_serialization) =
                match setup_layouter_for_fixture(&runtime, &input_path) {
                    Ok(result) => result,
                    Err(err) => {
                        let msg = format!("Setup failed: {err}");
                        error!("[LAYOUT] {display_name} ... FAILED: {msg}");
                        failed.push((display_name.clone(), msg));
                        continue;
                    }
                };

            let rects_external = layouter.compute_layout_geometry();

            let our_json = our_layout_json(&layouter, &rects_external, &computed_for_serialization);

            let ch_json = if let Some(cached_value) =
                read_cached_json_for_fixture(&input_path, harness_src)
            {
                cached_value
            } else {
                let chromium_value = chromium_layout_json_in_tab(&tab, &input_path)?;
                write_cached_json_for_fixture(&input_path, harness_src, &chromium_value)?;
                chromium_value
            };

            write_named_json_for_fixture(&input_path, harness_src, "chromium", &ch_json)?;
            write_named_json_for_fixture(&input_path, harness_src, "valor", &our_json)?;

            check_js_assertions(&ch_json, &display_name, &mut failed);

            let ch_layout_json =
                if ch_json.get("layout").is_some() || ch_json.get("asserts").is_some() {
                    ch_json.get("layout").cloned().unwrap_or_else(|| json!({}))
                } else {
                    ch_json.clone()
                };

            let eps = f64::from(f32::EPSILON) * 3.0;
            match common::compare_json_with_epsilon(&our_json, &ch_layout_json, eps) {
                Ok(()) => {
                    info!("[LAYOUT] {display_name} ... ok");
                    ran += 1;
                }
                Err(msg) => {
                    failed.push((display_name.clone(), msg));
                }
            }
        }
        if failed.is_empty() {
            info!("[LAYOUT] {ran} fixtures passed");
            Ok(())
        } else {
            error!("==== LAYOUT FAILURES ({} total) ====", failed.len());
            for (name, msg) in &failed {
                error!("- {name}\n  {msg}\n");
            }
            Err(anyhow::anyhow!(
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
            Display::None => "none",
        }
    }

    fn build_style_json(computed: &ComputedStyle) -> Value {
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

    fn collect_children_json(ctx: &LayoutCtx<'_>, key: NodeKey) -> Vec<Value> {
        let mut kids_json: Vec<Value> = Vec::new();
        if let Some(children) = ctx.children_by_key.get(&key) {
            for child in children {
                if matches!(
                    ctx.kind_by_key.get(child),
                    Some(LayoutNodeKind::Block { .. })
                ) {
                    kids_json.push(serialize_element_subtree(ctx, *child));
                }
            }
        }
        kids_json
    }

    fn serialize_element_subtree(ctx: &LayoutCtx<'_>, key: NodeKey) -> Value {
        let mut out = json!({});
        if let Some(LayoutNodeKind::Block { tag }) = ctx.kind_by_key.get(&key) {
            if is_non_rendering_tag(tag) {
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
                obj.insert("children".to_owned(), Value::Array(kids_json));
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

        // Search in ROOT's children
        if let Some(children) = children_by_key.get(&NodeKey::ROOT) {
            for child in children {
                if matches!(kind_by_key.get(child), Some(LayoutNodeKind::Block { .. })) {
                    return Some(*child);
                }
            }
        }

        // Fallback: any block element
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
    ) -> Value {
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

    /// Returns the JavaScript code for extracting layout information from Chromium.
    fn chromium_layout_extraction_script() -> &'static str {
        "(function() {
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
    })()"
    }

    /// Extract layout JSON from Chromium by navigating to an HTML file and executing JavaScript.
    ///
    /// # Errors
    /// Returns an error if navigation, script evaluation, or JSON parsing fails.
    fn chromium_layout_json_in_tab(tab: &Tab, path: &Path) -> Result<Value> {
        // Convert the file path to a file:// URL
        let url = to_file_url(path)?;

        // Use an owned String to avoid any possibility of truncated/borrowed URL issues in downstream logging/transport.
        let url_string = url.as_str().to_owned();
        tab.navigate_to(&url_string)?;
        tab.wait_until_navigated()?;

        // Inject CSS Reset for consistent defaults
        let _ = tab.evaluate(common::css_reset_injection_script(), false)?;

        let script = chromium_layout_extraction_script();
        let result = tab.evaluate(script, true)?;
        let value = result
            .value
            .ok_or_else(|| anyhow::anyhow!("No value returned from Chromium evaluate"))?;
        let json_string = value
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Chromium returned non-string JSON for layout"))?;
        let parsed: Value = from_str(json_string)?;
        Ok(parsed)
    }
}
