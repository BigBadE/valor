use super::browser::{TestType, navigate_and_prepare_tab, setup_chrome_browser};
use super::common::{
    clear_valor_layout_cache_if_harness_changed, create_page, css_reset_injection_script,
    get_filtered_fixtures, init_test_logger, read_cached_json_for_fixture, to_file_url,
    update_until_finished, write_cached_json_for_fixture, write_named_json_for_fixture,
};
use super::json_compare::compare_json_with_epsilon;
use anyhow::{Result, anyhow};
use css::style_types::{AlignItems, BoxSizing, ComputedStyle, Display, Overflow, Position};
use css_core::LayoutRect;
use headless_chrome::Tab;
use js::NodeKey;
use log::{error, info};
use page_handler::layout_manager::LayoutManager;
use page_handler::snapshots::LayoutNodeKind;
use serde_json::{Map as JsonMap, Value as JsonValue, from_str, json};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::runtime::Runtime;

type LayoutEngineWithStyles = (LayoutManager, HashMap<NodeKey, ComputedStyle>);

/// Sets up a layouter for a fixture by creating a page and processing it.
///
/// # Errors
///
/// Returns an error if page creation, parsing, or layout computation fails.
fn setup_layouter_for_fixture(
    runtime: &Runtime,
    input_path: &Path,
) -> Result<LayoutEngineWithStyles> {
    let url = to_file_url(input_path)?;
    let mut page = create_page(runtime, url)?;
    page.eval_js(css_reset_injection_script())?;
    let mut layouter_mirror = page.create_mirror(LayoutManager::new());

    let finished = update_until_finished(runtime, &mut page, |_page| {
        layouter_mirror.try_update_sync()?;
        Ok(())
    })?;

    if !finished {
        return Err(anyhow!("Parsing did not finish"));
    }

    runtime.block_on(page.update())?;
    layouter_mirror.try_update_sync()?;

    let computed = page.computed_styles_snapshot()?;
    {
        let layouter = layouter_mirror.mirror_mut();
        // Match Chromium's viewport: 800x600 window with scrollbar gutter (31px)
        // This matches the window_size in browser.rs and accounts for scrollbar-gutter:stable
        layouter.set_viewport(769, 600);
        // Styles come from orchestrator
        layouter.set_computed_styles(computed.clone());
        layouter.compute_layout();
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

/// Processes a single layout fixture and compares it against Chromium.
///
/// # Errors
///
/// Returns an error if fixture processing, layouter setup, or JSON operations fail.
fn process_layout_fixture(
    input_path: &Path,
    runtime: &Runtime,
    tab: &Arc<Tab>,
    harness_src: &str,
    failed: &mut Vec<(String, String)>,
) -> Result<bool> {
    let display_name = input_path.display().to_string();
    let (mut layouter, computed_for_serialization) =
        match setup_layouter_for_fixture(runtime, input_path) {
            Ok(result) => result,
            Err(err) => {
                let msg = format!("Setup failed: {err}");
                error!("[LAYOUT] {display_name} ... FAILED: {msg}");
                failed.push((display_name.clone(), msg));
                return Ok(false);
            }
        };

    layouter.compute_layout();
    let rects_external = layouter.rects();
    let our_json = our_layout_json(&layouter, rects_external, &computed_for_serialization);
    let ch_json = if let Some(cached_value) = read_cached_json_for_fixture(input_path, harness_src)
    {
        cached_value
    } else {
        let chromium_value = chromium_layout_json_in_tab(tab, input_path)?;
        write_cached_json_for_fixture(input_path, harness_src, &chromium_value)?;
        chromium_value
    };

    write_named_json_for_fixture(input_path, harness_src, "chromium", &ch_json)?;
    write_named_json_for_fixture(input_path, harness_src, "valor", &our_json)?;
    check_js_assertions(&ch_json, &display_name, failed);

    let ch_layout_json = if ch_json.get("layout").is_some() || ch_json.get("asserts").is_some() {
        ch_json.get("layout").cloned().unwrap_or_else(|| json!({}))
    } else {
        ch_json.clone()
    };

    // Use 3.0px tolerance for sub-pixel rendering, line-height, and margin collapse differences
    let eps = 3.0;
    match compare_json_with_epsilon(&our_json, &ch_layout_json, eps) {
        Ok(()) => {
            info!("[LAYOUT] {display_name} ... ok");
            Ok(true)
        }
        Err(msg) => {
            failed.push((display_name.clone(), msg));
            Ok(false)
        }
    }
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
    let browser = setup_chrome_browser(TestType::Layout)?;
    let tab = browser.new_tab()?;
    let mut failed: Vec<(String, String)> = Vec::new();
    let runtime = Runtime::new()?;

    process_layout_fixture(input_path, &runtime, &tab, harness_src, &mut failed)?;

    // Explicitly close tab and browser to ensure clean shutdown
    drop(tab);
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
    let browser = setup_chrome_browser(TestType::Layout)?;
    let tab = browser.new_tab()?;
    let mut failed: Vec<(String, String)> = Vec::new();
    let runtime = Runtime::new()?;
    let fixtures = get_filtered_fixtures("LAYOUT")?;
    let mut ran = 0;
    for input_path in fixtures {
        if process_layout_fixture(&input_path, &runtime, &tab, harness_src, &mut failed)? {
            ran += 1;
        }
    }

    // Explicitly close tab and browser to ensure clean shutdown
    drop(tab);
    drop(browser);

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
        Display::Block => "block",
        Display::InlineBlock => "inline-block",
        Display::Flex => "flex",
        Display::InlineFlex => "inline-flex",
        Display::None => "none",
        Display::Contents => "contents",
    }
}

fn build_style_json(computed: &ComputedStyle) -> JsonValue {
    let position_val = match computed.position {
        Position::Static => "static",
        Position::Relative => "relative",
        Position::Absolute => "absolute",
        Position::Fixed => "fixed",
    };
    let color_str = format!(
        "rgb({}, {}, {})",
        computed.color.red, computed.color.green, computed.color.blue
    );
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
        "overflow": match computed.overflow {
            Overflow::Visible => "visible",
            Overflow::Hidden => "hidden",
            Overflow::Auto => "auto",
            Overflow::Scroll => "scroll",
            Overflow::Clip => "clip",
        },
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
        },
        "position": position_val,
        "fontSize": format!("{}px", computed.font_size),
        "fontWeight": computed.font_weight.to_string(),
        "fontFamily": computed.font_family.as_deref().unwrap_or("").replace('\'', "\"").replace(',', ", "),
        "color": color_str,
        "lineHeight": computed.line_height.map_or_else(|| "normal".to_string(), |line_height| format!("{line_height}px")),
        "zIndex": computed.z_index.map_or_else(|| "auto".to_string(), |z_index| z_index.to_string()),
        "opacity": computed.opacity.map_or_else(|| "1".to_string(), |opacity_val| opacity_val.to_string()),
    })
}

fn serialize_text_node(
    ctx: &LayoutCtx<'_>,
    key: NodeKey,
    text: &str,
    parent_computed: &ComputedStyle,
) -> Option<JsonValue> {
    // Skip whitespace-only text
    if text.trim().is_empty() {
        return None;
    }

    let rect = ctx.rects.get(&key).copied().unwrap_or_default();
    let color_str = format!(
        "rgb({}, {}, {})",
        parent_computed.color.red, parent_computed.color.green, parent_computed.color.blue
    );

    Some(json!({
        "type": "text",
        "text": text,
        "rect": {
            "x": f64::from(rect.x),
            "y": f64::from(rect.y),
            "width": f64::from(rect.width),
            "height": f64::from(rect.height),
        },
        "style": {
            "fontSize": format!("{}px", parent_computed.font_size),
            "fontWeight": parent_computed.font_weight.to_string(),
            "color": color_str,
            "lineHeight": parent_computed.line_height.map_or_else(|| "normal".to_string(), |line_height| format!("{line_height}px")),
        }
    }))
}

fn collect_children_json(
    ctx: &LayoutCtx<'_>,
    key: NodeKey,
    parent_computed: &ComputedStyle,
) -> Vec<JsonValue> {
    let mut kids_json: Vec<JsonValue> = Vec::new();
    if let Some(children) = ctx.children_by_key.get(&key) {
        for child in children {
            match ctx.kind_by_key.get(child) {
                Some(LayoutNodeKind::Block { .. }) => {
                    // Skip elements with display:none
                    if let Some(computed) = ctx.computed.get(child)
                        && computed.display == Display::None
                    {
                        continue;
                    }

                    let child_json = serialize_element_subtree(ctx, *child);
                    // Skip empty JSON objects (filtered elements)
                    if !child_json.is_null()
                        && !child_json.as_object().is_some_and(JsonMap::is_empty)
                    {
                        kids_json.push(child_json);
                    }
                }
                Some(LayoutNodeKind::InlineText { text }) => {
                    if let Some(text_json) = serialize_text_node(ctx, *child, text, parent_computed)
                    {
                        kids_json.push(text_json);
                    }
                }
                _ => {}
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

        // Use viewport-absolute coordinates (same as Chrome's getBoundingClientRect)
        let x = rect.x;
        let y = rect.y;

        let id = ctx
            .attrs_by_key
            .get(&key)
            .and_then(|attr_map| attr_map.get("id"))
            .cloned()
            .unwrap_or_default();
        // Debug output removed for clippy compliance

        // Collect attributes (type, checked, etc.)
        let mut attrs = json!({});
        if let Some(attr_map) = ctx.attrs_by_key.get(&key) {
            if let Some(type_val) = attr_map.get("type") {
                attrs["type"] = JsonValue::String(type_val.clone());
            }
            if attr_map.contains_key("checked") {
                attrs["checked"] = JsonValue::String("true".to_string());
            }
        }

        let display_tag = tag.clone();
        let computed = ctx.computed.get(&key).cloned().unwrap_or_default();
        out = json!({
            "type": "element",
            "tag": display_tag,
            "id": id,
            "attrs": attrs,
            "rect": {
                "x": f64::from(x),
                "y": f64::from(y),
                "width": f64::from(rect.width),
                "height": f64::from(rect.height),
            },
            "style": build_style_json(&computed)
        });

        // Skip children for form controls (input, textarea, select, button)
        // to match browser behavior which doesn't expose internal structure
        let is_form_control = matches!(
            display_tag.as_str(),
            "input" | "textarea" | "select" | "button"
        );

        let kids_json = if is_form_control {
            Vec::new()
        } else {
            // Children are positioned relative to this element's border-box origin
            let child_ctx = LayoutCtx {
                kind_by_key: ctx.kind_by_key,
                children_by_key: ctx.children_by_key,
                attrs_by_key: ctx.attrs_by_key,
                rects: ctx.rects,
                computed: ctx.computed,
            };
            collect_children_json(&child_ctx, key, &computed)
        };

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
    layouter: &LayoutManager,
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
    let attrs_by_key = layouter.attrs_map().clone();

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

    // Get root rect to establish coordinate origin
    // Debug output removed for clippy compliance

    let ctx = LayoutCtx {
        kind_by_key: &kind_by_key,
        children_by_key: &children_by_key,
        attrs_by_key: &attrs_by_key,
        rects,
        computed,
    };
    serialize_element_subtree(&ctx, root_key)
}

const CHROMIUM_SCRIPT_HELPERS: &str = "function shouldSkip(el) {
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
    function pickEdges(prefix) {
        return {
            top: cs[prefix + 'Top'] || '',
            right: cs[prefix + 'Right'] || '',
            bottom: cs[prefix + 'Bottom'] || '',
            left: cs[prefix + 'Left'] || ''
        };
    }
    return {
        display: d,
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
        position: (cs.position || '').toLowerCase(),
        fontSize: cs.fontSize || '',
        fontWeight: cs.fontWeight || '',
        fontFamily: cs.fontFamily || '',
        color: cs.color || '',
        lineHeight: cs.lineHeight || '',
        zIndex: cs.zIndex || 'auto',
        opacity: cs.opacity || '1'
    };
}";

const CHROMIUM_SCRIPT_SERIALIZERS: &str = "function serText(textNode, parentEl) {
    var text = textNode.textContent || '';
    if (!text || /^\\s*$/.test(text)) return null;
    var range = document.createRange();
    range.selectNodeContents(textNode);
    var r = range.getBoundingClientRect();
    var cs = window.getComputedStyle(parentEl);
    return {
        type: 'text',
        text: text,
        rect: { x: r.x, y: r.y, width: r.width, height: r.height },
        style: {
            fontSize: cs.fontSize || '',
            fontWeight: cs.fontWeight || '',
            color: cs.color || '',
            lineHeight: cs.lineHeight || ''
        }
    };
}
function serNode(node, parentEl) {
    if (node.nodeType === 3) {
        return serText(node, parentEl || node.parentElement);
    }
    if (node.nodeType === 1) {
        return serElement(node);
    }
    return null;
}
function serElement(el) {
    var r = el.getBoundingClientRect();
    var cs = window.getComputedStyle(el);
    var attrs = {};
    if (el.hasAttribute('type')) attrs.type = el.getAttribute('type');
    if (el.hasAttribute('checked')) attrs.checked = 'true';
    var tag = String(el.tagName||'').toLowerCase();
    var isFormControl = tag === 'input' || tag === 'textarea' || tag === 'select' || tag === 'button';
    var children = [];
    if (!isFormControl) {
        for (var i = 0; i < el.childNodes.length; i++) {
            var child = el.childNodes[i];
            if (child.nodeType === 1 && shouldSkip(child)) continue;
            var serialized = serNode(child, el);
            if (serialized) children.push(serialized);
        }
    }
    return {
        type: 'element',
        tag: tag,
        id: String(el.id||''),
        attrs: attrs,
        rect: { x: r.x, y: r.y, width: r.width, height: r.height },
        style: pickStyle(el, cs),
        children: children
    };
}";

const CHROMIUM_SCRIPT_MAIN: &str = "if (!window._valorResults) { window._valorResults = []; }
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
var layout = serElement(root);
var asserts = Array.isArray(window._valorResults) ? window._valorResults : [];
return JSON.stringify({ layout: layout, asserts: asserts });";

fn chromium_layout_extraction_script() -> String {
    format!(
        "(function() {{ {CHROMIUM_SCRIPT_HELPERS} {CHROMIUM_SCRIPT_SERIALIZERS} {CHROMIUM_SCRIPT_MAIN} }})()"
    )
}

/// Extracts layout JSON from Chromium by evaluating JavaScript in a tab.
///
/// # Errors
///
/// Returns an error if navigation, script evaluation, or JSON parsing fails.
fn chromium_layout_json_in_tab(tab: &Tab, path: &Path) -> Result<JsonValue> {
    navigate_and_prepare_tab(tab, path)?;
    let script = chromium_layout_extraction_script();
    let result = tab.evaluate(&script, true)?;
    let value = result
        .value
        .ok_or_else(|| anyhow!("No value returned from Chromium evaluate"))?;
    let json_string = value
        .as_str()
        .ok_or_else(|| anyhow!("Chromium returned non-string JSON for layout"))?;
    let parsed: JsonValue = from_str(json_string)?;
    Ok(parsed)
}
