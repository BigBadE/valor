use anyhow::Result;
use headless_chrome::{Browser, LaunchOptionsBuilder, Tab};
use std::ffi::OsStr;
use std::path::Path;
use std::time::Duration;

use super::common::to_file_url;

/// Type of chromium comparison test being run.
#[derive(Debug, Clone, Copy)]
pub enum TestType {
    /// Layout comparison tests.
    Layout,
    /// Graphics/rendering comparison tests.
    Graphics,
}

/// Sets up a headless Chrome browser for comparison testing.
///
/// # Errors
///
/// Returns an error if browser launch fails.
pub fn setup_chrome_browser(test_type: TestType) -> Result<Browser> {
    let (timeout, extra_args): (Duration, Vec<&OsStr>) = match test_type {
        TestType::Layout => (
            Duration::from_secs(300),
            vec![
                OsStr::new("--disable-features=OverlayScrollbar"),
                OsStr::new("--allow-file-access-from-files"),
                OsStr::new("--disable-dev-shm-usage"),
                OsStr::new("--no-sandbox"),
                OsStr::new("--disable-extensions"),
                OsStr::new("--disable-background-networking"),
                OsStr::new("--disable-sync"),
            ],
        ),
        TestType::Graphics => (
            Duration::from_secs(120),
            vec![OsStr::new("--force-color-profile=sRGB")],
        ),
    };

    let mut args = vec![
        OsStr::new("--force-device-scale-factor=1"),
        OsStr::new("--hide-scrollbars"),
        OsStr::new("--blink-settings=imagesEnabled=false"),
        OsStr::new("--disable-gpu"),
    ];
    args.extend(extra_args);

    let launch_opts = LaunchOptionsBuilder::default()
        .headless(true)
        .window_size(Some((800, 600)))
        .idle_browser_timeout(timeout)
        .args(args)
        .build()?;
    Browser::new(launch_opts)
}

/// Navigates a Chrome tab to a fixture and prepares it for testing.
///
/// # Errors
///
/// Returns an error if navigation or script evaluation fails.
pub fn navigate_and_prepare_tab(tab: &Tab, path: &Path) -> Result<()> {
    let url = to_file_url(path)?;
    tab.navigate_to(url.as_str())?;

    // For file:// URLs, wait_until_navigated() doesn't work reliably
    // Instead, poll for document.readyState directly
    let timeout_ms: u128 = 10_000;
    let start = std::time::Instant::now();
    let mut last_error: Option<String> = None;

    loop {
        if start.elapsed().as_millis() > timeout_ms {
            let err_msg = last_error.unwrap_or_else(|| "Unknown error".to_owned());
            return Err(anyhow::anyhow!(
                "Timeout waiting for document ready after {}ms. Last error: {}",
                timeout_ms,
                err_msg
            ));
        }

        match tab.evaluate("document.readyState === 'complete'", false) {
            Ok(eval_result) => {
                if let Some(value) = eval_result.value {
                    if value.as_bool() == Some(true) {
                        return Ok(());
                    }
                }
            }
            Err(e) => {
                last_error = Some(format!("{e:?}"));
            }
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}
