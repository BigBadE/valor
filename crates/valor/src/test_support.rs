//! Shared test support utilities (moved from tests/common/mod.rs) so both tests and bins can reuse them.

#![allow(
    dead_code,
    reason = "Test support module with utilities used selectively"
)]

use crate::factory::{ChromeInit, create_chrome_and_content};
use anyhow::{Result, anyhow};
use core::time::Duration;
use image::{ColorType, ImageEncoder as _, codecs::png::PngEncoder};
use log::warn;
use page_handler::config::ValorConfig;
use page_handler::state::HtmlPage;
use serde_json::{Number, Value, from_str, to_string};
use std::collections::HashSet;
use std::env::var as env_var;
use std::fs::{
    self as std_fs, create_dir_all, read, read_dir, read_to_string, remove_dir_all, write,
};
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Instant;
use tokio::runtime::Runtime;
use url::Url;

/// Returns the directory containing HTML fixtures for integration tests.
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

// ===== Shared artifacts and caching utilities =====

/// Return the target directory for build/test outputs.
pub fn target_dir() -> PathBuf {
    if let Ok(dir) = env_var("CARGO_TARGET_DIR") {
        return PathBuf::from(dir);
    }
    // Derive workspace root target from this crate's manifest dir: crates/valor -> ../../target
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("..").join("..").join("target")
}

/// Return a subdirectory under target for test artifacts.
pub fn artifacts_subdir(name: &str) -> PathBuf {
    target_dir().join(name)
}

/// Remove and recreate a directory, ignoring errors on remove.
///
/// # Errors
/// Returns an error if directory creation fails.
pub fn clear_dir(dir: &Path) -> Result<()> {
    if dir.exists() {
        let _remove_result: Result<(), _> = remove_dir_all(dir);
    }
    create_dir_all(dir)?;
    Ok(())
}

/// Write bytes to a path only if they differ from any existing contents. Returns true if written.
///
/// # Errors
/// Returns an error if file operations fail.
pub fn write_bytes_if_changed(path: &Path, bytes: &[u8]) -> Result<bool> {
    if let Ok(existing) = read(path)
        && existing == bytes
    {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        let _dir_result: Result<(), _> = create_dir_all(parent);
    }
    write(path, bytes)?;
    Ok(true)
}

/// Encode and write an RGBA8 image only if changed. Returns true if written.
///
/// # Errors
/// Returns an error if encoding or file operations fail.
pub fn write_png_rgba_if_changed(
    path: &Path,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Result<bool> {
    let mut buf = Vec::new();
    let encoder = PngEncoder::new(&mut buf);
    encoder.write_image(rgba, width, height, ColorType::Rgba8.into())?;
    write_bytes_if_changed(path, &buf)
}

/// Read cached JSON for a fixture using a key derived from the fixture path and harness source.
pub fn read_cached_json_for_fixture(fixture_path: &Path, harness_src: &str) -> Option<Value> {
    fn checksum_u64(input_str: &str) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325; // FNV-1a 64-bit
        for byte in input_str.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
        }
        hash
    }
    fn layout_cache_file_for_key(key: &str) -> PathBuf {
        let dir = artifacts_subdir("valor_layout_cache");
        let _dir_result: Result<(), _> = std_fs::create_dir_all(&dir);
        let hash_val = checksum_u64(key);
        dir.join(format!("{hash_val:016x}.json"))
    }

    let canon = fixture_path
        .canonicalize()
        .unwrap_or_else(|_| fixture_path.to_path_buf());
    let key = format!("{}|{:016x}", canon.display(), checksum_u64(harness_src));
    let file = layout_cache_file_for_key(&key);
    if !file.exists() {
        return None;
    }
    let contents = read_to_string(file).ok()?;
    from_str(&contents).ok()
}

/// Write cached JSON for a fixture using a key derived from the fixture path and harness source.
///
/// # Errors
/// Returns an error if file operations fail.
pub fn write_cached_json_for_fixture(
    fixture_path: &Path,
    harness_src: &str,
    json_value: &Value,
) -> Result<()> {
    fn checksum_u64(input_str: &str) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325; // FNV-1a 64-bit
        for byte in input_str.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
        }
        hash
    }
    fn layout_cache_file_for_key(key: &str) -> PathBuf {
        let dir = artifacts_subdir("valor_layout_cache");
        let _dir_result: Result<(), _> = create_dir_all(&dir);
        let hash_val = checksum_u64(key);
        dir.join(format!("{hash_val:016x}.json"))
    }

    let canon = fixture_path
        .canonicalize()
        .unwrap_or_else(|_| fixture_path.to_path_buf());
    let key = format!("{}|{:016x}", canon.display(), checksum_u64(harness_src));
    let file = layout_cache_file_for_key(&key);
    if let Some(parent) = file.parent() {
        let _dir_result: Result<(), _> = create_dir_all(parent);
    }
    let json_str = to_string(json_value).unwrap_or_else(|_| String::from("{}"));
    write(file, json_str)?;
    log::info!(
        "[CACHE] wrote chromium JSON for {} to target/valor_layout_cache",
        canon.display()
    );
    Ok(())
}

/// Write a named JSON variant for a fixture (e.g., "chromium", "valor").
///
/// # Errors
/// Returns an error if file operations fail.
pub fn write_named_json_for_fixture(
    fixture_path: &Path,
    harness_src: &str,
    name: &str,
    json_value: &Value,
) -> Result<()> {
    fn checksum_u64(input_str: &str) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325; // FNV-1a 64-bit
        for byte in input_str.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
        }
        hash
    }
    fn layout_cache_file_for_key(key: &str) -> PathBuf {
        let dir = artifacts_subdir("valor_layout_cache");
        let _dir_result: Result<(), _> = create_dir_all(&dir);
        let hash_val = checksum_u64(key);
        dir.join(format!("{hash_val:016x}.json"))
    }

    let canon = fixture_path
        .canonicalize()
        .unwrap_or_else(|_| fixture_path.to_path_buf());
    let key = format!(
        "{}|{:016x}|{}",
        canon.display(),
        checksum_u64(harness_src),
        name
    );
    let file = layout_cache_file_for_key(&key);
    if let Some(parent) = file.parent() {
        let _dir_result: Result<(), _> = create_dir_all(parent);
    }
    let json_str = to_string(json_value).unwrap_or_else(|_| String::from("{}"));
    write(file, json_str)?;
    log::info!(
        "[CACHE] wrote {} JSON for {} to target/valor_layout_cache",
        name,
        canon.display()
    );
    Ok(())
}

pub fn fixtures_layout_dir() -> PathBuf {
    fixtures_dir().join("layout")
}

/// Returns the workspace root directory.
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
    let modules_dir = root.join("crates").join("css").join("modules");
    if let Ok(entries) = std_fs::read_dir(&modules_dir) {
        for ent in entries.filter_map(Result::ok) {
            let mod_dir = ent.path();
            if mod_dir.is_dir() {
                let fixture_path = mod_dir.join("tests").join("fixtures");
                if fixture_path.exists() {
                    roots.push(fixture_path);
                }
            }
        }
    }
    roots
}

pub fn fixtures_css_dir() -> PathBuf {
    fixtures_dir().join("css")
}

/// Discover layout fixture roots under every crate in the workspace: crates/*/tests/fixtures/layout
fn workspace_crate_layout_fixture_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let root = workspace_root_from_valor_manifest();
    let crates_dir = root.join("crates");
    if let Ok(entries) = std_fs::read_dir(&crates_dir) {
        for ent in entries.filter_map(Result::ok) {
            let krate_dir = ent.path();
            if krate_dir.is_dir() {
                let layout_path = krate_dir.join("tests").join("fixtures").join("layout");
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
    let entries = std_fs::read_dir(dir)
        .map_err(|err| anyhow!("Failed to read dir {}: {}", dir.display(), err))?;
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
                    .filter(|path| {
                        path.extension()
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
    files.retain(|path| {
        let parent_not_fixtures = path
            .parent()
            .and_then(|dir| dir.file_name())
            .is_some_and(|name| name != "fixtures");
        let mut has_fixtures_ancestor = false;
        for anc in path.ancestors().skip(1) {
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
    for path in files {
        let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
        if seen.insert(canon) {
            unique.push(path);
        }
    }
    Ok(unique)
}

/// Discover all .html files under each crate's tests/fixtures/graphics directory recursively.
///
/// # Errors
/// Returns an error if directory traversal fails.
pub fn graphics_fixture_html_files() -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = Vec::new();
    // Valor crate local graphics fixtures
    let local = fixtures_dir().join("graphics");
    if local.exists() {
        collect_html_recursively(&local, &mut files)?;
    }
    // Include graphics fixtures from every crate in the workspace
    for root in workspace_crate_layout_fixture_roots() {
        collect_html_recursively(&root, &mut files)?;
    }
    // Keep only files that are under a subdirectory of a fixtures folder (not directly under .../fixtures).
    files.retain(|path| {
        path.parent()
            .and_then(|dir| {
                dir.parent()
                    .map(|parent_parent| (parent_parent.file_name(), dir.file_name()))
            })
            .is_some_and(|(pp_name, dir_name)| {
                let is_under_fixtures = pp_name.is_some_and(|name| name == "fixtures");
                let not_directly_under_fixtures = dir_name.is_some_and(|name| name != "fixtures");
                is_under_fixtures && not_directly_under_fixtures
            })
    });
    files.sort();
    Ok(files)
}

/// Convert a local file Path to a file:// Url, after canonicalizing when possible.
///
/// # Errors
/// Returns an error if the path cannot be converted to a URL.
pub fn to_file_url(path: &Path) -> Result<Url> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    Url::from_file_path(&canonical)
        .map_err(|()| anyhow!("Invalid file path for URL: {}", canonical.display()))
}

/// Construct an `HtmlPage` using the provided Runtime and Url.
///
/// # Errors
/// Returns an error if page creation fails.
pub fn create_page(runtime: &Runtime, url: Url) -> Result<HtmlPage> {
    let config = ValorConfig::from_env();
    let page = runtime.block_on(HtmlPage::new(runtime.handle(), url, config))?;
    Ok(page)
}

/// Construct chrome and content pages using the same wiring as the Valor binary.
/// Returns the `ChromeInit` bundle from `valor::factory` for further use.
///
/// # Errors
/// Returns an error if page creation fails.
pub fn create_chrome_and_content_for_tests(
    runtime: &Runtime,
    initial_content_url: Url,
) -> Result<ChromeInit> {
    create_chrome_and_content(runtime, initial_content_url)
}

/// Drive `page.update()` until parsing finishes, running an optional per-tick callback (e.g., drain mirrors).
/// Returns true if finished within the allotted iterations.
///
/// # Errors
/// Returns an error if page updates fail.
pub fn update_until_finished<F>(
    runtime: &Runtime,
    page: &mut HtmlPage,
    mut per_tick: F,
) -> Result<bool>
where
    F: FnMut(&mut HtmlPage) -> Result<()>,
{
    let mut finished = page.parsing_finished();
    // Bound overall time to avoid hangs; use small sleeps between iterations to yield.
    let start_time = Instant::now();
    let max_total_time = Duration::from_secs(15);
    let timed_out_ticks: u32 = 0;
    for iter in 0i32..10_000i32 {
        if start_time.elapsed() > max_total_time {
            warn!(
                "update_until_finished: exceeded total time budget after {iter} iters ({timed_out_ticks} timeouts)"
            );
            break;
        }
        let fut = page.update();
        // Drive one update tick synchronously. If this stalls internally, the outer loop time budget will trip.
        let _update_result: Result<(), _> = runtime.block_on(fut);
        per_tick(page)?;
        finished = page.parsing_finished();
        if finished {
            break;
        }
        // Light backoff to avoid hot spinning; also keeps UI thread responsive if needed.
        sleep(Duration::from_millis(2));
    }
    Ok(finished)
}

/// Drive `page.update()` until parsing finishes (no per-tick callback). Returns true if finished in time.
///
/// # Errors
/// Returns an error if page updates fail.
pub fn update_until_finished_simple(runtime: &Runtime, page: &mut HtmlPage) -> Result<bool> {
    update_until_finished(runtime, page, |_page| Ok(()))
}

/// Compare two `serde_json` Values with an epsilon for numeric differences.
/// - Floats/integers are compared as f64 within `eps`.
/// - Strings, bools, null are compared for exact equality.
/// - Arrays/Objects must have identical structure and are compared recursively.
///   Returns Ok(()) if equal under epsilon; Err with a path-detailed message otherwise.
///
/// # Errors
/// Returns an error string if values differ.
pub fn compare_json_with_epsilon(actual: &Value, expected: &Value, eps: f64) -> Result<(), String> {
    fn eq_nums(num_a: &Number, num_b: &Number, eps: f64) -> bool {
        let float_a = num_a.as_f64().unwrap_or(0.0f64);
        let float_b = num_b.as_f64().unwrap_or(0.0f64);
        (float_a - float_b).abs() <= eps
    }
    match (actual, expected) {
        (Value::Null, Value::Null) => Ok(()),
        (Value::Bool(bool_a), Value::Bool(bool_b)) if bool_a == bool_b => Ok(()),
        (Value::Number(num_a), Value::Number(num_b)) if eq_nums(num_a, num_b, eps) => Ok(()),
        (Value::String(str_a), Value::String(str_b)) if str_a == str_b => Ok(()),
        (Value::Array(arr_a), Value::Array(arr_b)) => {
            if arr_a.len() != arr_b.len() {
                return Err(format!(
                    "array length mismatch: {} vs {}",
                    arr_a.len(),
                    arr_b.len()
                ));
            }
            for (index, (val_a, val_b)) in arr_a.iter().zip(arr_b.iter()).enumerate() {
                compare_json_with_epsilon(val_a, val_b, eps)
                    .map_err(|err| format!("[{index}]{err}"))?;
            }
            Ok(())
        }
        (Value::Object(obj_a), Value::Object(obj_b)) => {
            if obj_a.len() != obj_b.len() {
                return Err(format!(
                    "object size mismatch: {} vs {}",
                    obj_a.len(),
                    obj_b.len()
                ));
            }
            for (key, val_a) in obj_a {
                let Some(val_b) = obj_b.get(key) else {
                    return Err(format!("missing key: {key}"));
                };
                compare_json_with_epsilon(val_a, val_b, eps)
                    .map_err(|err| format!(".{key}{err}"))?;
            }
            Ok(())
        }
        _ => Err(format!("type/value mismatch: {actual:?} vs {expected:?}")),
    }
}

/// Returns a JS snippet that injects a CSS Reset into the current document in Chromium tests.
/// This should be executed via `tab.evaluate(script, /*return_by_value=*/ false)` after navigation.
pub const fn css_reset_injection_script() -> &'static str {
    r#"(function(){
        try {
            var css = "*,*::before,*::after{box-sizing:border-box;margin:0;padding:0;font-family:monospace,'Courier New',Courier,Consolas,'Liberation Mono',Menlo,Monaco,'DejaVu Sans Mono',monospace;}html,body{margin:0 !important;padding:0 !important;scrollbar-gutter:stable;}body{margin:0 !important;}h1,h2,h3,h4,h5,h6,p{margin:0;padding:0;}ul,ol{margin:0;padding:0;list-style:none;}";
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
