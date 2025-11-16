use headless_chrome::{Browser, LaunchOptionsBuilder};
use std::time::Duration;

#[test]
fn test_file_url_with_sleep() -> anyhow::Result<()> {
    env_logger::init();

    let chrome_path = std::path::PathBuf::from(
        "/root/.local/share/headless-chrome/linux-1095492/chrome-linux/chrome"
    );

    let launch_opts = LaunchOptionsBuilder::default()
        .headless(true)
        .path(Some(chrome_path))
        .sandbox(false)
        .window_size(Some((800, 600)))
        .idle_browser_timeout(Duration::from_secs(300))
        .args(vec![
            std::ffi::OsStr::new("--force-device-scale-factor=1"),
            std::ffi::OsStr::new("--hide-scrollbars"),
            std::ffi::OsStr::new("--blink-settings=imagesEnabled=false"),
            std::ffi::OsStr::new("--disable-gpu"),
        ])
        .build()?;

    eprintln!("Creating browser...");
    let browser = Browser::new(launch_opts)?;

    eprintln!("Creating tab...");
    let tab = browser.new_tab()?;

    eprintln!("Navigating to file:// URL...");
    let file_path = "/home/user/valor/crates/css/modules/display/tests/fixtures/layout/basics/01_display_none.html";
    let url = format!("file://{}", file_path);
    tab.navigate_to(&url)?;

    eprintln!("Sleeping for 2 seconds (mimicking original code)...");
    std::thread::sleep(Duration::from_secs(2));

    eprintln!("Evaluating JavaScript...");
    let script = r#"(function() {
        var root = document.body || document.documentElement;
        return root ? root.tagName : 'NO_ROOT';
    })()"#;

    let result = tab.evaluate(script, true)?;
    eprintln!("Result: {:?}", result);

    eprintln!("SUCCESS!");
    Ok(())
}
