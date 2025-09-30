//! Shared test support utilities (moved from tests/common/mod.rs) so both tests and bins can reuse them.

#![allow(dead_code)]

use crate::factory::{ChromeInit, create_chrome_and_content};
use anyhow::{Result, anyhow};
use image::ImageEncoder;
use log::warn;
use page_handler::config::ValorConfig;
use page_handler::state::HtmlPage;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
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
    if let Ok(dir) = std::env::var("CARGO_TARGET_DIR") {
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
pub fn clear_dir(dir: &Path) -> Result<()> {
    if dir.exists() {
        let _ = fs::remove_dir_all(dir);
    }
    fs::create_dir_all(dir)?;
    Ok(())
}

/// Write bytes to a path only if they differ from any existing contents. Returns true if written.
pub fn write_bytes_if_changed(path: &Path, bytes: &[u8]) -> Result<bool> {
    if let Ok(existing) = fs::read(path)
        && existing == bytes
    {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(path, bytes)?;
    Ok(true)
}

/// Encode and write an RGBA8 image only if changed. Returns true if written.
pub fn write_png_rgba_if_changed(
    path: &Path,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Result<bool> {
    let mut buf = Vec::new();
    {
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        encoder.write_image(rgba, width, height, image::ColorType::Rgba8.into())?;
    }
    write_bytes_if_changed(path, &buf)
}

/// Read cached JSON for a fixture using a key derived from the fixture path and harness source.
pub fn read_cached_json_for_fixture(fixture_path: &Path, harness_src: &str) -> Option<Value> {
    fn checksum_u64(s: &str) -> u64 {
        let mut hash: u64 = 0xcbf29ce484222325; // FNV-1a 64-bit
        for b in s.as_bytes() {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
        }
        hash
    }
    fn layout_cache_file_for_key(key: &str) -> PathBuf {
        let dir = artifacts_subdir("valor_layout_cache");
        let _ = fs::create_dir_all(&dir);
        let h = checksum_u64(key);
        dir.join(format!("{h:016x}.json"))
    }

    let canon = fixture_path
        .canonicalize()
        .unwrap_or_else(|_| fixture_path.to_path_buf());
    let key = format!("{}|{:016x}", canon.display(), checksum_u64(harness_src));
    let file = layout_cache_file_for_key(&key);
    if !file.exists() {
        return None;
    }
    fs::read_to_string(file)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

/// Write cached JSON for a fixture using a key derived from the fixture path and harness source.
pub fn write_cached_json_for_fixture(
    fixture_path: &Path,
    harness_src: &str,
    v: &Value,
) -> Result<()> {
    fn checksum_u64(s: &str) -> u64 {
        let mut hash: u64 = 0xcbf29ce484222325; // FNV-1a 64-bit
        for b in s.as_bytes() {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
        }
        hash
    }
    fn layout_cache_file_for_key(key: &str) -> PathBuf {
        let dir = artifacts_subdir("valor_layout_cache");
        let _ = fs::create_dir_all(&dir);
        let h = checksum_u64(key);
        dir.join(format!("{h:016x}.json"))
    }

    let canon = fixture_path
        .canonicalize()
        .unwrap_or_else(|_| fixture_path.to_path_buf());
    let key = format!("{}|{:016x}", canon.display(), checksum_u64(harness_src));
    let file = layout_cache_file_for_key(&key);
    if let Some(parent) = file.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let s = serde_json::to_string(v).unwrap_or_else(|_| String::from("{}"));
    fs::write(file, s)?;
    eprintln!(
        "[CACHE] wrote chromium JSON for {} to target/valor_layout_cache",
        canon.display()
    );
    Ok(())
}

/// Write a named JSON variant for a fixture (e.g., "chromium", "valor").
pub fn write_named_json_for_fixture(
    fixture_path: &Path,
    harness_src: &str,
    name: &str,
    v: &Value,
) -> Result<()> {
    fn checksum_u64(s: &str) -> u64 {
        let mut hash: u64 = 0xcbf29ce484222325; // FNV-1a 64-bit
        for b in s.as_bytes() {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
        }
        hash
    }
    fn layout_cache_file_for_key(key: &str) -> PathBuf {
        let dir = artifacts_subdir("valor_layout_cache");
        let _ = fs::create_dir_all(&dir);
        let h = checksum_u64(key);
        dir.join(format!("{h:016x}.json"))
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
        let _ = fs::create_dir_all(parent);
    }
    let s = serde_json::to_string(v).unwrap_or_else(|_| String::from("{}"));
    fs::write(file, s)?;
    eprintln!(
        "[CACHE] wrote {} JSON for {} to target/valor_layout_cache",
        name,
        canon.display()
    );
    Ok(())
}

pub fn fixtures_layout_dir() -> PathBuf {
    fixtures_dir().join("layout")
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
    let modules_dir = root.join("crates").join("css").join("modules");
    if let Ok(entries) = fs::read_dir(&modules_dir) {
        for ent in entries.filter_map(|e| e.ok()) {
            let mdir = ent.path();
            if mdir.is_dir() {
                let p = mdir.join("tests").join("fixtures");
                if p.exists() {
                    roots.push(p);
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
    if let Ok(entries) = fs::read_dir(&crates_dir) {
        for ent in entries.filter_map(|e| e.ok()) {
            let cdir = ent.path();
            if cdir.is_dir() {
                let p = cdir.join("tests").join("fixtures").join("layout");
                if p.exists() {
                    roots.push(p);
                }
            }
        }
    }
    roots
}

fn collect_html_recursively(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries =
        fs::read_dir(dir).map_err(|e| anyhow!("Failed to read dir {}: {}", dir.display(), e))?;
    for entry in entries.filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.is_dir() {
            collect_html_recursively(&p, out)?;
        } else if p
            .extension()
            .map(|ext| ext.eq_ignore_ascii_case("html"))
            .unwrap_or(false)
        {
            out.push(p);
        }
    }
    Ok(())
}

/// Discover all .html files under the tests/fixtures/layout directory recursively.
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
            let entries = fs::read_dir(&legacy)
                .map_err(|e| anyhow!("Failed to read fixtures dir {}: {}", legacy.display(), e))?;
            files.extend(
                entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| {
                        p.extension()
                            .map(|ext| ext.eq_ignore_ascii_case("html"))
                            .unwrap_or(false)
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
    files.retain(|p| {
        let parent_not_fixtures = p
            .parent()
            .and_then(|d| d.file_name())
            .map(|n| n != "fixtures")
            .unwrap_or(false);
        let mut has_fixtures_ancestor = false;
        for anc in p.ancestors().skip(1) {
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
    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::with_capacity(files.len());
    for p in files {
        let canon = p.canonicalize().unwrap_or(p.clone());
        if seen.insert(canon) {
            unique.push(p);
        }
    }
    Ok(unique)
}

/// Discover all .html files under each crate's tests/fixtures/graphics directory recursively.
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
    files.retain(|p| {
        p.parent()
            .and_then(|d| d.parent().map(|pp| (pp.file_name(), d.file_name())))
            .map(|(pp_name, d_name)| {
                let is_under_fixtures = pp_name.map(|n| n == "fixtures").unwrap_or(false);
                let not_directly_under_fixtures = d_name.map(|n| n != "fixtures").unwrap_or(false);
                is_under_fixtures && not_directly_under_fixtures
            })
            .unwrap_or(false)
    });
    files.sort();
    Ok(files)
}

/// Convert a local file Path to a file:// Url, after canonicalizing when possible.
pub fn to_file_url(p: &Path) -> Result<Url> {
    let canonical = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    Url::from_file_path(&canonical)
        .map_err(|_| anyhow!("Invalid file path for URL: {}", canonical.display()))
}

/// Construct an HtmlPage using the provided Runtime and Url.
pub fn create_page(rt: &Runtime, url: Url) -> Result<HtmlPage> {
    let config = ValorConfig::from_env();
    let page = rt.block_on(HtmlPage::new(rt.handle(), url, config))?;
    Ok(page)
}

/// Construct chrome and content pages using the same wiring as the Valor binary.
/// Returns the `ChromeInit` bundle from `valor::factory` for further use.
pub fn create_chrome_and_content_for_tests(
    rt: &Runtime,
    initial_content_url: Url,
) -> Result<ChromeInit> {
    create_chrome_and_content(rt, initial_content_url)
}

/// Drive page.update() until parsing finishes, running an optional per-tick callback (e.g., drain mirrors).
/// Returns true if finished within the allotted iterations.
pub fn update_until_finished<F>(rt: &Runtime, page: &mut HtmlPage, mut per_tick: F) -> Result<bool>
where
    F: FnMut(&mut HtmlPage) -> Result<()>,
{
    let mut finished = page.parsing_finished();
    // Bound overall time to avoid hangs; use small sleeps between iterations to yield.
    let start_time = std::time::Instant::now();
    let max_total_time = Duration::from_secs(15);
    let timed_out_ticks: u32 = 0;
    for iter in 0..10_000 {
        if start_time.elapsed() > max_total_time {
            warn!(
                "update_until_finished: exceeded total time budget after {iter} iters ({timed_out_ticks} timeouts)"
            );
            break;
        }
        let fut = page.update();
        // Drive one update tick synchronously. If this stalls internally, the outer loop time budget will trip.
        let _ = rt.block_on(fut);
        per_tick(page)?;
        finished = page.parsing_finished();
        if finished {
            break;
        }
        // Light backoff to avoid hot spinning; also keeps UI thread responsive if needed.
        std::thread::sleep(Duration::from_millis(2));
    }
    Ok(finished)
}

/// Drive page.update() until parsing finishes (no per-tick callback). Returns true if finished in time.
pub fn update_until_finished_simple(rt: &Runtime, page: &mut HtmlPage) -> Result<bool> {
    update_until_finished(rt, page, |_p| Ok(()))
}

/// Compare two serde_json Values with an epsilon for numeric differences.
/// - Floats/integers are compared as f64 within `eps`.
/// - Strings, bools, null are compared for exact equality.
/// - Arrays/Objects must have identical structure and are compared recursively.
///   Returns Ok(()) if equal under epsilon; Err with a path-detailed message otherwise.
pub fn compare_json_with_epsilon(actual: &Value, expected: &Value, eps: f64) -> Result<(), String> {
    fn eq_nums(a: &serde_json::Number, b: &serde_json::Number, eps: f64) -> bool {
        let fa = a.as_f64().unwrap_or(0.0);
        let fb = b.as_f64().unwrap_or(0.0);
        (fa - fb).abs() <= eps
    }
    match (actual, expected) {
        (Value::Null, Value::Null) => Ok(()),
        (Value::Bool(a), Value::Bool(b)) if a == b => Ok(()),
        (Value::Number(a), Value::Number(b)) if eq_nums(a, b, eps) => Ok(()),
        (Value::String(a), Value::String(b)) if a == b => Ok(()),
        (Value::Array(aa), Value::Array(bb)) => {
            if aa.len() != bb.len() {
                return Err(format!(
                    "array length mismatch: {} vs {}",
                    aa.len(),
                    bb.len()
                ));
            }
            for (i, (av, bv)) in aa.iter().zip(bb.iter()).enumerate() {
                compare_json_with_epsilon(av, bv, eps).map_err(|e| format!("[{i}]{e}"))?;
            }
            Ok(())
        }
        (Value::Object(ao), Value::Object(bo)) => {
            if ao.len() != bo.len() {
                return Err(format!(
                    "object size mismatch: {} vs {}",
                    ao.len(),
                    bo.len()
                ));
            }
            for (k, av) in ao.iter() {
                let Some(bv) = bo.get(k) else {
                    return Err(format!("missing key: {k}"));
                };
                compare_json_with_epsilon(av, bv, eps).map_err(|e| format!(".{k}{e}"))?;
            }
            Ok(())
        }
        _ => Err(format!("type/value mismatch: {actual:?} vs {expected:?}")),
    }
}

/// Returns a JS snippet that injects a CSS Reset into the current document in Chromium tests.
/// This should be executed via `tab.evaluate(script, /*return_by_value=*/ false)` after navigation.
pub fn css_reset_injection_script() -> &'static str {
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
