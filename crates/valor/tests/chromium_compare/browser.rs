use anyhow::Result;
use chromiumoxide::browser::Browser;
use chromiumoxide::page::Page;
use futures::StreamExt as _;
use std::path::Path;
use tokio::spawn;
use tokio::task::JoinHandle;

use super::common::{css_reset_injection_script, to_file_url};

/// Hardcoded port for the shared Chrome instance
const CHROME_PORT: u16 = 19222;

/// Browser with its handler task
pub struct BrowserWithHandler {
    pub browser: Browser,
    _handler_task: JoinHandle<()>,
}

/// Connects to the shared Chrome instance.
///
/// # Errors
///
/// Returns an error if connection fails.
pub async fn connect_to_chrome_internal() -> Result<BrowserWithHandler> {
    let ws_url = format!("http://localhost:{CHROME_PORT}");

    // Connect to the existing Chrome instance
    let (browser, mut handler) = Browser::connect(&ws_url).await.map_err(|_err| {
        anyhow::anyhow!(
            "Failed to connect to Chrome on {ws_url}\n\n\
            ERROR: You need to run tests with cargo-nextest!\n\
            The nextest setup script starts Chrome instances that tests connect to.\n\n\
            Run: cargo nextest run --workspace --exclude cosmic-text --exclude glyphon\n\
            Or:  ./scripts/verify.sh"
        )
    })?;

    // Spawn the handler task to keep it running
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
