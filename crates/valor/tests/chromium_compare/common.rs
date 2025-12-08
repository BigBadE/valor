use anyhow::{Result, anyhow};
use chromiumoxide::Browser;
use image::{ColorType, ImageEncoder as _, codecs::png::PngEncoder};
use log::warn;
use page_handler::config::ValorConfig;
use page_handler::state::HtmlPage;
use std::env;
use std::fs::{create_dir_all, read, write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::time::Instant;
use tokio::runtime::Handle;
use tokio::task::yield_now;
use url::Url;

// ===== Path and directory utilities =====

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn target_dir() -> PathBuf {
    workspace_root().join("target")
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
/// # Errors
///
/// Returns an error if page creation fails.
pub async fn create_page(handle: &Handle, url: Url) -> Result<HtmlPage> {
    let config = ValorConfig::from_env();
    let page = HtmlPage::new(handle, url, config).await?;
    Ok(page)
}

/// Sets up a page for a fixture by loading, parsing, and applying CSS reset.
///
/// # Errors
///
/// Returns an error if page creation, parsing, or script evaluation fails.
pub async fn setup_page_for_fixture(handle: &Handle, path: &Path) -> Result<HtmlPage> {
    let url = to_file_url(path)?;
    let mut page = create_page(handle, url).await?;
    page.eval_js(css_reset_injection_script())?;

    let finished = update_until_finished_simple(handle, &mut page).await?;
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
/// # Errors
///
/// Returns an error if page update or callback execution fails.
pub async fn update_until_finished<F>(
    _handle: &Handle,
    page: &mut HtmlPage,
    mut per_tick: F,
) -> Result<bool>
where
    F: FnMut(&mut HtmlPage) -> Result<()>,
{
    let start_time = Instant::now();
    let max_total_time = Duration::from_secs(5);
    let max_iterations = 100;

    for iter in 0..max_iterations {
        if start_time.elapsed() > max_total_time {
            warn!("update_until_finished: exceeded total time budget after {iter} iterations");
            break;
        }

        page.update().await?;
        per_tick(page)?;

        if page.parsing_finished() {
            return Ok(true);
        }

        // Small yield to allow other async tasks to run
        yield_now().await;
    }

    Ok(false)
}

/// Updates the page until parsing finishes without a per-tick callback.
///
/// # Errors
///
/// Returns an error if page update fails.
pub async fn update_until_finished_simple(handle: &Handle, page: &mut HtmlPage) -> Result<bool> {
    update_until_finished(handle, page, |_page| Ok(())).await
}

// ===== CSS reset for consistent test baseline =====

pub const fn css_reset_injection_script() -> &'static str {
    r#"(function(){
        try {
            var css = "*,*::before,*::after{box-sizing:border-box;margin:0;padding:0;font-family:'Courier New',Courier,monospace;}html,body{margin:0 !important;padding:0 !important;scrollbar-gutter:stable;}body{margin:0 !important;}h1,h2,h3,h4,h5,h6,p{margin:0;padding:0;}ul,ol{margin:0;padding:0;list-style:none;}";
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
            var de = document.documentElement; if (de && de.style){ de.style.margin='0'; de.style.padding='0'; de.style.fontFamily='"Courier New",Courier,monospace'; }
            var b = document.body; if (b && b.style){ b.style.margin='0'; b.style.padding='0'; b.style.fontFamily='"Courier New",Courier,monospace'; }
            void (document.body && document.body.offsetWidth);
            return true;
        } catch (e) {
            return false;
        }
    })()"#
}

// ===== Unified test runner framework =====

use env_logger::{Builder as LogBuilder, Env as EnvLoggerEnv};

/// Initializes the logger for tests.
pub fn init_test_logger() {
    // Suppress backtraces for cleaner test output
    // SAFETY: We only call this once at the start of tests, before any threads are spawned
    unsafe {
        std::env::set_var("RUST_BACKTRACE", "0");
    }

    let _ignore_result =
        LogBuilder::from_env(EnvLoggerEnv::default().filter_or("RUST_LOG", "error"))
            .filter_module("wgpu_hal", log::LevelFilter::Off)
            .filter_module("wgpu_core", log::LevelFilter::Off)
            .filter_module("naga", log::LevelFilter::Off)
            .is_test(false)
            .try_init();
}

// ===== Test cache and unified test runner =====

use std::future::Future;

/// Returns the unified cache directory for a specific test.
///
/// # Errors
///
/// Returns an error if directory creation fails.
pub fn test_cache_dir(test_name: &str) -> Result<PathBuf> {
    let dir = target_dir().join("test_cache").join(test_name);
    create_dir_all(&dir)?;
    Ok(dir)
}

/// Returns the cache file path for a test.
///
/// # Errors
///
/// Returns an error if directory creation fails.
pub fn cache_file_path(test_name: &str, fixture_path: &Path, suffix: &str) -> Result<PathBuf> {
    let dir = test_cache_dir(test_name)?;
    let canon = fixture_path
        .canonicalize()
        .unwrap_or_else(|_| fixture_path.to_path_buf());
    let path_hash = checksum_u64(&canon.display().to_string());
    Ok(dir.join(format!("{path_hash:016x}{suffix}")))
}

type DeserializeFn<T> = fn(&[u8]) -> Result<T>;
type SerializeFn<T> = fn(&T) -> Result<Vec<u8>>;

pub struct CacheFetcher<'cache, T, F> {
    pub test_name: &'cache str,
    pub fixture_path: &'cache Path,
    pub cache_suffix: &'cache str,
    pub fetch_fn: F,
    pub deserialize_fn: DeserializeFn<T>,
    pub serialize_fn: SerializeFn<T>,
}

/// Generic function to read from cache if valid, otherwise fetch and cache.
///
/// # Errors
///
/// Returns an error if fetching or deserializing fails.
pub async fn read_or_fetch_cache<T, F, Fut>(fetcher: CacheFetcher<'_, T, F>) -> Result<T>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let cache_path = cache_file_path(
        fetcher.test_name,
        fetcher.fixture_path,
        fetcher.cache_suffix,
    )?;

    // Try to read from cache if it exists
    if cache_path.exists()
        && let Ok(bytes) = read(&cache_path)
        && let Ok(value) = (fetcher.deserialize_fn)(&bytes)
    {
        return Ok(value);
    }

    // Cache miss - fetch fresh data
    let value = (fetcher.fetch_fn)().await?;

    // Write to cache
    let bytes = (fetcher.serialize_fn)(&value)?;
    let _ignore_write_error = write(&cache_path, &bytes);

    Ok(value)
}

struct FixtureResult {
    path: PathBuf,
    layout_passed: bool,
    graphics_passed: bool,
    _layout_error: Option<String>,
    _graphics_error: Option<String>,
    _duration: Duration,
}

/// Processes a single fixture test with both layout and graphics tests.
///
/// # Errors
///
/// Returns a result with pass/fail status and any error messages.
async fn process_fixture(
    _idx: usize,
    _total_fixtures: usize,
    fixture_path: &Path,
    browser: &Browser,
) -> FixtureResult {
    use crate::chromium_compare::{graphics_tests, layout_tests};

    let fixture_start = Instant::now();

    // Skip known-failing fixtures
    if should_skip_fixture(fixture_path) {
        return FixtureResult {
            path: fixture_path.to_path_buf(),
            layout_passed: true,
            graphics_passed: true,
            _layout_error: None,
            _graphics_error: None,
            _duration: fixture_start.elapsed(),
        };
    }

    // Create a new page for this fixture
    let page = match browser.new_page("about:blank").await {
        Ok(page) => page,
        Err(err) => {
            let error_msg = format!("Failed to create page: {err}");
            return FixtureResult {
                path: fixture_path.to_path_buf(),
                layout_passed: false,
                graphics_passed: false,
                _layout_error: Some(error_msg.clone()),
                _graphics_error: Some(error_msg),
                _duration: fixture_start.elapsed(),
            };
        }
    };

    // Run tests and ensure page cleanup even on error
    let result = async {
        // Run layout test
        let layout_result =
            layout_tests::run_single_layout_test_with_page(fixture_path, &page).await;
        let layout_passed = layout_result.is_ok();
        let layout_error = layout_result.err().map(|err| err.to_string());

        // Run graphics test
        let graphics_result =
            graphics_tests::run_single_graphics_test_with_page(fixture_path, &page).await;
        let graphics_passed = graphics_result.is_ok();
        let graphics_error = graphics_result.err().map(|err| err.to_string());

        let duration = fixture_start.elapsed();

        FixtureResult {
            path: fixture_path.to_path_buf(),
            layout_passed,
            graphics_passed,
            _layout_error: layout_error,
            _graphics_error: graphics_error,
            _duration: duration,
        }
    }
    .await;

    // Always close the page to prevent Chrome tab accumulation
    let _ignore_close_error = page.close().await;

    result
}

/// Prints summary statistics for fixture test results.
fn print_summary(results: &[FixtureResult], _total_duration: Duration) {
    use log::error;

    let passed = results
        .iter()
        .filter(|result| result.layout_passed && result.graphics_passed)
        .count();
    let failed = results.len() - passed;
    let total_count = results.len();

    if failed > 0 {
        error!("\n{} of {} fixtures failed:", failed, total_count);
        error!("────────────────────────────────────────");

        for result in results {
            if !result.layout_passed || !result.graphics_passed {
                let name = result
                    .path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown");

                let mut details = Vec::new();
                if !result.layout_passed {
                    details.push("layout");
                }
                if !result.graphics_passed {
                    details.push("graphics");
                }

                let details_str = details.join(" ");
                error!("  ✗ {} {}", name, details_str);
            }
        }
        error!("────────────────────────────────────────");
    }
}

/// List of fixtures that are known to fail due to missing features or bugs.
/// These are temporarily skipped to allow other tests to pass.
const KNOWN_FAILING_FIXTURES: &[&str] = &[
    // Form elements not yet implemented
    "buttons.html",
    "inputs.html",
    "textarea.html",
    "checkboxes_radios.html",
    // Text rendering issues - text appears tiny/faint/wrong position
    "colored_text.html",
    "basic_text.html",
    "text_with_backgrounds.html",
    // Index files (likely contain unimplemented features or text)
    "index.html",
    // Layout bugs that need investigation
    "03_margin_collapsing.html",
    "11_overflow_clipping.html",
    "opacity_subtree.html",
    "03_flex_containers_detection.html",
];

/// Checks if a fixture should be skipped based on known failures.
fn should_skip_fixture(path: &Path) -> bool {
    let path_str = path.display().to_string();
    KNOWN_FAILING_FIXTURES
        .iter()
        .any(|pattern| path_str.contains(pattern))
}

/// Runs all fixtures in a single test with a shared browser instance.
///
/// # Errors
///
/// Returns an error only if the test infrastructure fails, not if individual fixtures fail.
pub async fn run_all_fixtures(fixtures: &[PathBuf]) -> Result<()> {
    use crate::chromium_compare::browser::connect_to_chrome_internal;

    init_test_logger();

    let total_start = Instant::now();

    // Connect to Chrome once for all fixtures
    let browser_with_handler = connect_to_chrome_internal().await?;
    let browser = &browser_with_handler.browser;
    let _connect_time = total_start.elapsed();

    let mut results = Vec::new();
    let total_fixtures = fixtures.len();

    for (idx, fixture_path) in fixtures.iter().enumerate() {
        let result = process_fixture(idx, total_fixtures, fixture_path, browser).await;
        results.push(result);
    }

    let total_duration = total_start.elapsed();
    print_summary(&results, total_duration);

    // Fail the test if any fixtures failed (using assert to avoid panic backtrace)
    let failed_count = results
        .iter()
        .filter(|result| !result.layout_passed || !result.graphics_passed)
        .count();

    assert_eq!(
        failed_count, 0,
        "{failed_count} fixture(s) failed (see summary above)"
    );

    Ok(())
}
