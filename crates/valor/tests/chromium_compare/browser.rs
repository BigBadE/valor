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

    // Navigate with timeout
    timeout(Duration::from_secs(10), page.goto(url.as_str()))
        .await
        .map_err(|_| anyhow::anyhow!("Navigation timeout after 10s for {}", url.as_str()))??;

    log::info!("Navigation completed, waiting for page to be fully ready");

    // Wait for document to be fully loaded and layout to be stable
    let ready_script = r#"
        (function() {
            return new Promise((resolve) => {
                if (document.readyState === 'complete') {
                    // Give a moment for layout to settle after load event
                    setTimeout(resolve, 100);
                } else {
                    window.addEventListener('load', () => {
                        setTimeout(resolve, 100);
                    });
                }
            });
        })()
    "#;

    timeout(Duration::from_secs(5), page.evaluate(ready_script))
        .await
        .map_err(|_| anyhow::anyhow!("Page ready timeout after 5s for {}", url.as_str()))??;

    log::info!("Page fully loaded for: {}", url.as_str());
    Ok(())
}
