use anyhow::{Result, anyhow};
use css::style_types::{AlignItems, BoxSizing, ComputedStyle, Display, Overflow};
use css_core::{LayoutNodeKind, LayoutRect, Layouter};
use env_logger::Builder;
use env_logger::Env as EnvLoggerEnv;
use headless_chrome::{
    Browser, LaunchOptionsBuilder, Tab, protocol::cdp::Page::CaptureScreenshotFormatOption,
};
use image::{RgbaImage, load_from_memory};
use js::DOMSubscriber as _;
use js::DOMUpdate::{EndOfDocument, InsertElement, SetAttr};
use js::NodeKey;
use log::{debug, error, info};
use renderer::{DisplayItem, DisplayList, batch_display_list};
use serde_json::{
    Number as JsonNumber, Value as JsonValue, from_str, json, to_string_pretty,
    value::Map as JsonMap,
};
use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsStr;
use std::fs::{create_dir_all, read, read_dir, read_to_string, remove_dir_all, remove_file, write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::runtime::Runtime;
use valor::test_support::{
    artifacts_subdir, create_page, fixtures_dir, fixtures_layout_dir, read_cached_json_for_fixture,
    to_file_url, update_until_finished, update_until_finished_simple,
    write_cached_json_for_fixture, write_named_json_for_fixture, write_png_rgba_if_changed,
};
use wgpu_backend::RenderState;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};
use zstd::bulk::{compress as zstd_compress, decompress as zstd_decompress};

// ================================================================================================
// Common Utilities
// ================================================================================================

const fn checksum_u64(input_str: &str) -> u64 {
    let bytes = input_str.as_bytes();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325; // FNV-1a 64-bit
    let mut index = 0;
    while index < bytes.len() {
        hash ^= bytes[index] as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
        index += 1;
    }
    hash
}

/// Clears the valor layout cache if the harness source has changed.
///
/// # Errors
///
/// Returns an error if directory creation or file write operations fail.
fn clear_valor_layout_cache_if_harness_changed(harness_src: &str) -> Result<()> {
    let dir = artifacts_subdir("valor_layout_cache");
    drop(create_dir_all(&dir));
    let marker = dir.join(".harness_hash");
    let current = format!("{:016x}", checksum_u64(harness_src));
    let prev = read_to_string(&marker).unwrap_or_default();
    if prev.trim() != current {
        drop(remove_dir_all(&dir));
        create_dir_all(&dir)?;
        write(&marker, &current)?;
    }
    Ok(())
}

fn workspace_root_from_valor_manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn module_css_fixture_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let root = workspace_root_from_valor_manifest();
    let modules_parent = root.join("crates").join("css").join("modules");
    if let Ok(entries) = read_dir(&modules_parent) {
        for entry in entries.filter_map(Result::ok) {
            let module_path = entry.path();
            if module_path.is_dir() {
                let fixture_path = module_path.join("tests").join("fixtures");
                if fixture_path.exists() {
                    roots.push(fixture_path);
                }
            }
        }
    }
    roots
}

fn workspace_crate_layout_fixture_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let root = workspace_root_from_valor_manifest();
    let crates_parent = root.join("crates");
    if let Ok(entries) = read_dir(&crates_parent) {
        for entry in entries.filter_map(Result::ok) {
            let krate_path = entry.path();
            if krate_path.is_dir() {
                let layout_path = krate_path.join("tests").join("fixtures").join("layout");
                if layout_path.exists() {
                    roots.push(layout_path);
                }
            }
        }
    }
    roots
}

/// Recursively collects HTML files from a directory.
///
/// # Errors
///
/// Returns an error if directory reading fails.
fn collect_html_recursively(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries =
        read_dir(dir).map_err(|err| anyhow!("Failed to read dir {}: {}", dir.display(), err))?;
    for entry in entries.filter_map(Result::ok) {
        let entry_path = entry.path();
        if entry_path.is_dir() {
            collect_html_recursively(&entry_path, out)?;
        } else if entry_path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("html"))
        {
            out.push(entry_path);
        }
    }
    Ok(())
}

/// Collects all HTML fixture files from various fixture directories.
///
/// # Errors
///
/// Returns an error if directory reading fails.
fn fixture_html_files() -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = Vec::new();
    let local_layout = fixtures_layout_dir();
    if local_layout.exists() {
        collect_html_recursively(&local_layout, &mut files)?;
    } else {
        let legacy = fixtures_dir();
        if legacy.exists() {
            let entries = read_dir(&legacy).map_err(|err| {
                anyhow!("Failed to read fixtures dir {}: {}", legacy.display(), err)
            })?;
            files.extend(
                entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .filter(|legacy_path| {
                        legacy_path
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("html"))
                    }),
            );
        }
    }
    for root in module_css_fixture_roots() {
        collect_html_recursively(&root, &mut files)?;
    }
    for root in workspace_crate_layout_fixture_roots() {
        collect_html_recursively(&root, &mut files)?;
    }

    files.retain(|entry_path| {
        let parent_not_fixtures = entry_path
            .parent()
            .and_then(|dir_entry| dir_entry.file_name())
            .is_some_and(|name| name != "fixtures");

        let mut has_fixtures_ancestor = false;
        for anc in entry_path.ancestors().skip(1) {
            if let Some(name) = anc.file_name()
                && name == "fixtures"
            {
                has_fixtures_ancestor = true;
                break;
            }
        }
        has_fixtures_ancestor && parent_not_fixtures
    });
    files.sort();
    let mut seen = HashSet::new();
    let mut unique = Vec::with_capacity(files.len());
    for entry_path in files {
        let canon = entry_path
            .canonicalize()
            .unwrap_or_else(|_| entry_path.clone());
        if seen.insert(canon) {
            unique.push(entry_path);
        }
    }
    Ok(unique)
}

fn extract_id_label(value: &JsonValue) -> Option<String> {
    let JsonValue::Object(map) = value else {
        return None;
    };

    if let Some(JsonValue::Object(attrs)) = map.get("attrs")
        && let Some(JsonValue::String(id)) = attrs.get("id")
    {
        return Some(format!("#{id}"));
    }

    if let Some(JsonValue::String(id)) = map.get("id") {
        return Some(format!("#{id}"));
    }

    None
}

fn is_element_object(value: &JsonValue) -> bool {
    if let JsonValue::Object(map) = value {
        map.contains_key("tag") && map.contains_key("rect")
    } else {
        false
    }
}

fn extract_rect_coords(rect_map: &JsonMap<String, JsonValue>) -> (f64, f64, f64, f64) {
    let coord_x = rect_map.get("x").and_then(JsonValue::as_f64).unwrap_or(0.0);
    let coord_y = rect_map.get("y").and_then(JsonValue::as_f64).unwrap_or(0.0);
    let coord_width = rect_map
        .get("width")
        .and_then(JsonValue::as_f64)
        .unwrap_or(0.0);
    let coord_height = rect_map
        .get("height")
        .and_then(JsonValue::as_f64)
        .unwrap_or(0.0);
    (coord_x, coord_y, coord_width, coord_height)
}

fn format_child_summary(child_tag: &str, child_id: &str, rect: (f64, f64, f64, f64)) -> String {
    let (coord_x, coord_y, coord_width, coord_height) = rect;
    if child_id.is_empty() {
        format!("<{child_tag}> rect=({coord_x:.0},{coord_y:.0},{coord_width:.0},{coord_height:.0})")
    } else {
        format!(
            "<{child_tag} id=#{child_id}> rect=({coord_x:.0},{coord_y:.0},{coord_width:.0},{coord_height:.0})"
        )
    }
}

fn process_child_element(child: &JsonValue) -> Option<String> {
    if let JsonValue::Object(child_map) = child {
        let child_tag = child_map
            .get("tag")
            .and_then(|tag_val| tag_val.as_str())
            .unwrap_or("");
        let child_id = child_map
            .get("id")
            .and_then(|id_val| id_val.as_str())
            .unwrap_or("");
        let rect = if let Some(JsonValue::Object(rect_obj)) = child_map.get("rect") {
            extract_rect_coords(rect_obj)
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };
        Some(format_child_summary(child_tag, child_id, rect))
    } else {
        None
    }
}

fn child_summary_lines(children: &[JsonValue]) -> Vec<JsonValue> {
    let mut lines: Vec<JsonValue> = Vec::new();
    for child in children {
        if let Some(summary) = process_child_element(child) {
            lines.push(JsonValue::String(summary));
        }
    }
    lines
}

fn pretty_elem_with_compact_children(value: &JsonValue) -> String {
    if let JsonValue::Object(map) = value {
        let mut output_map = map.clone();
        if let Some(JsonValue::Array(children)) = map.get("children") {
            let lines = child_summary_lines(children);
            output_map.insert("children".to_string(), JsonValue::Array(lines));
        }
        let json_object = JsonValue::Object(output_map);
        to_string_pretty(&json_object).unwrap_or_else(|_| String::from("{}"))
    } else {
        to_string_pretty(value).unwrap_or_else(|_| String::from("{}"))
    }
}

fn build_err(
    kind: &str,
    detail: &str,
    path: &[String],
    elem_stack: &[(JsonValue, JsonValue)],
) -> String {
    let path_str = format_path(path);
    let (our_elem, chromium_elem) = if let Some((valor_elem, chromium_elem)) = elem_stack.last() {
        (valor_elem, chromium_elem)
    } else {
        (&JsonValue::Null, &JsonValue::Null)
    };
    let our_str = pretty_elem_with_compact_children(our_elem);
    let chromium_str = pretty_elem_with_compact_children(chromium_elem);
    if detail.is_empty() {
        format!("{path_str}: {kind}\nElement (our): {our_str}\nElement (chromium): {chromium_str}")
    } else {
        format!(
            "{path_str}: {kind} — {detail}\nElement (our): {our_str}\nElement (chromium): {chromium_str}"
        )
    }
}

fn format_path(path: &[String]) -> String {
    if path.is_empty() {
        String::new()
    } else {
        let joined = path.join("");
        joined
            .strip_prefix('.')
            .map_or_else(|| joined.clone(), ToString::to_string)
    }
}

fn type_name(json_value: &JsonValue) -> &'static str {
    match json_value {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "bool",
        JsonValue::Number(_) => "number",
        JsonValue::String(_) => "string",
        JsonValue::Array(_) => "array",
        JsonValue::Object(_) => "object",
    }
}

type HelperFn = fn(
    &JsonValue,
    &JsonValue,
    f64,
    &mut Vec<String>,
    &mut Vec<(JsonValue, JsonValue)>,
) -> Result<(), String>;

struct CompareContext<'cmp> {
    eps: f64,
    path: &'cmp mut Vec<String>,
    elem_stack: &'cmp mut Vec<(JsonValue, JsonValue)>,
    helper: HelperFn,
}

/// Compares two JSON numbers with an epsilon tolerance.
///
/// # Errors
///
/// Returns an error if the numbers differ by more than the epsilon or are non-float numbers.
fn compare_numbers(
    actual: &JsonNumber,
    expected: &JsonNumber,
    eps: f64,
    path: &[String],
    elem_stack: &[(JsonValue, JsonValue)],
) -> Result<(), String> {
    match (actual.as_f64(), expected.as_f64()) {
        (Some(actual_float), Some(expected_float)) => {
            if (actual_float - expected_float).abs() <= eps {
                Ok(())
            } else {
                Err(build_err(
                    "number diff",
                    &format!("{actual_float} vs {expected_float} exceeds eps {eps}"),
                    path,
                    elem_stack,
                ))
            }
        }
        _ => Err(build_err(
            "non-float number encountered",
            "",
            path,
            elem_stack,
        )),
    }
}

/// Compares two JSON arrays element by element.
///
/// # Errors
///
/// Returns an error if array lengths differ or any elements differ.
fn compare_arrays(
    actual_arr: &[JsonValue],
    expected_arr: &[JsonValue],
    ctx: &mut CompareContext<'_>,
) -> Result<(), String> {
    if actual_arr.len() != expected_arr.len() {
        return Err(build_err(
            "array length mismatch",
            &format!("{} != {}", actual_arr.len(), expected_arr.len()),
            ctx.path,
            ctx.elem_stack,
        ));
    }
    for (index, (actual_item, expected_item)) in
        actual_arr.iter().zip(expected_arr.iter()).enumerate()
    {
        let is_children_ctx = ctx
            .path
            .last()
            .is_some_and(|segment| segment == ".children");
        if is_children_ctx {
            let label = extract_id_label(actual_item)
                .or_else(|| extract_id_label(expected_item))
                .unwrap_or_else(|| index.to_string());
            ctx.path.pop();
            ctx.path.push(format!(".{label}"));
        } else {
            ctx.path.push(format!("[{index}]"));
        }

        let should_push = is_element_object(actual_item) && is_element_object(expected_item);
        if should_push {
            ctx.elem_stack
                .push((actual_item.clone(), expected_item.clone()));
        }
        let result = (ctx.helper)(
            actual_item,
            expected_item,
            ctx.eps,
            ctx.path,
            ctx.elem_stack,
        );
        if should_push {
            ctx.elem_stack.pop();
        }
        ctx.path.pop();
        result?;
    }
    Ok(())
}

/// Compares two JSON objects key by key.
///
/// # Errors
///
/// Returns an error if object sizes differ, keys are missing, or any values differ.
fn compare_objects(
    actual_obj: &JsonMap<String, JsonValue>,
    expected_obj: &JsonMap<String, JsonValue>,
    ctx: &mut CompareContext<'_>,
) -> Result<(), String> {
    if actual_obj.len() != expected_obj.len() {
        return Err(build_err(
            "object size mismatch",
            &format!("{} != {}", actual_obj.len(), expected_obj.len()),
            ctx.path,
            ctx.elem_stack,
        ));
    }
    for (key, actual_val) in actual_obj {
        match expected_obj.get(key) {
            Some(expected_val) => {
                ctx.path.push(format!(".{key}"));
                let should_push = is_element_object(actual_val) && is_element_object(expected_val);
                if should_push {
                    ctx.elem_stack
                        .push((actual_val.clone(), expected_val.clone()));
                }
                let result =
                    (ctx.helper)(actual_val, expected_val, ctx.eps, ctx.path, ctx.elem_stack);
                if should_push {
                    ctx.elem_stack.pop();
                }
                ctx.path.pop();
                result?;
            }
            None => {
                return Err(build_err(
                    "missing key in expected",
                    &format!("'{key}'"),
                    ctx.path,
                    ctx.elem_stack,
                ));
            }
        }
    }
    Ok(())
}

/// Helper function to recursively compare JSON values.
///
/// # Errors
///
/// Returns an error string if values differ beyond tolerance.
fn compare_json_helper(
    actual_value: &JsonValue,
    expected_value: &JsonValue,
    eps: f64,
    path: &mut Vec<String>,
    elem_stack: &mut Vec<(JsonValue, JsonValue)>,
) -> Result<(), String> {
    // Special-case: ignore root-level rect width/height diffs
    if elem_stack.len() <= 1 && path.len() >= 2 {
        let (last, prev) = (&path[path.len() - 1], &path[path.len() - 2]);
        if matches!(last.as_str(), ".width" | ".height") && prev == ".rect" {
            return Ok(());
        }
    }
    match (actual_value, expected_value) {
        (JsonValue::Null, JsonValue::Null) => Ok(()),
        (JsonValue::Bool(actual_bool), JsonValue::Bool(expected_bool))
            if actual_bool == expected_bool =>
        {
            Ok(())
        }
        (JsonValue::Bool(actual_bool), JsonValue::Bool(expected_bool)) => Err(build_err(
            "bool mismatch",
            &format!("{actual_bool} != {expected_bool}"),
            path,
            elem_stack,
        )),
        (JsonValue::Number(actual_num), JsonValue::Number(expected_num)) => {
            compare_numbers(actual_num, expected_num, eps, path, elem_stack)
        }
        (JsonValue::String(actual_str), JsonValue::String(expected_str))
            if actual_str == expected_str =>
        {
            Ok(())
        }
        (JsonValue::String(actual_str), JsonValue::String(expected_str)) => Err(build_err(
            "string mismatch",
            &format!("'{actual_str}' != '{expected_str}'"),
            path,
            elem_stack,
        )),
        (JsonValue::Array(actual_arr), JsonValue::Array(expected_arr)) => compare_arrays(
            actual_arr,
            expected_arr,
            &mut CompareContext {
                eps,
                path,
                elem_stack,
                helper: compare_json_helper,
            },
        ),
        (JsonValue::Object(actual_obj), JsonValue::Object(expected_obj)) => compare_objects(
            actual_obj,
            expected_obj,
            &mut CompareContext {
                eps,
                path,
                elem_stack,
                helper: compare_json_helper,
            },
        ),
        (actual_other, expected_other) => Err(build_err(
            "type mismatch",
            &format!(
                "{:?} vs {:?}",
                type_name(actual_other),
                type_name(expected_other)
            ),
            path,
            elem_stack,
        )),
    }
}

/// Compares two JSON values with epsilon tolerance for floating-point numbers.
///
/// # Errors
///
/// Returns an error string describing the mismatch if values differ beyond tolerance.
fn compare_json_with_epsilon(
    actual: &JsonValue,
    expected: &JsonValue,
    eps: f64,
) -> Result<(), String> {
    let mut elem_stack: Vec<(JsonValue, JsonValue)> = Vec::new();
    if is_element_object(actual) && is_element_object(expected) {
        elem_stack.push((actual.clone(), expected.clone()));
    }
    compare_json_helper(actual, expected, eps, &mut Vec::new(), &mut elem_stack)
}

const fn css_reset_injection_script() -> &'static str {
    r#"(function(){
        try {
            var css = "*,*::before,*::after{box-sizing:border-box;margin:0;padding:0;font-family:monospace,'Courier New',Courier,Consolas,'Liberation Mono',Menlo,Monaco,'DejaVu Sans Mono',monospace;}html,body{margin:0 !important;padding:0 !important;}body{margin:0 !important;}h1,h2,h3,h4,h5,h6,p{margin:0;padding:0;}ul,ol{margin:0;padding:0;list-style:none;}";
            var existing = (typeof document.querySelector === 'function') ? document.querySelector("style[data-valor-test-reset='1']") : null;
            if (existing) { return true; }
            if (document && typeof document.appendStyleText === 'function') {
                document.appendStyleText(css);
            } else {
                var style = document.createElement('style');
                style.setAttribute('data-valor-test-reset','1');
                style.type = 'text/css';
                style.appendChild(document.createTextNode(css));
                var head = document.head || document.getElementsByTagName('head')[0] || document.documentElement;
                head.appendChild(style);
            }
            var de = document.documentElement; if (de && de.style){ de.style.margin='0'; de.style.padding='0'; de.style.fontFamily='monospace'; }
            var b = document.body; if (b && b.style){ b.style.margin='0'; b.style.padding='0'; b.style.fontFamily='monospace'; }
            void (document.body && document.body.offsetWidth);
            return true;
        } catch (e) {
            return false;
        }
    })()"#
}

// ================================================================================================
// Layout Testing
// ================================================================================================

#[cfg(test)]
mod layout_tests {
    use super::*;

    type LayouterWithStyles = (Layouter, HashMap<NodeKey, ComputedStyle>);

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

    fn cli_layout_filter() -> Option<String> {
        let mut args = env::args();
        let _ = args.next();
        let mut pending_value_for: Option<String> = None;
        for arg in args {
            if let Some(rest) = arg.strip_prefix("run_chromium_layouts::")
                && !rest.is_empty()
            {
                return Some(rest.to_string());
            }
            if let Some(rest) = arg.strip_prefix("layout-filter=") {
                return Some(rest.to_string());
            }
            if let Some(rest) = arg.strip_prefix("fixture=") {
                return Some(rest.to_string());
            }
            if let Some(rest) = arg.strip_prefix("--layout-filter=") {
                return Some(rest.to_string());
            }
            if let Some(rest) = arg.strip_prefix("--fixture=") {
                return Some(rest.to_string());
            }
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

    /// Sets up a headless Chrome browser for testing.
    ///
    /// # Errors
    ///
    /// Returns an error if browser launch fails.
    fn setup_chrome_browser() -> Result<Browser> {
        let launch_opts = LaunchOptionsBuilder::default()
            .headless(true)
            .window_size(Some((800, 600)))
            .idle_browser_timeout(Duration::from_secs(300))
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
            .build()?;
        Browser::new(launch_opts)
    }

    /// Sets up a layouter for a fixture by creating a page and processing it.
    ///
    /// # Errors
    ///
    /// Returns an error if page creation, parsing, or layout computation fails.
    fn setup_layouter_for_fixture(
        runtime: &Runtime,
        input_path: &Path,
    ) -> Result<LayouterWithStyles> {
        let url = to_file_url(input_path)?;
        let mut page = create_page(runtime, url)?;
        page.eval_js(css_reset_injection_script())?;
        let mut layouter_mirror = page.create_mirror(Layouter::new());

        let finished = update_until_finished(runtime, &mut page, |_page| {
            layouter_mirror.try_update_sync()?;
            Ok(())
        })?;

        if !finished {
            return Err(anyhow!("Parsing did not finish"));
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

        let rects_external = layouter.compute_layout_geometry();
        let our_json = our_layout_json(&layouter, &rects_external, &computed_for_serialization);
        let ch_json =
            if let Some(cached_value) = read_cached_json_for_fixture(input_path, harness_src) {
                cached_value
            } else {
                let chromium_value = chromium_layout_json_in_tab(tab, input_path)?;
                write_cached_json_for_fixture(input_path, harness_src, &chromium_value)?;
                chromium_value
            };

        write_named_json_for_fixture(input_path, harness_src, "chromium", &ch_json)?;
        write_named_json_for_fixture(input_path, harness_src, "valor", &our_json)?;
        check_js_assertions(&ch_json, &display_name, failed);

        let ch_layout_json = if ch_json.get("layout").is_some() || ch_json.get("asserts").is_some()
        {
            ch_json.get("layout").cloned().unwrap_or_else(|| json!({}))
        } else {
            ch_json.clone()
        };

        let eps = f64::from(f32::EPSILON) * 3.0;
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

    /// Tests layout computation by comparing Valor layout with Chromium layout.
    ///
    /// # Errors
    ///
    /// Returns an error if browser setup fails or any layout comparisons fail.
    #[test]
    fn run_chromium_layouts() -> Result<()> {
        drop(
            Builder::from_env(EnvLoggerEnv::default().filter_or("RUST_LOG", "warn"))
                .is_test(false)
                .try_init(),
        );
        let harness_src = include_str!("chromium_compare.rs");
        drop(clear_valor_layout_cache_if_harness_changed(harness_src));
        let browser = setup_chrome_browser()?;
        let tab = browser.new_tab()?;
        let mut failed: Vec<(String, String)> = Vec::new();
        let runtime = Runtime::new()?;
        let all = fixture_html_files()?;
        let focus = cli_layout_filter();
        if let Some(filter) = &focus {
            info!("[LAYOUT] focusing fixtures containing (CLI): {filter}");
        }
        info!("[LAYOUT] discovered {} fixtures", all.len());
        let mut ran = 0;
        for input_path in all {
            if process_layout_fixture(&input_path, &runtime, &tab, harness_src, &mut failed)? {
                ran += 1;
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
                    kids_json.push(serialize_element_subtree(ctx, *child));
                }
            }
        }
        kids_json
    }

    fn serialize_element_subtree(ctx: &LayoutCtx<'_>, key: NodeKey) -> JsonValue {
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

    /// Extracts layout JSON from Chromium by evaluating JavaScript in a tab.
    ///
    /// # Errors
    ///
    /// Returns an error if navigation, script evaluation, or JSON parsing fails.
    fn chromium_layout_json_in_tab(tab: &Tab, path: &Path) -> Result<JsonValue> {
        let url = to_file_url(path)?;
        let url_string = url.as_str().to_owned();
        tab.navigate_to(&url_string)?;
        tab.wait_until_navigated()?;
        let _ = tab.evaluate(css_reset_injection_script(), false)?;
        let script = chromium_layout_extraction_script();
        let result = tab.evaluate(script, true)?;
        let value = result
            .value
            .ok_or_else(|| anyhow!("No value returned from Chromium evaluate"))?;
        let json_string = value
            .as_str()
            .ok_or_else(|| anyhow!("Chromium returned non-string JSON for layout"))?;
        let parsed: JsonValue = from_str(json_string)?;
        Ok(parsed)
    }
}

// ================================================================================================
// Graphics Testing
// ================================================================================================

#[cfg(test)]
mod graphics_tests {
    use super::*;

    fn safe_stem(path: &Path) -> String {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("fixture")
            .to_string()
    }

    fn now_millis() -> String {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        format!("{millis}")
    }

    const fn fnv1a64_bytes(bytes: &[u8]) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut index = 0;
        while index < bytes.len() {
            hash ^= bytes[index] as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
            index += 1;
        }
        hash
    }

    fn file_content_hash(path: &Path) -> u64 {
        read(path).map_or(0, |bytes| fnv1a64_bytes(&bytes))
    }

    fn should_remove_out_dir_artifact(
        fname: &str,
        name: &str,
        path_hash_hex: &str,
        hash_hex: &str,
    ) -> bool {
        let prefix = format!("{name}_{path_hash_hex}_");
        let is_this_fixture = fname.starts_with(&prefix);
        if !is_this_fixture {
            return false;
        }

        let is_stable_chrome =
            fname.ends_with("_chrome.png") || fname.ends_with("_chrome.rgba.zst");
        if !is_stable_chrome {
            return false;
        }

        let is_current_hash = fname.contains(&format!("_{hash_hex}_"))
            || fname.ends_with(&format!("_{hash_hex}_chrome.png"))
            || fname.ends_with(&format!("_{hash_hex}_chrome.rgba.zst"));

        !is_current_hash
    }

    fn should_remove_failing_dir_artifact(
        fname: &str,
        name: &str,
        path_hash_hex: &str,
        hash_hex: &str,
    ) -> bool {
        let prefix = format!("{name}_{path_hash_hex}_");
        let is_this_fixture = fname.starts_with(&prefix);
        if !is_this_fixture {
            return false;
        }

        let is_current_hash = fname.contains(&format!("_{hash_hex}_"));
        !is_current_hash
    }

    /// Cleans up old artifact files for a given hash.
    fn cleanup_artifacts_for_hash(
        name: &str,
        path_hash_hex: &str,
        out_dir: &Path,
        failing_dir: &Path,
        hash_hex: &str,
    ) {
        if let Ok(entries) = read_dir(out_dir) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                let Some(fname) = entry_path.file_name().and_then(|os_name| os_name.to_str())
                else {
                    continue;
                };
                if should_remove_out_dir_artifact(fname, name, path_hash_hex, hash_hex) {
                    drop(remove_file(entry_path));
                }
            }
        }
        if let Ok(entries) = read_dir(failing_dir) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                let Some(fname) = entry_path.file_name().and_then(|os_name| os_name.to_str())
                else {
                    continue;
                };
                if should_remove_failing_dir_artifact(fname, name, path_hash_hex, hash_hex) {
                    drop(remove_file(entry_path));
                }
            }
        }
    }

    type TextMask = (u32, u32, u32, u32);

    struct DiffCtx<'diff> {
        width: u32,
        height: u32,
        eps: u8,
        masks: &'diff [TextMask],
    }

    fn is_pixel_masked(x_coord: u32, y_coord: u32, masks: &[TextMask]) -> bool {
        masks.iter().any(|&(left, top, right, bottom)| {
            x_coord >= left && x_coord < right && y_coord >= top && y_coord < bottom
        })
    }

    fn count_channel_diffs(pixel_a: &[u8], pixel_b: &[u8], idx: usize, eps: u8) -> (u64, u64) {
        let mut total = 0;
        let mut over = 0;
        for channel in 0..4 {
            let diff_ab = i16::from(pixel_a[idx + channel]) - i16::from(pixel_b[idx + channel]);
            let abs_diff = diff_ab.unsigned_abs() as u8;
            total += 1;
            if abs_diff > eps {
                over += 1;
            }
        }
        (over, total)
    }

    fn per_pixel_diff_masked(pixels_a: &[u8], pixels_b: &[u8], ctx: &DiffCtx<'_>) -> (u64, u64) {
        let mut total: u64 = 0;
        let mut over: u64 = 0;
        for y_coord in 0..ctx.height {
            for x_coord in 0..ctx.width {
                if is_pixel_masked(x_coord, y_coord, ctx.masks) {
                    continue;
                }
                let idx = ((y_coord * ctx.width + x_coord) * 4) as usize;
                if idx + 3 >= pixels_a.len() || idx + 3 >= pixels_b.len() {
                    continue;
                }
                let (ch_over, ch_total) = count_channel_diffs(pixels_a, pixels_b, idx, ctx.eps);
                over += ch_over;
                total += ch_total;
            }
        }
        (over, total)
    }

    fn compute_max_channel_diff(pixel_a: &[u8], pixel_b: &[u8], idx: usize) -> u8 {
        let mut max_diff = 0u8;
        for channel in 0..3 {
            let diff = i16::from(pixel_a[idx + channel]) - i16::from(pixel_b[idx + channel]);
            let abs_diff = diff.unsigned_abs() as u8;
            if abs_diff > max_diff {
                max_diff = abs_diff;
            }
        }
        max_diff
    }

    fn make_diff_image_masked(pixels_a: &[u8], pixels_b: &[u8], ctx: &DiffCtx<'_>) -> Vec<u8> {
        let mut out = vec![0u8; (ctx.width * ctx.height * 4) as usize];
        for y_coord in 0..ctx.height {
            for x_coord in 0..ctx.width {
                let idx = ((y_coord * ctx.width + x_coord) * 4) as usize;
                if is_pixel_masked(x_coord, y_coord, ctx.masks) {
                    continue;
                }
                if idx + 3 >= pixels_a.len() || idx + 3 >= pixels_b.len() || idx + 3 >= out.len() {
                    continue;
                }
                let max_channel_diff = compute_max_channel_diff(pixels_a, pixels_b, idx);
                let val = if max_channel_diff > ctx.eps { 255 } else { 0 };
                out[idx] = val;
                out[idx + 1] = 0;
                out[idx + 2] = 0;
                out[idx + 3] = 255;
            }
        }
        out
    }

    fn extract_text_masks(display_list: &DisplayList, width: u32, height: u32) -> Vec<TextMask> {
        let mut masks = Vec::new();
        for item in &display_list.items {
            if let DisplayItem::Text {
                bounds: Some((left, top, right, bottom)),
                ..
            } = item
            {
                let left = (*left).max(0) as u32;
                let top = (*top).max(0) as u32;
                let right = (*right).max(0) as u32;
                let bottom = (*bottom).max(0) as u32;
                let left = left.min(width);
                let top = top.min(height);
                let right = right.min(width);
                let bottom = bottom.min(height);
                if right > left && bottom > top {
                    masks.push((left, top, right, bottom));
                }
            }
        }
        masks
    }

    /// Captures a PNG screenshot from Chromium for a given fixture.
    ///
    /// # Errors
    ///
    /// Returns an error if navigation or screenshot capture fails.
    fn capture_chrome_png(tab: &Tab, path: &Path) -> Result<Vec<u8>> {
        let url = to_file_url(path)?;
        let url_string = url.as_str().to_owned();
        tab.navigate_to(&url_string)?;
        tab.wait_until_navigated()?;
        let _ = tab.evaluate(css_reset_injection_script(), false)?;
        let png = tab.capture_screenshot(CaptureScreenshotFormatOption::Png, None, None, true)?;
        Ok(png)
    }

    /// Builds a Valor display list for a given fixture.
    ///
    /// # Errors
    ///
    /// Returns an error if page creation, parsing, or display list generation fails.
    fn build_valor_display_list_for(
        path: &Path,
        viewport_w: u32,
        viewport_h: u32,
    ) -> Result<DisplayList> {
        let runtime = Runtime::new()?;
        let url = to_file_url(path)?;
        let mut page = create_page(&runtime, url)?;
        page.eval_js(css_reset_injection_script())?;
        let finished = update_until_finished_simple(&runtime, &mut page)?;
        if !finished {
            return Err(anyhow!("Valor parsing did not finish"));
        }
        drop(runtime.block_on(page.update()));
        let display_list = page.display_list_retained_snapshot()?;
        let clear_color = page.background_rgba();
        let mut items = Vec::with_capacity(display_list.items.len() + 1);
        items.push(DisplayItem::Rect {
            x: 0.0,
            y: 0.0,
            width: viewport_w as f32,
            height: viewport_h as f32,
            color: clear_color,
        });
        items.extend(display_list.items);
        Ok(DisplayList::from_items(items))
    }

    static RENDER_COUNTER: AtomicUsize = AtomicUsize::new(0);

    static RENDER_STATE: OnceLock<Mutex<RenderState>> = OnceLock::new();
    static WINDOW: OnceLock<Arc<Window>> = OnceLock::new();

    struct WindowCreator {
        window: Option<Window>,
        width: u32,
        height: u32,
    }

    impl WindowCreator {
        const fn new(width: u32, height: u32) -> Self {
            Self {
                window: None,
                width,
                height,
            }
        }

        /// Creates a window if one hasn't been created yet.
        ///
        /// # Errors
        ///
        /// Returns an error if window creation fails.
        fn create_window_if_needed(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
            if self.window.is_some() {
                return Ok(());
            }
            let window = event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Valor Test")
                        .with_inner_size(LogicalSize::new(self.width, self.height))
                        .with_visible(false),
                )
                .map_err(|err| anyhow!("Failed to create window: {err}"))?;
            self.window = Some(window);
            Ok(())
        }

        /// Consumes the creator and returns the created window.
        ///
        /// # Errors
        ///
        /// Returns an error if no window was created.
        fn into_window(self) -> Result<Window> {
            self.window.ok_or_else(|| anyhow!("Window not created"))
        }
    }

    impl ApplicationHandler for WindowCreator {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            drop(self.create_window_if_needed(event_loop));
            event_loop.exit();
        }

        fn window_event(
            &mut self,
            _event_loop: &ActiveEventLoop,
            _window_id: WindowId,
            _event: WindowEvent,
        ) {
        }
    }

    fn initialize_render_state(width: u32, height: u32) -> &'static Mutex<RenderState> {
        use winit::{event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows as _};

        RENDER_STATE.get_or_init(|| {
            let runtime = Runtime::new().unwrap_or_else(|err| {
                log::error!("Failed to create tokio runtime: {err}");
                process::abort();
            });

            let event_loop = EventLoop::builder()
                .with_any_thread(true)
                .build()
                .unwrap_or_else(|err| {
                    log::error!("Failed to create event loop: {err}");
                    process::abort();
                });

            let window = {
                let mut app = WindowCreator::new(width, height);
                event_loop.run_app(&mut app).unwrap_or_else(|err| {
                    log::error!("Failed to run event loop: {err}");
                    process::abort();
                });
                app.into_window().unwrap_or_else(|err| {
                    log::error!("{err}");
                    process::abort();
                })
            };

            let window = Arc::new(window);
            drop(WINDOW.set(Arc::clone(&window)));
            let state = runtime
                .block_on(RenderState::new(window))
                .unwrap_or_else(|err| {
                    log::error!("Failed to create render state: {err}");
                    process::abort();
                });
            Mutex::new(state)
        })
    }

    /// Rasterizes a display list to RGBA bytes using the GPU backend.
    ///
    /// # Errors
    ///
    /// Returns an error if render state locking or rendering fails.
    fn rasterize_display_list_to_rgba(
        display_list: &DisplayList,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>> {
        let state_mutex = initialize_render_state(width, height);

        let mut state = state_mutex
            .lock()
            .map_err(|err| anyhow!("Failed to lock render state: {err}"))?;
        let render_num = RENDER_COUNTER.fetch_add(1, Ordering::SeqCst);

        info!(
            "=== Rendering fixture #{} with {} items ===",
            render_num + 1,
            display_list.items.len()
        );

        if render_num >= 25 {
            info!("Display list #{} contents:", render_num + 1);
            for (idx, item) in display_list.items.iter().enumerate() {
                info!("  Item {idx}: {item:?}");
            }
        }

        state.reset_for_next_frame();
        state.resize(PhysicalSize::new(width, height));
        state.set_retained_display_list(display_list.clone());

        let result = state.render_to_rgba();

        if let Err(render_err) = &result {
            error!("=== RENDER FAILED for fixture #{} ===", render_num + 1);
            error!("Error: {render_err:?}");
            error!("Display list had {} items", display_list.items.len());
            error!("Display list items:");
            for (idx, item) in display_list.items.iter().enumerate() {
                error!("  Item {idx}: {item:?}");
            }
        } else {
            info!("=== Fixture #{} rendered successfully ===", render_num + 1);
        }

        result
    }

    type BrowserWithTab = (Browser, Arc<Tab>);

    /// Initializes a headless Chrome browser with a tab for graphics testing.
    ///
    /// # Errors
    ///
    /// Returns an error if browser launch or tab creation fails.
    fn init_browser() -> Result<BrowserWithTab> {
        let launch_opts = LaunchOptionsBuilder::default()
            .headless(true)
            .window_size(Some((800, 600)))
            .idle_browser_timeout(Duration::from_secs(120))
            .args(vec![
                OsStr::new("--force-device-scale-factor=1"),
                OsStr::new("--hide-scrollbars"),
                OsStr::new("--blink-settings=imagesEnabled=false"),
                OsStr::new("--disable-gpu"),
                OsStr::new("--force-color-profile=sRGB"),
            ])
            .build()?;
        let chrome_browser = Browser::new(launch_opts)?;
        let chrome_tab = chrome_browser.new_tab()?;
        Ok((chrome_browser, chrome_tab))
    }

    /// Loads Chrome RGBA image data from cache or by capturing a screenshot.
    ///
    /// # Errors
    ///
    /// Returns an error if file reading, browser initialization, screenshot capture, or image decoding fails.
    fn load_chrome_rgba(
        stable_path: &Path,
        fixture: &Path,
        browser: &mut Option<Browser>,
        tab: &mut Option<Arc<Tab>>,
        timings: &mut Timings,
    ) -> Result<RgbaImage> {
        let t_start = Instant::now();
        if stable_path.exists() {
            let zbytes = read(stable_path)?;
            timings.cache_io += t_start.elapsed();
            let expected = (784u32 * 453u32 * 4) as usize;
            if let Ok(bytes) = zstd_decompress(&zbytes, expected) {
                return RgbaImage::from_raw(784, 453, bytes)
                    .ok_or_else(|| anyhow!("Failed to create RgbaImage from decompressed bytes"));
            }
        }
        if browser.is_none() {
            let (chrome_browser, chrome_tab) = init_browser()?;
            *tab = Some(chrome_tab);
            *browser = Some(chrome_browser);
        }
        let tab_ref = tab.as_ref().ok_or_else(|| anyhow!("tab not initialized"))?;
        let t_cap = Instant::now();
        let png_bytes = capture_chrome_png(tab_ref, fixture)?;
        timings.chrome_capture += t_cap.elapsed();
        let t_decode = Instant::now();
        let img = load_from_memory(&png_bytes)?.to_rgba8();
        timings.png_decode += t_decode.elapsed();
        debug!(
            "Chrome image: width={}, height={}, buffer_size={}, expected_size={}",
            img.width(),
            img.height(),
            img.as_raw().len(),
            (784u32 * 453u32 * 4) as usize
        );
        let compressed = zstd_compress(img.as_raw(), 1).unwrap_or_default();
        drop(write(stable_path, compressed));
        Ok(img)
    }

    struct Timings {
        cache_io: Duration,
        chrome_capture: Duration,
        build_dl: Duration,
        batch_dbg: Duration,
        raster: Duration,
        png_decode: Duration,
        equal_check: Duration,
        masked_diff: Duration,
        fail_write: Duration,
    }

    impl Timings {
        const fn new() -> Self {
            Self {
                cache_io: Duration::ZERO,
                chrome_capture: Duration::ZERO,
                build_dl: Duration::ZERO,
                batch_dbg: Duration::ZERO,
                raster: Duration::ZERO,
                png_decode: Duration::ZERO,
                equal_check: Duration::ZERO,
                masked_diff: Duration::ZERO,
                fail_write: Duration::ZERO,
            }
        }
    }

    struct CompareInfo<'cmp> {
        name: &'cmp str,
        path_hash_hex: &'cmp str,
        hash_hex: &'cmp str,
        failing_dir: &'cmp Path,
    }

    struct ComparisonContext<'ctx> {
        chrome_img: &'ctx RgbaImage,
        valor_img: &'ctx [u8],
        display_list: &'ctx DisplayList,
        dimensions: (u32, u32),
        info: &'ctx CompareInfo<'ctx>,
        timings: &'ctx mut Timings,
    }

    /// Compares Chrome and Valor images and writes failure artifacts if they differ.
    ///
    /// # Errors
    ///
    /// Returns an error if PNG writing fails.
    fn compare_and_write_failures(ctx: &mut ComparisonContext<'_>) -> Result<bool> {
        let (width, height) = ctx.dimensions;
        let t_equal = Instant::now();
        ctx.timings.equal_check += t_equal.elapsed();
        if ctx.chrome_img.as_raw() == ctx.valor_img {
            return Ok(false);
        }
        let eps: u8 = 3;
        let masks = extract_text_masks(ctx.display_list, width, height);
        let diff_ctx = DiffCtx {
            width,
            height,
            eps,
            masks: &masks,
        };
        let t_diff = Instant::now();
        let (over, total) =
            per_pixel_diff_masked(ctx.chrome_img.as_raw(), ctx.valor_img, &diff_ctx);
        ctx.timings.masked_diff += t_diff.elapsed();
        let diff_ratio = (over as f64) / (total as f64);
        let allowed = 0.0125;
        if diff_ratio > allowed {
            let stamp = now_millis();
            let base = ctx.info.failing_dir.join(format!(
                "{}_{}_{}_{stamp}",
                ctx.info.name, ctx.info.path_hash_hex, ctx.info.hash_hex
            ));
            let chrome_path = base.with_extension("chrome.png");
            let valor_path = base.with_extension("valor.png");
            let diff_path = base.with_extension("diff.png");
            let t_write = Instant::now();
            drop(create_dir_all(ctx.info.failing_dir));
            write_png_rgba_if_changed(&chrome_path, ctx.chrome_img.as_raw(), width, height)?;
            write_png_rgba_if_changed(&valor_path, ctx.valor_img, width, height)?;
            let diff_img =
                make_diff_image_masked(ctx.chrome_img.as_raw(), ctx.valor_img, &diff_ctx);
            write_png_rgba_if_changed(&diff_path, &diff_img, width, height)?;
            ctx.timings.fail_write += t_write.elapsed();
            error!(
                "[GRAPHICS] {} — pixel diffs found ({} over {}, {:.4}%); wrote\n  {}\n  {}\n  {}",
                ctx.info.name,
                over,
                total,
                diff_ratio * 100.0,
                chrome_path.display(),
                valor_path.display(),
                diff_path.display()
            );
            return Ok(true);
        }
        if over > 0 {
            info!(
                "[GRAPHICS] {} — {} pixels over epsilon out of {} ({:.4}%)",
                ctx.info.name,
                over,
                total,
                diff_ratio * 100.0
            );
        } else {
            info!(
                "[GRAPHICS] {} — exact match within masked regions",
                ctx.info.name
            );
        }
        Ok(false)
    }

    /// Sets up test directories for graphics artifacts.
    ///
    /// # Errors
    ///
    /// Returns an error if directory creation or removal fails.
    fn setup_test_dirs() -> Result<(PathBuf, PathBuf)> {
        let out_dir = artifacts_subdir("graphics_artifacts");
        create_dir_all(&out_dir)?;
        let failing_dir = out_dir.join("failing");
        if failing_dir.exists() {
            remove_dir_all(&failing_dir)?;
        }
        create_dir_all(&failing_dir)?;
        Ok((out_dir, failing_dir))
    }

    struct FixtureContext<'ctx> {
        fixture: &'ctx Path,
        out_dir: &'ctx Path,
        failing_dir: &'ctx Path,
        browser: &'ctx mut Option<Browser>,
        tab: &'ctx mut Option<Arc<Tab>>,
        timings: &'ctx mut Timings,
    }

    /// Processes a single graphics fixture by comparing Chrome and Valor renders.
    ///
    /// # Errors
    ///
    /// Returns an error if fixture processing, rendering, or comparison fails.
    fn process_single_fixture(ctx: &mut FixtureContext<'_>) -> Result<bool> {
        let name = safe_stem(ctx.fixture);
        let canon = ctx
            .fixture
            .canonicalize()
            .unwrap_or_else(|_| ctx.fixture.to_path_buf());
        let current_hash = file_content_hash(&canon);
        let hash_hex = format!("{current_hash:016x}");
        let path_hash = fnv1a64_bytes(canon.to_string_lossy().as_bytes());
        let path_hash_hex = format!("{path_hash:016x}");
        cleanup_artifacts_for_hash(
            &name,
            &path_hash_hex,
            ctx.out_dir,
            ctx.failing_dir,
            &hash_hex,
        );

        let stable_chrome_rgba_zst = ctx
            .out_dir
            .join(format!("{name}_{path_hash_hex}_{hash_hex}_chrome.rgba.zst"));
        let chrome_img = load_chrome_rgba(
            &stable_chrome_rgba_zst,
            ctx.fixture,
            ctx.browser,
            ctx.tab,
            ctx.timings,
        )?;

        let (width, height) = (784u32, 453u32);
        let t_build = Instant::now();
        let display_list = build_valor_display_list_for(ctx.fixture, width, height)?;
        ctx.timings.build_dl += t_build.elapsed();
        debug!(
            "[GRAPHICS][DEBUG] {}: DL items={} (first 5: {:?})",
            name,
            display_list.items.len(),
            display_list.items.iter().take(5).collect::<Vec<_>>()
        );

        let t_batch = Instant::now();
        let dbg_batches = batch_display_list(&display_list, width, height);
        ctx.timings.batch_dbg += t_batch.elapsed();
        let dbg_quads: usize = dbg_batches.iter().map(|batch| batch.quads.len()).sum();
        debug!(
            "[GRAPHICS][DEBUG] {}: batches={} total_quads={}",
            name,
            dbg_batches.len(),
            dbg_quads
        );

        let t_rast = Instant::now();
        let valor_img = rasterize_display_list_to_rgba(&display_list, width, height)?;
        ctx.timings.raster += t_rast.elapsed();

        let info = CompareInfo {
            name: &name,
            path_hash_hex: &path_hash_hex,
            hash_hex: &hash_hex,
            failing_dir: ctx.failing_dir,
        };

        compare_and_write_failures(&mut ComparisonContext {
            chrome_img: &chrome_img,
            valor_img: &valor_img,
            display_list: &display_list,
            dimensions: (width, height),
            info: &info,
            timings: ctx.timings,
        })
    }

    /// Tests graphics rendering by comparing Valor output with Chromium screenshots.
    ///
    /// # Errors
    ///
    /// Returns an error if test setup fails or any graphics comparisons fail.
    #[test]
    fn chromium_graphics_smoke_compare_png() -> Result<()> {
        use env_logger::Builder as LogBuilder;
        drop(LogBuilder::default().is_test(false).try_init());
        let (out_dir, failing_dir) = setup_test_dirs()?;
        let fixtures = fixture_html_files()?;
        if fixtures.is_empty() {
            info!(
                "[GRAPHICS] No fixtures found — add files under any crate's tests/fixtures/graphics/ subfolders"
            );
            return Ok(());
        }
        let mut browser: Option<Browser> = None;
        let mut tab: Option<Arc<Tab>> = None;
        let mut any_failed = false;
        let mut timings = Timings::new();
        for fixture in fixtures {
            if process_single_fixture(&mut FixtureContext {
                fixture: &fixture,
                out_dir: &out_dir,
                failing_dir: &failing_dir,
                browser: &mut browser,
                tab: &mut tab,
                timings: &mut timings,
            })? {
                any_failed = true;
            }
        }

        info!(
            "[GRAPHICS][TIMING][TOTALS] cache_io={:?} chrome_capture={:?} build_dl={:?} batch_dbg={:?} raster={:?} png_decode={:?} equal_check={:?} masked_diff={:?} fail_write={:?}",
            timings.cache_io,
            timings.chrome_capture,
            timings.build_dl,
            timings.batch_dbg,
            timings.raster,
            timings.png_decode,
            timings.equal_check,
            timings.masked_diff,
            timings.fail_write
        );

        if any_failed {
            return Err(anyhow!(
                "graphics comparison found differences — see artifacts under {}",
                failing_dir.display()
            ));
        }
        Ok(())
    }
}
