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

    let url = to_file_url(path)?;
    log::info!("Navigating to: {}", url.as_str());

    // Add 10 second timeout to navigation
    timeout(Duration::from_secs(10), page.goto(url.as_str()))
        .await
        .map_err(|_| anyhow::anyhow!("Navigation timeout after 10s for {}", url.as_str()))??;

    log::info!("Navigation completed for: {}", url.as_str());
    Ok(())
}

/// Navigates a headless_chrome Tab to a fixture and prepares it for testing.
///
/// # Errors
///
/// Returns an error if navigation fails.
pub fn navigate_and_prepare_tab(tab: &headless_chrome::Tab, path: &Path) -> Result<()> {
    use std::time::Duration;

    let url = to_file_url(path)?;
    log::info!("Navigating tab to: {}", url.as_str());

    tab.navigate_to(url.as_str())?;
    tab.wait_until_navigated()?;

    // Small delay to ensure page is ready
    std::thread::sleep(Duration::from_millis(100));

    log::info!("Tab navigation completed for: {}", url.as_str());
    Ok(())
}
