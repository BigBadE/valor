use headless_chrome::{Browser, LaunchOptionsBuilder};
use std::time::Duration;

#[test]
fn test_minimal_chrome_evaluate() -> anyhow::Result<()> {
    env_logger::init();

    let chrome_path = std::path::PathBuf::from(
        "/root/.local/share/headless-chrome/linux-1095492/chrome-linux/chrome"
    );

    let launch_opts = LaunchOptionsBuilder::default()
        .headless(true)
        .path(Some(chrome_path))
        .sandbox(false)  // Required when running as root
        .args(vec![std::ffi::OsStr::new("--disable-web-security")])  // Allow JS execution on file:// URLs
        .idle_browser_timeout(Duration::from_secs(30))
        .build()?;

    eprintln!("Creating browser...");
    let browser = Browser::new(launch_opts)?;

    eprintln!("Creating tab...");
    let tab = browser.new_tab()?;

    eprintln!("Navigating to file:// URL...");
    tab.navigate_to("file:///home/user/valor/crates/css/modules/display/tests/fixtures/layout/basics/01_display_none.html")?;

    eprintln!("Sleeping 2 seconds...");
    std::thread::sleep(Duration::from_secs(2));

    eprintln!("Evaluating simple expression: 1 + 1...");
    let result = tab.evaluate("1 + 1", false)?;

    eprintln!("Result: {:?}", result);
    eprintln!("SUCCESS!");

    Ok(())
}
