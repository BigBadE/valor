use anyhow::{Result, anyhow};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use chromiumoxide::browser::Browser;
use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
use chromiumoxide::cdp::browser_protocol::page::{
    CaptureScreenshotFormat, CaptureScreenshotParams,
};
use chromiumoxide::page::Page;
use futures::StreamExt as _;
use image::{RgbaImage, load_from_memory};
use std::env;
use std::fs::{create_dir_all, remove_dir_all};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};
use tokio::spawn;
use tokio::task::JoinHandle;
use tokio::time::sleep;

use super::common::{css_reset_injection_script, to_file_url};

/// Hardcoded port for the Chrome instance
const CHROME_PORT: u16 = 19222;

/// Browser with its handler task and Chrome process handle
pub struct BrowserWithHandler {
    pub browser: Browser,
    _handler_task: JoinHandle<()>,
    chrome_process: Option<Child>,
    user_data_dir: Option<PathBuf>,
}

impl Drop for BrowserWithHandler {
    fn drop(&mut self) {
        if let Some(mut process) = self.chrome_process.take() {
            let _ignore_result = process.kill();
        }
        if let Some(dir) = self.user_data_dir.take() {
            let _ignore_result = remove_dir_all(&dir);
        }
    }
}

/// Finds the Chrome executable on the system.
///
/// # Errors
///
/// Returns an error if Chrome cannot be found.
fn find_chrome_executable() -> Result<PathBuf> {
    let candidates = vec![
        "google-chrome",
        "chromium",
        "chromium-browser",
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        "/c/Program Files/Google/Chrome/Application/chrome.exe",
        "/mnt/c/Program Files/Google/Chrome/Application/chrome.exe",
    ];

    for candidate in candidates {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Ok(path);
        }
        if Command::new(candidate).arg("--version").output().is_ok() {
            return Ok(PathBuf::from(candidate));
        }
    }

    Err(anyhow!(
        "Chrome/Chromium executable not found. Please install Chrome or set CHROME_BIN environment variable."
    ))
}

/// Checks if Chrome is already running on the specified port.
fn is_chrome_running(port: u16) -> bool {
    TcpStream::connect(format!("127.0.0.1:{port}")).is_ok()
}

/// Kills any existing Chrome processes on the specified port.
///
/// # Errors
///
/// Returns an error if process termination fails.
async fn kill_existing_chrome(port: u16) -> Result<()> {
    if is_chrome_running(port) {
        log::info!("Found existing Chrome on port {port}, attempting graceful shutdown");
        sleep(Duration::from_millis(500)).await;
    }

    if cfg!(target_os = "windows") {
        let _ignore_result = Command::new("taskkill")
            .args(["/F", "/IM", "chrome.exe"])
            .output();
    } else {
        let _ignore_result = Command::new("pkill")
            .args(["-f", &format!("remote-debugging-port={port}")])
            .output();
    }

    sleep(Duration::from_secs(1)).await;
    Ok(())
}

/// Starts a Chrome instance in headless mode.
///
/// # Errors
///
/// Returns an error if Chrome fails to start or cannot be found.
async fn start_chrome_process() -> Result<(Child, PathBuf)> {
    let chrome_bin = find_chrome_executable()?;

    let workspace_root =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()))
            .join("..")
            .join("..");
    let target_dir = workspace_root.join("target");
    create_dir_all(&target_dir)?;

    let user_data_dir = target_dir.join("chrome_test_data");
    if user_data_dir.exists() {
        let _ignore_result = remove_dir_all(&user_data_dir);
    }
    create_dir_all(&user_data_dir)?;

    let chrome_args = vec![
        format!("--remote-debugging-port={CHROME_PORT}"),
        format!("--user-data-dir={}", user_data_dir.display()),
        "--headless=new".to_string(),
        "--disable-gpu".to_string(),
        "--no-sandbox".to_string(),
        "--disable-dev-shm-usage".to_string(),
        "--disable-extensions".to_string(),
        "--disable-background-networking".to_string(),
        "--disable-sync".to_string(),
        "--force-device-scale-factor=1".to_string(),
        "--hide-scrollbars".to_string(),
        "--blink-settings=imagesEnabled=false".to_string(),
        "--disable-features=OverlayScrollbar".to_string(),
        "--allow-file-access-from-files".to_string(),
        "--force-color-profile=sRGB".to_string(),
        "--window-size=800,600".to_string(),
    ];

    log::info!(
        "Starting Chrome: {} {:?}",
        chrome_bin.display(),
        chrome_args
    );

    let mut process = Command::new(&chrome_bin)
        .args(&chrome_args)
        .spawn()
        .map_err(|err| anyhow!("Failed to start Chrome: {err}"))?;

    let max_wait = Duration::from_secs(10);
    let start = Instant::now();

    while start.elapsed() < max_wait {
        if is_chrome_running(CHROME_PORT) {
            log::info!("Chrome started successfully on port {CHROME_PORT}");
            return Ok((process, user_data_dir));
        }

        if let Ok(Some(status)) = process.try_wait() {
            return Err(anyhow!(
                "Chrome process exited unexpectedly with status: {status}"
            ));
        }

        sleep(Duration::from_millis(100)).await;
    }

    let _ignore_result = process.kill();
    Err(anyhow!("Chrome failed to start within {max_wait:?}"))
}

/// Starts Chrome and connects to it.
///
/// # Errors
///
/// Returns an error if Chrome fails to start or connection fails.
pub async fn start_and_connect_chrome() -> Result<BrowserWithHandler> {
    kill_existing_chrome(CHROME_PORT).await?;

    let (chrome_process, user_data_dir) = start_chrome_process().await?;

    let ws_url = format!("http://localhost:{CHROME_PORT}");

    let (browser, mut handler) = Browser::connect(&ws_url)
        .await
        .map_err(|err| anyhow!("Failed to connect to Chrome on {ws_url}: {err}"))?;

    let handler_task = spawn(async move {
        while let Some(event) = handler.next().await {
            match event {
                Ok(()) => {}
                Err(err) => {
                    log::debug!("Browser handler error: {err}");
                }
            }
        }
    });

    Ok(BrowserWithHandler {
        browser,
        _handler_task: handler_task,
        chrome_process: Some(chrome_process),
        user_data_dir: Some(user_data_dir),
    })
}

/// Navigates a Chrome page to a fixture and prepares it for testing.
///
/// # Errors
///
/// Returns an error if navigation or script evaluation fails.
pub async fn navigate_and_prepare_page(page: &Page, path: &Path) -> Result<()> {
    let url = to_file_url(path)?;
    page.goto(url.as_str()).await?;
    page.evaluate(css_reset_injection_script()).await?;
    Ok(())
}

/// Captures a PNG screenshot from Chrome for a given fixture.
///
/// # Errors
///
/// Returns an error if navigation or screenshot capture fails.
pub async fn capture_screenshot_png(
    page: &Page,
    path: &Path,
    width: u32,
    height: u32,
) -> Result<Vec<u8>> {
    navigate_and_prepare_page(page, path).await?;

    let viewport_params = SetDeviceMetricsOverrideParams::builder()
        .width(i64::from(width))
        .height(i64::from(height))
        .device_scale_factor(1.0)
        .mobile(false)
        .build()
        .map_err(|err| anyhow!("Failed to build viewport params: {err}"))?;
    page.execute(viewport_params).await?;

    let params = CaptureScreenshotParams::builder()
        .format(CaptureScreenshotFormat::Png)
        .from_surface(true)
        .build();
    let response = page.execute(params).await?;
    let base64_str: &str = response.data.as_ref();
    let bytes = BASE64_STANDARD
        .decode(base64_str)
        .map_err(|err| anyhow!("Failed to decode base64 screenshot: {err}"))?;
    Ok(bytes)
}

/// Captures a Chrome screenshot and decodes it to RGBA.
///
/// # Errors
///
/// Returns an error if screenshot capture or image decoding fails.
pub async fn capture_screenshot_rgba(
    page: &Page,
    path: &Path,
    width: u32,
    height: u32,
) -> Result<RgbaImage> {
    let png_bytes = capture_screenshot_png(page, path, width, height).await?;
    let img = load_from_memory(&png_bytes)?.to_rgba8();
    Ok(img)
}
