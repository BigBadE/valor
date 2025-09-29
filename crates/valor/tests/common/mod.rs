#![allow(dead_code)]
use anyhow::{Result, anyhow};
use image::ImageEncoder;
use log::info;
use page_handler::config::ValorConfig;
use page_handler::state::HtmlPage;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::string::String as StdString;
use std::time::Duration;
use tokio::runtime::Runtime;
use url::Url;
use valor::factory::{ChromeInit, create_chrome_and_content};

/// Returns the directory containing HTML fixtures for integration tests.
pub fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// Clear an artifacts subdirectory under `target/` if the provided harness source has changed.
/// Returns the path to the (recreated) subdirectory.
pub fn clear_artifacts_subdir_if_harness_changed(name: &str, harness_src: &str) -> Result<PathBuf> {
    let dir = artifacts_subdir(name);
    let _ = fs::create_dir_all(&dir);
    let marker = dir.join(".harness_hash");
    let current = format!("{:016x}", checksum_u64(harness_src));
    let prev = fs::read_to_string(&marker).unwrap_or_default();
    if prev.trim() != current {
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir)?;
        fs::write(&marker, &current)?;
    }
    Ok(dir)
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

/// Set the layouter JSON cache directory to target/valor_layout_cache and create it.
pub fn route_layouter_cache_to_target() -> Result<PathBuf> {
    let dir = artifacts_subdir("valor_layout_cache");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Clear the `target/valor_layout_cache` directory if the provided harness source has changed.
/// Uses a small marker file to track the last seen harness hash.
pub fn clear_valor_layout_cache_if_harness_changed(harness_src: &str) -> Result<()> {
    let dir = artifacts_subdir("valor_layout_cache");
    let _ = fs::create_dir_all(&dir);
    let marker = dir.join(".harness_hash");
    let current = format!("{:016x}", checksum_u64(harness_src));
    let prev = fs::read_to_string(&marker).unwrap_or_default();
    if prev.trim() != current {
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir)?;
        fs::write(&marker, &current)?;
    }
    Ok(())
}

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

/// Read cached JSON for a fixture using a key derived from the fixture path and harness source.
pub fn read_cached_json_for_fixture(fixture_path: &Path, harness_src: &str) -> Option<Value> {
    let canon = fixture_path
        .canonicalize()
        .unwrap_or_else(|_| fixture_path.to_path_buf());
    let content_hash = fs::read_to_string(&canon)
        .map(|s| checksum_u64(&s))
        .unwrap_or(0);
    let key = format!(
        "{}|{:016x}|{:016x}",
        canon.display(),
        checksum_u64(harness_src),
        content_hash
    );
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
    let canon = fixture_path
        .canonicalize()
        .unwrap_or_else(|_| fixture_path.to_path_buf());
    let content_hash = fs::read_to_string(&canon)
        .map(|s| checksum_u64(&s))
        .unwrap_or(0);
    let key = format!(
        "{}|{:016x}|{:016x}",
        canon.display(),
        checksum_u64(harness_src),
        content_hash
    );
    let file = layout_cache_file_for_key(&key);
    if let Some(parent) = file.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let s = serde_json::to_string(v).unwrap_or_else(|_| String::from("{}"));
    fs::write(file, s)?;
    info!(
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
    info!(
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

/// Discover graphics fixture roots under every crate in the workspace: crates/*/tests/fixtures/graphics
fn workspace_crate_graphics_fixture_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let root = workspace_root_from_valor_manifest();
    let crates_dir = root.join("crates");
    if let Ok(entries) = fs::read_dir(&crates_dir) {
        for ent in entries.filter_map(|e| e.ok()) {
            let cdir = ent.path();
            if cdir.is_dir() {
                let p = cdir.join("tests").join("fixtures").join("graphics");
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
    // This applies both to valor's tests/fixtures/layout/* and each crate's tests/fixtures/layout/* structure.
    files.retain(|p| {
        // Must have a parent directory that is not named "fixtures" (i.e., at least one subdir)
        let parent_not_fixtures = p
            .parent()
            .and_then(|d| d.file_name())
            .map(|n| n != "fixtures")
            .unwrap_or(false);

        // And somewhere in its ancestors there must be a directory named "fixtures"
        let mut has_fixtures_ancestor = false;
        for anc in p.ancestors().skip(1) {
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
    // Bound each async update with a small timeout to avoid hangs inside the update path during tests.
    // This keeps the test harness responsive even if an await-point stalls.
    let per_tick_timeout = Duration::from_millis(25);
    // Also enforce a hard wall-clock budget for the whole helper to avoid very long stalls if every tick times out.
    let start_time = std::time::Instant::now();
    let max_total_time = Duration::from_secs(15);
    let mut timed_out_ticks: u32 = 0;
    for iter in 0..10_000 {
        // Abort if we exceeded the total time budget
        if start_time.elapsed() > max_total_time {
            eprintln!(
                "update_until_finished: exceeded total time budget after {iter} iters ({timed_out_ticks} timeouts)"
            );
            break;
        }
        // Run one update tick with a timeout guard. If it times out, continue trying rather than hanging.
        let update_result: Result<Result<(), anyhow::Error>, tokio::time::error::Elapsed> =
            rt.block_on(async { tokio::time::timeout(per_tick_timeout, page.update()).await });
        match update_result {
            Ok(Ok(())) => { /* progressed */ }
            Ok(Err(err)) => {
                return Err(err);
            }
            Err(_elapsed) => {
                timed_out_ticks = timed_out_ticks.saturating_add(1);
            }
        }

        // Allow the caller to drain mirrors or perform additional per-tick work.
        per_tick(page)?;
        if page.parsing_finished() {
            finished = true;
            break;
        }
        if iter % 500 == 0 {
            eprintln!(
                "update_until_finished: iter={} finished={} timeouts={} elapsed_ms={}",
                iter,
                finished,
                timed_out_ticks,
                start_time.elapsed().as_millis()
            );
        }
        // Yield to background tasks without requiring a Tokio reactor on this thread
        std::thread::sleep(Duration::from_millis(1));
    }
    Ok(finished)
}

/// Drive page.update() until parsing finishes (no per-tick callback). Returns true if finished in time.
pub fn update_until_finished_simple(rt: &Runtime, page: &mut HtmlPage) -> Result<bool> {
    update_until_finished(rt, page, |_| Ok(()))
}

/// Compare two serde_json Values with an epsilon for numeric differences.
/// - Floats/integers are compared as f64 within `eps`.
/// - Strings, bools, null are compared for exact equality.
/// - Arrays/Objects must have identical structure and are compared recursively.
///   Returns Ok(()) if equal under epsilon; Err with a path-detailed message otherwise.
pub fn compare_json_with_epsilon(actual: &Value, expected: &Value, eps: f64) -> Result<(), String> {
    fn extract_id_label(v: &Value) -> Option<String> {
        if let Value::Object(map) = v
            && let Some(Value::Object(attrs)) = map.get("attrs")
            && let Some(Value::String(id)) = attrs.get("id")
        {
            return Some(format!("#{id}"));
        }
        if let Value::Object(map) = v
            && let Some(Value::String(id)) = map.get("id")
        {
            return Some(format!("#{id}"));
        }
        None
    }

    fn is_element_object(v: &Value) -> bool {
        if let Value::Object(map) = v {
            map.contains_key("tag") && map.contains_key("rect")
        } else {
            false
        }
    }

    fn helper(
        a: &Value,
        b: &Value,
        eps: f64,
        path: &mut Vec<StdString>,
        elem_stack: &mut Vec<(Value, Value)>,
    ) -> Result<(), StdString> {
        use serde_json::Value::*;
        type CmpFn = fn(
            &serde_json::Value,
            &serde_json::Value,
            f64,
            &mut Vec<StdString>,
            &mut Vec<(serde_json::Value, serde_json::Value)>,
        ) -> Result<(), StdString>;
        struct CmpState<'a> {
            eps: f64,
            path: &'a mut Vec<StdString>,
            elem_stack: &'a mut Vec<(serde_json::Value, serde_json::Value)>,
        }
        #[inline]
        fn call_with_elem_ctx(
            xv: &serde_json::Value,
            yv: &serde_json::Value,
            state: &mut CmpState<'_>,
            helper: CmpFn,
        ) -> Result<(), StdString> {
            let should_push = is_element_object(xv) && is_element_object(yv);
            if should_push {
                state.elem_stack.push((xv.clone(), yv.clone()));
            }
            let result = helper(xv, yv, state.eps, state.path, state.elem_stack);
            if should_push {
                state.elem_stack.pop();
            }
            result
        }
        // Special-case: ignore root-level rect width/height diffs (border-box model vs platform viewport quirks)
        // The path segments are stored as components like ".rect", ".width".
        if elem_stack.len() <= 1 && path.len() >= 2 {
            let last = path[path.len() - 1].as_str();
            let prev = path[path.len() - 2].as_str();
            if (last == ".width" || last == ".height") && prev == ".rect" {
                return Ok(());
            }
        }
        match (a, b) {
            (Null, Null) => Ok(()),
            (Bool(x), Bool(y)) => {
                if x == y {
                    Ok(())
                } else {
                    Err(build_err(
                        "bool mismatch",
                        &format!("{x} != {y}"),
                        path,
                        elem_stack,
                    ))
                }
            }
            (Number(x), Number(y)) => match (x.as_f64(), y.as_f64()) {
                (Some(xf), Some(yf)) => {
                    if (xf - yf).abs() <= eps {
                        Ok(())
                    } else {
                        Err(build_err(
                            "number diff",
                            &format!("{xf} vs {yf} exceeds eps {eps}"),
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
            },
            (String(xs), String(ys)) => {
                if xs == ys {
                    Ok(())
                } else {
                    Err(build_err(
                        "string mismatch",
                        &format!("'{xs}' != '{ys}'"),
                        path,
                        elem_stack,
                    ))
                }
            }
            (Array(xa), Array(ya)) => {
                if xa.len() != ya.len() {
                    return Err(build_err(
                        "array length mismatch",
                        &format!("{} != {}", xa.len(), ya.len()),
                        path,
                        elem_stack,
                    ));
                }
                for (i, (xe, ye)) in xa.iter().zip(ya.iter()).enumerate() {
                    // If this array is a children array, label with element id and compress the path
                    let is_children_ctx = path.last().map(|s| s == ".children").unwrap_or(false);
                    if is_children_ctx {
                        let label = extract_id_label(xe)
                            .or_else(|| extract_id_label(ye))
                            .unwrap_or_else(|| i.to_string());
                        // Replace trailing ".children" with ".<label>" (label is "#id" or numeric)
                        path.pop();
                        path.push(format!(".{label}"));
                    } else {
                        path.push(format!("[{i}]"));
                    }
                    // Maintain element context for better error snippets
                    let mut state = CmpState {
                        eps,
                        path,
                        elem_stack,
                    };
                    let r = call_with_elem_ctx(xe, ye, &mut state, helper);
                    path.pop();
                    r?;
                }
                Ok(())
            }
            (Object(xo), Object(yo)) => {
                if xo.len() != yo.len() {
                    return Err(build_err(
                        "object size mismatch",
                        &format!("{} != {}", xo.len(), yo.len()),
                        path,
                        elem_stack,
                    ));
                }
                for (k, xv) in xo.iter() {
                    match yo.get(k) {
                        Some(yv) => {
                            path.push(format!(".{k}"));
                            // Keep element context
                            let mut state = CmpState {
                                eps,
                                path,
                                elem_stack,
                            };
                            let r = call_with_elem_ctx(xv, yv, &mut state, helper);
                            path.pop();
                            r?;
                        }
                        None => {
                            return Err(build_err(
                                "missing key in expected",
                                &format!("'{k}'"),
                                path,
                                elem_stack,
                            ));
                        }
                    }
                }
                Ok(())
            }
            (x, y) => Err(build_err(
                "type mismatch",
                &format!("{:?} vs {:?}", type_name(x), type_name(y)),
                path,
                elem_stack,
            )),
        }
    }

    #[allow(clippy::excessive_nesting)]
    fn build_err(
        kind: &str,
        detail: &str,
        path: &[String],
        elem_stack: &[(Value, Value)],
    ) -> String {
        fn child_summary_lines(children: &[serde_json::Value]) -> Vec<serde_json::Value> {
            use serde_json::Value::*;
            let mut lines: Vec<serde_json::Value> = Vec::new();
            for c in children {
                if let Object(cm) = c {
                    let ctag = cm.get("tag").and_then(|x| x.as_str()).unwrap_or("");
                    let cid = cm.get("id").and_then(|x| x.as_str()).unwrap_or("");
                    let (cx, cy, cw, ch) = if let Some(Object(r)) = cm.get("rect") {
                        let cx = r.get("x").and_then(|n| n.as_f64()).unwrap_or(0.0);
                        let cy = r.get("y").and_then(|n| n.as_f64()).unwrap_or(0.0);
                        let cw = r.get("width").and_then(|n| n.as_f64()).unwrap_or(0.0);
                        let ch = r.get("height").and_then(|n| n.as_f64()).unwrap_or(0.0);
                        (cx, cy, cw, ch)
                    } else {
                        (0.0, 0.0, 0.0, 0.0)
                    };
                    let s = if !cid.is_empty() {
                        format!("<{ctag} id=#{cid}> rect=({cx:.0},{cy:.0},{cw:.0},{ch:.0})")
                    } else {
                        format!("<{ctag}> rect=({cx:.0},{cy:.0},{cw:.0},{ch:.0})")
                    };
                    lines.push(serde_json::Value::String(s));
                }
            }
            lines
        }

        fn pretty_elem_with_compact_children(v: &Value) -> String {
            use serde_json::Value::*;
            if let Object(map) = v {
                // clone the element and replace children with compact summaries
                let mut omap = map.clone();
                if let Some(Array(children)) = map.get("children") {
                    let lines = child_summary_lines(children);
                    omap.insert("children".to_string(), Array(lines));
                }
                let vv = Object(omap);
                serde_json::to_string_pretty(&vv).unwrap_or_else(|_| StdString::from("{}"))
            } else {
                serde_json::to_string_pretty(v).unwrap_or_else(|_| StdString::from("{}"))
            }
        }

        let path_str = format_path(path);
        let (our_elem, ch_elem) = if let Some((a, b)) = elem_stack.last() {
            (a, b)
        } else {
            (&Value::Null, &Value::Null)
        };
        let our_s = pretty_elem_with_compact_children(our_elem);
        let ch_s = pretty_elem_with_compact_children(ch_elem);
        if detail.is_empty() {
            format!("{path_str}: {kind}\nElement (our): {our_s}\nElement (chromium): {ch_s}")
        } else {
            format!(
                "{path_str}: {kind} — {detail}\nElement (our): {our_s}\nElement (chromium): {ch_s}"
            )
        }
    }

    fn format_path(path: &[String]) -> String {
        if path.is_empty() {
            String::new()
        } else {
            let joined = path.join("");
            if let Some(stripped) = joined.strip_prefix('.') {
                stripped.to_string()
            } else {
                joined
            }
        }
    }
    fn type_name(v: &Value) -> &'static str {
        match v {
            Value::Null => "null",
            Value::Bool(_) => "bool",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        }
    }
    let mut elem_stack: Vec<(Value, Value)> = Vec::new();
    // Seed context with root if it looks like an element
    if is_element_object(actual) && is_element_object(expected) {
        elem_stack.push((actual.clone(), expected.clone()));
    }
    helper(actual, expected, eps, &mut Vec::new(), &mut elem_stack)
}

/// Returns a JS snippet that injects a CSS Reset into the current document in Chromium tests.
/// This should be executed via `tab.evaluate(script, /*return_by_value=*/ false)` after navigation.
pub fn css_reset_injection_script() -> &'static str {
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
