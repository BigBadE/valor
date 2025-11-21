use anyhow::Result;
use headless_chrome::{Browser, LaunchOptionsBuilder};
use std::ffi::OsStr;
use std::time::Duration;

#[test]
fn test_navigate_and_immediate_eval() -> Result<()> {
    println!("\n=== Testing navigate + immediate evaluate ===");

    let chrome_path = std::path::PathBuf::from(
        "/root/.local/share/headless-chrome/linux-1095492/chrome-linux/chrome"
    );

    let launch_opts = LaunchOptionsBuilder::default()
        .headless(true)
        .path(Some(chrome_path))
        .sandbox(false)
        .window_size(Some((800, 600)))
        .idle_browser_timeout(Duration::from_secs(60))
        .args(vec![
            OsStr::new("--disable-gpu"),
            OsStr::new("--no-sandbox"),
            OsStr::new("--disable-features=OverlayScrollbar"),
            OsStr::new("--allow-file-access-from-files"),
            OsStr::new("--disable-dev-shm-usage"),
            OsStr::new("--disable-extensions"),
            OsStr::new("--disable-background-networking"),
            OsStr::new("--disable-sync"),
            OsStr::new("--remote-debugging-port=0"),  // Let Chrome pick port (but ensure debugging is on)
        ])
        .build()?;

    let browser = Browser::new(launch_opts)?;
    println!("✓ Browser launched");

    let tab = browser.new_tab()?;
    println!("✓ Tab created");

    // Create test file
    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_immediate.html");
    std::fs::write(&test_file, r#"<!DOCTYPE html>
<html><head><title>Test</title></head><body>Hello</body></html>"#)?;
    let url = format!("file://{}", test_file.display());

    println!("Navigating to: {}", url);
    tab.navigate_to(&url)?;
    println!("✓ Navigation completed");

    // Immediate evaluation without any wait
    println!("Evaluating immediately after navigate_to...");
    match tab.evaluate("1+1", false) {
        Ok(r) => println!("✓ Success: {:?}", r.value),
        Err(e) => {
            println!("✗ Error: {:?}", e);
            return Err(e.into());
        }
    }

    // Small delay before second evaluation
    std::thread::sleep(Duration::from_millis(100));

    // Try CSS injection
    let css_script = r#"(function(){
        var css = "*{margin:0;padding:0;}";
        var style = document.createElement('style');
        style.textContent = css;
        document.head.appendChild(style);
        return true;
    })();"#;

    println!("Injecting CSS...");
    match tab.evaluate(css_script, false) {
        Ok(r) => println!("✓ CSS injection success: {:?}", r.value),
        Err(e) => {
            println!("✗ CSS injection error: {:?}", e);
            return Err(e.into());
        }
    }

    println!("=== Test Complete ===\n");
    Ok(())
}
