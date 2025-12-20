use anyhow::{Result, anyhow};
use image::{ColorType, ImageEncoder as _, codecs::png::PngEncoder};
use log::{LevelFilter, warn};
use page_handler::{HtmlPage, ValorConfig};
use std::env::{set_var, var};
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

pub fn target_dir() -> PathBuf {
    workspace_root().join("target")
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
    let mut config = ValorConfig::from_env();
    // Override viewport dimensions from env if set, otherwise use test defaults
    config.viewport_width = var("VALOR_VIEWPORT_WIDTH")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(config.viewport_width);
    config.viewport_height = var("VALOR_VIEWPORT_HEIGHT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(config.viewport_height);
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

    let finished = update_until_finished_simple(handle, &mut page).await?;
    if !finished {
        return Err(anyhow!(
            "Page parsing did not finish for {}",
            path.display()
        ));
    }

    // Inject CSS reset is now done AFTER parsing completes
    // See setup_page_with_css_reset() below
    Ok(page)
}

/// Inject CSS reset AFTER HTML parsing completes to ensure correct source order
pub async fn inject_css_reset_after_parsing(page: &mut HtmlPage) -> Result<()> {
    // Ensure all pending DOMUpdates from parsing are fully processed
    // This prevents race conditions where CSS reset might be processed before HTML styles
    // Call update() multiple times to ensure the async channel is fully drained
    page.update().await?;
    page.update().await?;
    page.update().await?;

    // Inject CSS reset synchronously to ensure it comes AFTER HTML styles
    // This directly adds to CSSMirror, guaranteeing it's added after HTML styles
    let css_reset = css_reset_text();
    page.inject_css_sync(css_reset)?;

    // Update to process the injected styles
    page.update().await?;

    Ok(())
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

pub fn css_reset_text() -> String {
    String::from("*,*::before,*::after{box-sizing:border-box;margin:0;padding:0;}body,html{font-family:\"Courier New\",Courier,monospace;}html,body{margin:0 !important;padding:0 !important;overflow:hidden;}body{margin:0 !important;}h1,h2,h3,h4,h5,h6,p{margin:0;padding:0;}ul,ol{margin:0;padding:0;list-style:none;}")
}

pub const fn css_reset_injection_script() -> &'static str {
    r#"(function(){
        try {
            var css = "*,*::before,*::after{box-sizing:border-box;margin:0;padding:0;}body,html{font-family:\"Courier New\",Courier,monospace;}html,body{margin:0 !important;padding:0 !important;overflow:hidden;}body{margin:0 !important;}h1,h2,h3,h4,h5,h6,p{margin:0;padding:0;}ul,ol{margin:0;padding:0;list-style:none;}";
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
            var de = document.documentElement; if (de && de.style){ de.style.margin='0'; de.style.padding='0'; }
            var b = document.body; if (b && b.style){ b.style.margin='0'; b.style.padding='0'; }
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
        set_var("RUST_BACKTRACE", "0");
    }

    let _ignore_result =
        LogBuilder::from_env(EnvLoggerEnv::default().filter_or("RUST_LOG", "error"))
            .filter_module("wgpu_hal", LevelFilter::Off)
            .filter_module("wgpu_core", LevelFilter::Off)
            .filter_module("naga", LevelFilter::Off)
            .is_test(false)
            .try_init();
}

// ===== Test cache and unified test runner =====

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
