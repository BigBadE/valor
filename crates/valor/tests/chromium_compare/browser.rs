use anyhow::Result;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::page::Page;
use futures::StreamExt;
use std::path::Path;
use tokio::task::JoinHandle;

use super::common::to_file_url;

/// Type of chromium comparison test being run.
#[derive(Debug, Clone, Copy)]
pub enum TestType {
    /// Layout comparison tests.
    Layout,
    /// Graphics/rendering comparison tests.
    Graphics,
}

/// Browser instance with background event handler.
pub struct ChromeBrowser {
    pub browser: Browser,
    _handler: JoinHandle<()>,
}

impl ChromeBrowser {
    /// Create a new tab/page.
    pub async fn new_page(&self) -> Result<Page> {
        Ok(self.browser.new_page("about:blank").await?)
    }
}

/// Sets up a headless Chrome browser for comparison testing.
///
/// # Errors
///
/// Returns an error if browser launch fails.
pub async fn setup_chrome_browser(_test_type: TestType) -> Result<ChromeBrowser> {
    let chrome_path = std::path::PathBuf::from(
        "/root/.local/share/headless-chrome/linux-1095492/chrome-linux/chrome",
    );

    let config_builder = BrowserConfig::builder()
        .chrome_executable(chrome_path)
        .no_sandbox()
        .window_size(800, 600)
        .arg("--force-device-scale-factor=1")
        .arg("--hide-scrollbars")
        .arg("--blink-settings=imagesEnabled=false")
        .arg("--disable-gpu")
        .arg("--disable-features=OverlayScrollbar")
        .arg("--allow-file-access-from-files")
        .arg("--disable-dev-shm-usage")
        .arg("--disable-extensions")
        .arg("--disable-background-networking")
        .arg("--disable-sync");

    let (browser, mut handler) = Browser::launch(
        config_builder
            .build()
            .map_err(|e| anyhow::anyhow!("Browser config error: {}", e))?,
    )
    .await?;

    // Spawn background handler for Chrome events
    let handler_task = tokio::task::spawn(async move {
        while let Some(event) = handler.next().await {
            if let Err(e) = event {
                eprintln!("Browser event error: {:?}", e);
            }
        }
    });

    Ok(ChromeBrowser {
        browser,
        _handler: handler_task,
    })
}

/// Navigates a Chrome page to a fixture and prepares it for testing.
///
/// # Errors
///
/// Returns an error if navigation fails.
pub async fn navigate_and_prepare_page(page: &Page, path: &Path) -> Result<()> {
    use tokio::time::{Duration, timeout};
    use std::time::Instant;

    let url = to_file_url(path)?;
    log::info!("[TIMING] Starting navigation to: {}", url.as_str());

    // Navigate with timeout
    let nav_start = Instant::now();
    timeout(Duration::from_secs(10), page.goto(url.as_str()))
        .await
        .map_err(|_| anyhow::anyhow!("Navigation timeout after 10s for {}", url.as_str()))??;
    log::info!("[TIMING] Navigation (goto): {:?}", nav_start.elapsed());

    log::info!("Navigation completed, checking page ready state");

    // Poll for document.readyState === 'complete'
    let ready_start = Instant::now();
    let ready_check = async {
        for _ in 0..50 {  // Try up to 50 times (5 seconds with 100ms delay)
            let result = page.evaluate("document.readyState").await?;
            if let Some(state) = result.value().and_then(|v| v.as_str()) {
                if state == "complete" {
                    // Wait a bit more for layout to settle
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Err(anyhow::anyhow!("Page never reached readyState=complete"))
    };

    timeout(Duration::from_secs(10), ready_check)
        .await
        .map_err(|_| anyhow::anyhow!("Page ready timeout after 10s for {}", url.as_str()))??;
    log::info!("[TIMING] Page ready wait: {:?}", ready_start.elapsed());

    log::info!("Page fully loaded for: {}", url.as_str());
    Ok(())
}
