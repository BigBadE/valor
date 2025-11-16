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
                OsStr::new("--disable-web-security"),  // Allow JavaScript execution on file:// URLs
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

    let chrome_path = std::path::PathBuf::from(
        "/root/.local/share/headless-chrome/linux-1095492/chrome-linux/chrome"
    );

    let launch_opts = LaunchOptionsBuilder::default()
        .headless(true)
        .path(Some(chrome_path))
        .sandbox(false)  // Required when running as root
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

    // For file:// URLs, both wait_until_navigated() and tab.evaluate() are unreliable.
    // Use a longer sleep to ensure page is fully loaded before JavaScript execution.
    std::thread::sleep(Duration::from_secs(2));

    Ok(())
}
