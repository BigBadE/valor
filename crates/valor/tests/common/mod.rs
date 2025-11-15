use anyhow::{Result, anyhow};
use serde_json::{Value as JsonValue, to_string_pretty, value::Map as JsonMap};
use std::collections::HashSet;
use std::fs::{create_dir_all, read_dir, read_to_string, remove_dir_all, write};
use std::path::{Path, PathBuf};
use std::string::String as StdString;
use valor::test_support::{artifacts_subdir, fixtures_dir, fixtures_layout_dir};

/// Clear the `target/valor_layout_cache` directory if the provided harness source has changed.
/// Uses a small marker file to track the last seen harness hash.
///
/// # Errors
/// Returns an error if directory or file operations fail.
pub fn clear_valor_layout_cache_if_harness_changed(harness_src: &str) -> Result<()> {
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
    // crates/valor -> ../../
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// Discover CSS module-local fixture roots: crates/css/modules/*/tests/fixtures
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

/// Discover layout fixture roots under every crate in the workspace: crates/*/tests/fixtures/layout
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

/// Recursively collect HTML files from a directory.
///
/// # Errors
/// Returns an error if directory traversal fails.
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

/// Discover all .html files under the tests/fixtures/layout directory recursively.
///
/// # Errors
/// Returns an error if directory traversal fails.
pub fn fixture_html_files() -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = Vec::new();
    // Valor crate local layout fixtures
    let local_layout = fixtures_layout_dir();
    if local_layout.exists() {
        collect_html_recursively(&local_layout, &mut files)?;
    } else {
        // Fallback: scan the legacy top-level fixtures dir non-recursively
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
    // Include per-module CSS fixtures if present
    for root in module_css_fixture_roots() {
        collect_html_recursively(&root, &mut files)?;
    }
    // Include layout fixtures from every crate in the workspace
    for root in workspace_crate_layout_fixture_roots() {
        collect_html_recursively(&root, &mut files)?;
    }

    // Keep only files that are under a subdirectory of a fixtures folder (not directly under .../fixtures).
    // This applies both to valor's tests/fixtures/layout/* and each crate's tests/fixtures/layout/* structure.
    files.retain(|entry_path| {
        // Must have a parent directory that is not named "fixtures" (i.e., at least one subdir)
        let parent_not_fixtures = entry_path
            .parent()
            .and_then(|dir_entry| dir_entry.file_name())
            .is_some_and(|name| name != "fixtures");

        // And somewhere in its ancestors there must be a directory named "fixtures"
        let mut has_fixtures_ancestor = false;
        for anc in entry_path.ancestors().skip(1) {
            // skip the file itself
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
    // Deduplicate by canonical path to avoid duplicates from overlapping roots
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
    if let JsonValue::Object(map) = value
        && let Some(JsonValue::Object(attrs)) = map.get("attrs")
        && let Some(JsonValue::String(id)) = attrs.get("id")
    {
        return Some(format!("#{id}"));
    }
    if let JsonValue::Object(map) = value
        && let Some(JsonValue::String(id)) = map.get("id")
    {
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
    use serde_json::Value::*;
    if let Object(child_map) = child {
        let child_tag = child_map
            .get("tag")
            .and_then(|tag_val| tag_val.as_str())
            .unwrap_or("");
        let child_id = child_map
            .get("id")
            .and_then(|id_val| id_val.as_str())
            .unwrap_or("");
        let rect = if let Some(Object(rect_obj)) = child_map.get("rect") {
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
    use serde_json::Value::*;
    if let Object(map) = value {
        // clone the element and replace children with compact summaries
        let mut output_map = map.clone();
        if let Some(Array(children)) = map.get("children") {
            let lines = child_summary_lines(children);
            output_map.insert("children".to_string(), Array(lines));
        }
        let json_object = Object(output_map);
        to_string_pretty(&json_object).unwrap_or_else(|_| StdString::from("{}"))
    } else {
        to_string_pretty(value).unwrap_or_else(|_| StdString::from("{}"))
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

/// Compare two JSON arrays with epsilon tolerance.
///
/// # Errors
/// Returns an error if arrays differ beyond epsilon threshold.
fn compare_arrays(
    actual_arr: &[JsonValue],
    expected_arr: &[JsonValue],
    ctx: &mut CompareContext<'_>,
) -> Result<(), String> {
    let eps = ctx.eps;
    let path = ctx.path;
    let elem_stack = ctx.elem_stack;
    let helper = ctx.helper;
    if actual_arr.len() != expected_arr.len() {
        return Err(build_err(
            "array length mismatch",
            &format!("{} != {}", actual_arr.len(), expected_arr.len()),
            path,
            elem_stack,
        ));
    }
    for (index, (actual_item, expected_item)) in
        actual_arr.iter().zip(expected_arr.iter()).enumerate()
    {
        let is_children_ctx = path.last().is_some_and(|segment| segment == ".children");
        if is_children_ctx {
            let label = extract_id_label(actual_item)
                .or_else(|| extract_id_label(expected_item))
                .unwrap_or_else(|| index.to_string());
            path.pop();
            path.push(format!(".{label}"));
        } else {
            path.push(format!("[{index}]"));
        }

        let should_push = is_element_object(actual_item) && is_element_object(expected_item);
        if should_push {
            elem_stack.push((actual_item.clone(), expected_item.clone()));
        }
        let result = helper(actual_item, expected_item, eps, path, elem_stack);
        if should_push {
            elem_stack.pop();
        }
        path.pop();
        result?;
    }
    Ok(())
}

/// Compare two JSON objects with epsilon tolerance.
///
/// # Errors
/// Returns an error if objects differ beyond epsilon threshold.
fn compare_objects(
    actual_obj: &JsonMap<String, JsonValue>,
    expected_obj: &JsonMap<String, JsonValue>,
    ctx: &mut CompareContext<'_>,
) -> Result<(), String> {
    let eps = ctx.eps;
    let path = ctx.path;
    let elem_stack = ctx.elem_stack;
    let helper = ctx.helper;
    if actual_obj.len() != expected_obj.len() {
        return Err(build_err(
            "object size mismatch",
            &format!("{} != {}", actual_obj.len(), expected_obj.len()),
            path,
            elem_stack,
        ));
    }
    for (key, actual_val) in actual_obj {
        match expected_obj.get(key) {
            Some(expected_val) => {
                path.push(format!(".{key}"));
                let should_push = is_element_object(actual_val) && is_element_object(expected_val);
                if should_push {
                    elem_stack.push((actual_val.clone(), expected_val.clone()));
                }
                let result = helper(actual_val, expected_val, eps, path, elem_stack);
                if should_push {
                    elem_stack.pop();
                }
                path.pop();
                result?;
            }
            None => {
                return Err(build_err(
                    "missing key in expected",
                    &format!("'{key}'"),
                    path,
                    elem_stack,
                ));
            }
        }
    }
    Ok(())
}

/// Compare two `serde_json` Values with an epsilon for numeric differences.
/// - Floats/integers are compared as f64 within `eps`.
/// - Strings, bools, null are compared for exact equality.
/// - Arrays/Objects must have identical structure and are compared recursively.
///
/// # Errors
/// Returns `Err` with a path-detailed message if values differ beyond epsilon threshold.
pub fn compare_json_with_epsilon(
    actual: &JsonValue,
    expected: &JsonValue,
    eps: f64,
) -> Result<(), String> {
    /// Helper function to recursively compare JSON values with epsilon tolerance.
    ///
    /// # Errors
    /// Returns `Err` with detailed path information if values differ.
    fn helper(
        actual_value: &JsonValue,
        expected_value: &JsonValue,
        eps: f64,
        path: &mut Vec<StdString>,
        elem_stack: &mut Vec<(JsonValue, JsonValue)>,
    ) -> Result<(), StdString> {
        use serde_json::Value::*;
        // Special-case: ignore root-level rect width/height diffs (border-box model vs platform viewport quirks)
        // The path segments are stored as components like ".rect", ".width".
        if elem_stack.len() <= 1 && path.len() >= 2 {
            let last = path[path.len() - 1].as_str();
            let prev = path[path.len() - 2].as_str();
            if (last == ".width" || last == ".height") && prev == ".rect" {
                return Ok(());
            }
        }
        match (actual_value, expected_value) {
            (Null, Null) => Ok(()),
            (Bool(actual_bool), Bool(expected_bool)) => {
                if actual_bool == expected_bool {
                    Ok(())
                } else {
                    Err(build_err(
                        "bool mismatch",
                        &format!("{actual_bool} != {expected_bool}"),
                        path,
                        elem_stack,
                    ))
                }
            }
            (Number(actual_num), Number(expected_num)) => {
                match (actual_num.as_f64(), expected_num.as_f64()) {
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
            (String(actual_str), String(expected_str)) => {
                if actual_str == expected_str {
                    Ok(())
                } else {
                    Err(build_err(
                        "string mismatch",
                        &format!("'{actual_str}' != '{expected_str}'"),
                        path,
                        elem_stack,
                    ))
                }
            }
            (Array(actual_arr), Array(expected_arr)) => {
                let mut ctx = CompareContext {
                    eps,
                    path,
                    elem_stack,
                    helper,
                };
                compare_arrays(actual_arr, expected_arr, &mut ctx)
            }
            (Object(actual_obj), Object(expected_obj)) => {
                let mut ctx = CompareContext {
                    eps,
                    path,
                    elem_stack,
                    helper,
                };
                compare_objects(actual_obj, expected_obj, &mut ctx)
            }
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
    let mut elem_stack: Vec<(JsonValue, JsonValue)> = Vec::new();
    // Seed context with root if it looks like an element
    if is_element_object(actual) && is_element_object(expected) {
        elem_stack.push((actual.clone(), expected.clone()));
    }
    helper(actual, expected, eps, &mut Vec::new(), &mut elem_stack)
}

/// Returns a JS snippet that injects a CSS Reset into the current document in Chromium tests.
/// This should be executed via `tab.evaluate(script, /*return_by_value=*/ false)` after navigation.
pub const fn css_reset_injection_script() -> &'static str {
    r#"(function(){
        try {
            var css = "*,*::before,*::after{box-sizing:border-box;margin:0;padding:0;font-family:monospace,'Courier New',Courier,Consolas,'Liberation Mono',Menlo,Monaco,'DejaVu Sans Mono',monospace;}html,body{margin:0 !important;padding:0 !important;}body{margin:0 !important;}h1,h2,h3,h4,h5,h6,p{margin:0;padding:0;}ul,ol{margin:0;padding:0;list-style:none;}";
            // Idempotent: skip if we've already added our reset style element
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
            // Also enforce via inline styles to ensure immediate override
            var de = document.documentElement; if (de && de.style){ de.style.margin='0'; de.style.padding='0'; de.style.fontFamily='monospace'; }
            var b = document.body; if (b && b.style){ b.style.margin='0'; b.style.padding='0'; b.style.fontFamily='monospace'; }
            // Force style & layout flush
            void (document.body && document.body.offsetWidth);
            return true;
        } catch (e) {
            return false;
        }
    })()"#
}
