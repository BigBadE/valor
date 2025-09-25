use anyhow::Error;
use css::style_types::{AlignItems, BoxSizing, ComputedStyle, Display, Overflow};
use css_core::LayoutRect;
use css_core::{LayoutNodeKind, Layouter};
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

use crate::test_support as common;

fn apply_basic_attrs(lay: &mut Layouter, node: NodeKey, map: &HashMap<String, String>) {
    for key_name in ["id", "class", "style"] {
        if let Some(val) = map.get(key_name) {
            let _ = lay.apply_update(SetAttr {
                node,
                name: key_name.to_owned(),
                value: val.clone(),
            });
        }
    }
}

pub fn run(filter: Option<String>) -> Result<usize, Error> {
    let _ = env_logger::Builder::from_env(env_logger::Env::default().filter_or("RUST_LOG", "warn"))
        .is_test(false)
        .try_init();

    let launch_opts = LaunchOptionsBuilder::default()
        .headless(true)
        .window_size(Some((800, 600)))
        .idle_browser_timeout(std::time::Duration::from_secs(300))
        .args(vec![
            OsStr::new("--force-device-scale-factor=1"),
            OsStr::new("--disable-features=OverlayScrollbar"),
            OsStr::new("--allow-file-access-from-files"),
            OsStr::new("--disable-gpu"),
            OsStr::new("--disable-dev-shm-usage"),
            OsStr::new("--no-sandbox"),
            OsStr::new("--disable-extensions"),
            OsStr::new("--disable-background-networking"),
            OsStr::new("--disable-sync"),
            OsStr::new("--hide-scrollbars"),
            OsStr::new("--blink-settings=imagesEnabled=false"),
        ])
        .build()
        .expect("Failed to build LaunchOptions for headless_chrome");
    let browser = Browser::new(launch_opts).expect("Failed to launch headless Chrome browser");
    let tab = browser.new_tab()?;

    let mut failed: Vec<(String, String)> = Vec::new();
    let rt = Runtime::new()?;
    let all = common::fixture_html_files()?;

    if let Some(ref f) = filter {
        info!("[LAYOUT] focusing fixtures containing (CLI/bin): {f}");
    }
    info!("[LAYOUT] discovered {} fixtures", all.len());

    let mut ran = 0usize;
    for input_path in all {
        if let Some(ref f) = filter {
            let display_name = input_path.display().to_string();
            if !display_name.contains(f) {
                continue;
            }
        }
        let display_name = input_path.display().to_string();
        let url = common::to_file_url(&input_path)?;
        let mut page = common::create_page(&rt, url)?;
        page.eval_js(common::css_reset_injection_script())?;
        let mut layouter_mirror = page.create_mirror(Layouter::new());

        let finished = common::update_until_finished(&rt, &mut page, |_| {
            layouter_mirror.try_update_sync()?;
            Ok(())
        })?;
        if !finished {
            let msg = "Parsing did not finish".to_string();
            eprintln!("[LAYOUT] {display_name} ... FAILED: {msg}");
            failed.push((display_name.clone(), msg));
            continue;
        }
        let _ = rt.block_on(page.update());
        layouter_mirror.try_update_sync()?;

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
                    apply_basic_attrs(lay, *child, map);
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

        let computed = page.computed_styles_snapshot()?;
        let computed_for_serialization = computed.clone();
        let layouter = layouter_mirror.mirror_mut();
        let sheet_for_layout = page.styles_snapshot()?;
        layouter.set_stylesheet(sheet_for_layout);
        layouter.set_computed_styles(computed);
        let _count = layouter.compute_layout();
        let updates_applied = layouter.perf_updates_applied();
        let snap_tmp = layouter.snapshot();
        let blocks_count = snap_tmp
            .iter()
            .filter(|(_, kind, _)| matches!(kind, LayoutNodeKind::Block { .. }))
            .count();
        debug!(
            "[LAYOUT][DIAG] external layouter after replay: updates_applied={updates_applied}, blocks_in_snapshot={blocks_count}"
        );
        let rects_external = layouter.compute_layout_geometry();

        let our_json = our_layout_json(layouter, &rects_external, &computed_for_serialization);
        let harness_src = include_str!("../tests/layouter_chromium_compare.rs");
        let ch_json =
            if let Some(v) = common::read_cached_json_for_fixture(&input_path, harness_src) {
                v
            } else {
                let value = chromium_layout_json_in_tab(&tab, &input_path)?;
                common::write_cached_json_for_fixture(&input_path, harness_src, &value)?;
                value
            };
        common::write_named_json_for_fixture(&input_path, harness_src, "chromium", &ch_json)?;
        common::write_named_json_for_fixture(&input_path, harness_src, "valor", &our_json)?;

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
        let eps = f32::EPSILON as f64 * 3.0;
        match common::compare_json_with_epsilon(&our_json, &ch_layout_json, eps) {
            Ok(_) => {
                info!("[LAYOUT] {display_name} ... ok");
                ran += 1;
            }
            Err(msg) => {
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
                    if let Some((_, _, kids)) = snapshot_diag.iter().find(|(k, _, _)| *k == elem) {
                        child_count = kids.len();
                    }
                    debug!(
                        "[LAYOUT][DIAG] first element child under ROOT has {child_count} children"
                    );
                }
                error!("[LAYOUT] {display_name} ... FAILED: {msg}");
                failed.push((display_name.clone(), msg));
            }
        }
    }
    if !failed.is_empty() {
        error!("==== LAYOUT FAILURES ({} total) ====", failed.len());
        for (name, msg) in &failed {
            error!("- {name}\n  {msg}\n");
        }
        return Err(anyhow::anyhow!(
            "{} layout fixture(s) failed; see log above.",
            failed.len()
        ));
    }
    info!("[LAYOUT] {ran} fixtures passed");
    Ok(ran)
}

fn our_layout_json(
    layouter: &Layouter,
    rects: &HashMap<NodeKey, LayoutRect>,
    computed: &HashMap<NodeKey, ComputedStyle>,
) -> Value {
    let snapshot = layouter.snapshot();
    let mut kind_by_key = HashMap::new();
    let mut children_by_key = HashMap::new();
    for (k, kind, children) in snapshot.into_iter() {
        kind_by_key.insert(k, kind);
        children_by_key.insert(k, children);
    }
    let attrs_by_key = layouter.attrs_map();
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
    let ctx = LayoutCtx {
        kind_by_key: &kind_by_key,
        children_by_key: &children_by_key,
        attrs_by_key: &attrs_by_key,
        rects,
        computed,
    };
    serialize_element_subtree(&ctx, root_key)
}

struct LayoutCtx<'a> {
    kind_by_key: &'a HashMap<NodeKey, LayoutNodeKind>,
    children_by_key: &'a HashMap<NodeKey, Vec<NodeKey>>,
    attrs_by_key: &'a HashMap<NodeKey, HashMap<String, String>>,
    rects: &'a HashMap<NodeKey, LayoutRect>,
    computed: &'a HashMap<NodeKey, ComputedStyle>,
}

fn serialize_element_subtree(ctx: &LayoutCtx<'_>, key: NodeKey) -> Value {
    fn is_non_rendering_tag(tag: &str) -> bool {
        matches!(
            tag,
            "head" | "meta" | "title" | "link" | "style" | "script" | "base"
        )
    }
    // Flex basis formatting not needed for core compare; use a placeholder for now.
    fn flex_basis_str() -> &'static str {
        "auto"
    }
    fn effective_display(d: &Display) -> &'static str {
        match d {
            Display::Inline => "inline",
            Display::Block => "block",
            Display::Flex => "flex",
            Display::InlineFlex => "inline-flex",
            Display::None => "none",
            Display::Contents => "block",
        }
    }
    let mut out = json!({});
    if let Some(LayoutNodeKind::Block { tag }) = ctx.kind_by_key.get(&key) {
        if is_non_rendering_tag(tag) {
            return json!({});
        }
        let rect = ctx.rects.get(&key).cloned().unwrap_or_default();
        let display_tag = tag.to_string();
        let id = ctx
            .attrs_by_key
            .get(&key)
            .and_then(|m| m.get("id"))
            .cloned()
            .unwrap_or_default();
        let computed = ctx.computed.get(&key).cloned().unwrap_or_default();
        out = json!({
            "tag": display_tag,
            "id": id,
            "rect": {
                "x": rect.x as f64,
                "y": rect.y as f64,
                "width": rect.width as f64,
                "height": rect.height as f64,
            },
            "style": {
                "display": effective_display(&computed.display),
                "boxSizing": match computed.box_sizing { BoxSizing::BorderBox => "border-box", BoxSizing::ContentBox => "content-box" },
                "flexBasis": flex_basis_str(),
                "flexGrow": computed.flex_grow as f64,
                "flexShrink": computed.flex_shrink as f64,
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
            }
        });
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
        if let Some(obj) = out.as_object_mut() {
            obj.insert("children".to_string(), Value::Array(kids_json));
        }
    }
    out
}

fn chromium_layout_json_in_tab(tab: &headless_chrome::Tab, path: &Path) -> anyhow::Result<Value> {
    let url = url::Url::from_file_path(path.canonicalize().unwrap_or_else(|_| path.to_path_buf()))
        .map_err(|_| anyhow::anyhow!("Invalid fixture path for Chrome: {}", path.display()))?;
    tab.navigate_to(url.as_str())?;
    tab.wait_until_navigated()?;
    // Ensure consistent baseline styles inside Chrome like the test harness
    let _ = tab.evaluate(crate::test_support::css_reset_injection_script(), false);
    let script = r#"
      (function(){
        try {
          var out = { layout: {}, asserts: window._valorResults || [] };
          var root = document.body || document.documentElement;
          function rectOf(el){ var r = el.getBoundingClientRect(); return { x: Math.round(r.left), y: Math.round(r.top), width: Math.round(r.width), height: Math.round(r.height) }; }
          function styleOf(el){
            var cs = getComputedStyle(el);
            function px(n){ return String(n|0) + 'px'; }
            return {
              display: cs.display === 'contents' ? 'block' : cs.display,
              boxSizing: cs.boxSizing,
              flexBasis: cs.flexBasis || 'auto',
              flexGrow: +(cs.flexGrow||0),
              flexShrink: +(cs.flexShrink||0),
              alignItems: cs.alignItems === 'normal' ? 'normal' : cs.alignItems,
              overflow: cs.overflowX === cs.overflowY ? cs.overflowX : cs.overflowX,
              margin: { top: cs.marginTop, right: cs.marginRight, bottom: cs.marginBottom, left: cs.marginLeft },
              padding: { top: cs.paddingTop, right: cs.paddingRight, bottom: cs.paddingBottom, left: cs.paddingLeft },
              borderWidth: { top: cs.borderTopWidth, right: cs.borderRightWidth, bottom: cs.borderBottomWidth, left: cs.borderLeftWidth },
            };
          }
          function serialize(el){
            var obj = { tag: el.tagName.toLowerCase(), id: el.id || '', rect: rectOf(el), style: styleOf(el), children: [] };
            for (var i=0;i<el.children.length;i++){
              var kid = el.children[i];
              var disp = getComputedStyle(kid).display;
              if (disp === 'none') continue;
              obj.children.push(serialize(kid));
            }
            return obj;
          }
          out.layout = serialize(root);
          if (typeof window._valorRun === 'function') { try { window._valorRun(); } catch(e){} }
          return out;
        } catch (e) {
          return { error: String(e && (e.stack || e.message) || e) };
        }
      })();
    "#;
    let result = tab.evaluate(script, true)?;
    match result.value {
        Some(value) => Ok(value),
        None => Err(anyhow::anyhow!(
            "Chromium evaluation returned no value (null) for {}",
            url
        )),
    }
}
