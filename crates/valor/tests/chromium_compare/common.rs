use anyhow::{Result, anyhow};
use image::{ColorType, ImageEncoder as _, codecs::png::PngEncoder};
use log::warn;
use page_handler::config::ValorConfig;
use page_handler::state::HtmlPage;
use serde_json::{self, Value, from_str, to_string};
use std::collections::HashSet;
use std::env;
use std::fs::{create_dir_all, read, read_dir, read_to_string, remove_dir_all, write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::time::Instant;
use url::Url;

// ===== CLI argument parsing =====

/// Parses fixture filter from command-line arguments.
///
/// Supports multiple formats:
/// - `fixture=pattern`
/// - `--fixture=pattern`
/// - `--fixture pattern`
/// - `layout-filter=pattern` (legacy)
/// - `run_chromium_layouts::pattern` (legacy test name format)
pub fn cli_fixture_filter() -> Option<String> {
    let mut args = env::args();
    let _ = args.next();
    let mut pending_value_for: Option<String> = None;
    for arg in args {
        if let Some(rest) = arg.strip_prefix("run_chromium_layouts::")
            && !rest.is_empty()
        {
            return Some(rest.to_string());
        }
        if let Some(rest) = arg.strip_prefix("run_chromium_tests::")
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

// ===== Path and directory utilities =====

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn target_dir() -> PathBuf {
    workspace_root().join("target")
}

pub fn artifacts_subdir(name: &str) -> PathBuf {
    target_dir().join(name)
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

fn fixtures_layout_dir() -> PathBuf {
    fixtures_dir().join("layout")
}

// ===== FNV-1a hash for cache keys =====

pub const fn checksum_u64(input_str: &str) -> u64 {
    let bytes = input_str.as_bytes();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut index = 0;
    while index < bytes.len() {
        hash ^= bytes[index] as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
        index += 1;
    }
    hash
}

// ===== File I/O utilities =====

/// Writes bytes to a file only if the content has changed.
///
/// # Errors
///
/// Returns an error if file I/O operations fail.
fn write_bytes_if_changed(path: &Path, bytes: &[u8]) -> Result<bool> {
    if let Ok(existing) = read(path)
        && existing == bytes
    {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    write(path, bytes)?;
    Ok(true)
}

/// Writes RGBA image data as a PNG file only if the content has changed.
///
/// # Errors
///
/// Returns an error if PNG encoding or file I/O operations fail.
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

// ===== JSON caching =====

/// Returns the cache file path for a given key.
///
/// # Errors
///
/// Returns an error if directory creation fails.
fn layout_cache_file_for_key(key: &str) -> Result<PathBuf> {
    let dir = artifacts_subdir("valor_layout_cache");
    create_dir_all(&dir)?;
    let hash_val = checksum_u64(key);
    Ok(dir.join(format!("{hash_val:016x}.json")))
}

pub fn read_cached_json_for_fixture(fixture_path: &Path, harness_src: &str) -> Option<Value> {
    let canon = fixture_path
        .canonicalize()
        .unwrap_or_else(|_| fixture_path.to_path_buf());
    let key = format!("{}|{:016x}", canon.display(), checksum_u64(harness_src));
    let file = layout_cache_file_for_key(&key).ok()?;
    if !file.exists() {
        return None;
    }
    let contents = read_to_string(file).ok()?;
    from_str(&contents).ok()
}

/// Writes cached JSON data for a fixture.
///
/// # Errors
///
/// Returns an error if directory creation, JSON serialization, or file I/O operations fail.
pub fn write_cached_json_for_fixture(
    fixture_path: &Path,
    harness_src: &str,
    json_value: &Value,
) -> Result<()> {
    let canon = fixture_path
        .canonicalize()
        .unwrap_or_else(|_| fixture_path.to_path_buf());
    let key = format!("{}|{:016x}", canon.display(), checksum_u64(harness_src));
    let file = layout_cache_file_for_key(&key)?;
    if let Some(parent) = file.parent() {
        create_dir_all(parent)?;
    }
    let json_str = to_string(json_value)?;
    write(file, json_str)?;
    Ok(())
}

/// Writes named JSON data for a fixture.
///
/// # Errors
///
/// Returns an error if directory creation, JSON serialization, or file I/O operations fail.
pub fn write_named_json_for_fixture(
    fixture_path: &Path,
    harness_src: &str,
    name: &str,
    json_value: &Value,
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
    let file = layout_cache_file_for_key(&key)?;
    if let Some(parent) = file.parent() {
        create_dir_all(parent)?;
    }
    let json_str = to_string(json_value)?;
    write(file, json_str)?;
    Ok(())
}

// ===== Fixture discovery =====

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

fn module_css_fixture_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let root = workspace_root();
    let modules_dir = root.join("crates").join("css").join("modules");
    if let Ok(entries) = read_dir(&modules_dir) {
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

fn workspace_crate_layout_fixture_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let root = workspace_root();
    let crates_dir = root.join("crates");
    if let Ok(entries) = read_dir(&crates_dir) {
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

/// Returns all fixture HTML files from the workspace.
///
/// # Errors
///
/// Returns an error if directory reading fails.
pub fn fixture_html_files() -> Result<Vec<PathBuf>> {
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
                    .filter(|path| {
                        path.extension()
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

// ===== Page utilities =====

/// Converts a file path to a file URL.
///
/// # Errors
///
/// Returns an error if the path cannot be converted to a valid file URL.
pub fn to_file_url(path: &Path) -> Result<Url> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    Url::from_file_path(&canonical)
        .map_err(|()| anyhow!("Invalid file path for URL: {}", canonical.display()))
}

/// Creates a new HTML page for the given URL.
///
/// The parser runs in the background. Call page.update() repeatedly to check
/// for parsing progress - it will naturally yield via timeout-based polling.
///
/// # Errors
///
/// Returns an error if page creation fails.
pub async fn create_page(handle: &tokio::runtime::Handle, url: Url) -> Result<HtmlPage> {
    let config = ValorConfig::from_env();
    let page = HtmlPage::new(handle, url, config).await?;
    Ok(page)
}

/// Creates a new HTML page using the current tokio handle.
///
/// The parser runs in the background. Call page.update() repeatedly to check
/// for parsing progress - it will naturally yield via timeout-based polling.
///
/// # Errors
///
/// Returns an error if page creation fails.
pub async fn create_page_from_current(url: Url) -> Result<HtmlPage> {
    let config = ValorConfig::from_env();
    let handle = tokio::runtime::Handle::current();
    let page = HtmlPage::new(&handle, url, config).await?;
    Ok(page)
}

/// Sets up a page for a fixture by loading, parsing, and applying CSS reset.
///
/// # Errors
///
/// Returns an error if page creation, parsing, or script evaluation fails.
pub async fn setup_page_for_fixture(
    handle: &tokio::runtime::Handle,
    path: &Path,
) -> Result<HtmlPage> {
    let url = to_file_url(path)?;
    let mut page = create_page(handle, url).await?;
    page.eval_js(css_reset_injection_script())?;

    let finished = update_until_finished_simple(&mut page).await?;
    if !finished {
        return Err(anyhow!(
            "Page parsing did not finish for {}",
            path.display()
        ));
    }

    page.update().await?;
    Ok(page)
}

/// Updates the page until parsing finishes, calling a callback per tick.
///
/// Each update naturally yields to the parser task via timeout-based polling,
/// so no manual yielding is needed. The parser runs in the background and
/// page.update() checks for completion with a 1ms timeout on each call.
///
/// # Errors
///
/// Returns an error if page update or callback execution fails.
pub async fn update_until_finished<F>(page: &mut HtmlPage, mut per_tick: F) -> Result<bool>
where
    F: FnMut(&mut HtmlPage) -> Result<()>,
{
    let start_time = Instant::now();
    let max_total_time = Duration::from_secs(15);

    // Loop until parsing completes or timeout
    while !page.parsing_finished() {
        if start_time.elapsed() > max_total_time {
            warn!("update_until_finished: exceeded total time budget of 15s");
            return Ok(false);
        }

        // Update and yield to allow parser task to run
        page.update().await?;
        per_tick(page)?;

        // Small sleep to yield to other tasks (parser running in background)
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    // Final update after parsing completes
    page.update().await?;
    per_tick(page)?;

    Ok(true)
}

/// Updates the page until parsing finishes without a per-tick callback.
///
/// # Errors
///
/// Returns an error if page update fails.
pub async fn update_until_finished_simple(page: &mut HtmlPage) -> Result<bool> {
    update_until_finished(page, |_page| Ok(())).await
}

// ===== CSS reset for consistent test baseline =====

pub const fn css_reset_injection_script() -> &'static str {
    r#"(function(){
        try {
            var css = "*,*::before,*::after{box-sizing:border-box;margin:0;padding:0;font-family:monospace,'Courier New',Courier,Consolas,'Liberation Mono',Menlo,Monaco,'DejaVu Sans Mono',monospace;}html,body{margin:0 !important;padding:0 !important;scrollbar-gutter:stable;}body{margin:0 !important;}h1,h2,h3,h4,h5,h6,p{margin:0;padding:0;}ul,ol{margin:0;padding:0;list-style:none;}";
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

/// Clears the valor layout cache if the harness source has changed.
///
/// # Errors
///
/// Returns an error if directory creation or file write operations fail.
pub fn clear_valor_layout_cache_if_harness_changed(harness_src: &str) -> Result<()> {
    let dir = artifacts_subdir("valor_layout_cache");
    create_dir_all(&dir)?;
    let marker = dir.join(".harness_hash");
    let current = format!("{:016x}", checksum_u64(harness_src));
    let prev = read_to_string(&marker).unwrap_or_default();
    if prev.trim() != current {
        let _ignore_error = remove_dir_all(&dir);
        create_dir_all(&dir)?;
        write(&marker, &current)?;
    }
    Ok(())
}

// ===== Unified test runner framework =====

use env_logger::{Builder as LogBuilder, Env as EnvLoggerEnv};
use log::info;

/// Initializes the logger for tests.
pub fn init_test_logger() {
    let _ignore_result =
        LogBuilder::from_env(EnvLoggerEnv::default().filter_or("RUST_LOG", "warn"))
            .is_test(false)
            .try_init();
}

/// Returns filtered fixtures based on CLI args.
///
/// # Errors
///
/// Returns an error if fixture discovery fails.
pub fn get_filtered_fixtures(test_name: &str) -> Result<Vec<PathBuf>> {
    let all = fixture_html_files()?;
    let focus = cli_fixture_filter();
    if let Some(filter) = &focus {
        info!("[{test_name}] focusing fixtures containing (CLI): {filter}");
    }
    info!("[{test_name}] discovered {} fixtures", all.len());

    if let Some(filter) = &focus {
        Ok(all
            .into_iter()
            .filter(|path| path.display().to_string().contains(filter))
            .collect())
    } else {
        Ok(all)
    }
}
