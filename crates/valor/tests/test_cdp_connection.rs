use anyhow::Result;
use headless_chrome::{Browser, LaunchOptionsBuilder};
use std::ffi::OsStr;
use std::time::Duration;

#[test]
fn test_cdp_connection_stability() -> Result<()> {
    println!("\n=== Testing CDP Connection Stability ===");

    let chrome_path = std::path::PathBuf::from(
        "/root/.local/share/headless-chrome/linux-1095492/chrome-linux/chrome"
    );

    let launch_opts = LaunchOptionsBuilder::default()
        .headless(true)
        .path(Some(chrome_path))
        .sandbox(false)
        .window_size(Some((800, 600)))
        .idle_browser_timeout(Duration::from_secs(60)) // Longer timeout
        .args(vec![
            OsStr::new("--disable-gpu"),
            OsStr::new("--no-sandbox"),
            OsStr::new("--disable-web-security"),
            OsStr::new("--allow-file-access-from-files"),
            OsStr::new("--disable-dev-shm-usage"),
            OsStr::new("--disable-extensions"),
            OsStr::new("--disable-background-networking"),
            OsStr::new("--remote-debugging-port=0"), // Let Chrome choose port
        ])
        .build()?;

    let browser = Browser::new(launch_opts)?;
    println!("✓ Browser launched");

    let tab = browser.new_tab()?;
    println!("✓ Tab created");

    // Test 1: Evaluation before navigation
    println!("\n1. Testing evaluation BEFORE navigation:");
    match tab.evaluate("1+1", false) {
        Ok(r) => println!("  ✓ Success: {:?}", r.value),
        Err(e) => println!("  ✗ Error: {:?}", e),
    }

    // Create test file
    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_cdp_simple.html");
    std::fs::write(&test_file, r#"<!DOCTYPE html>
<html><head><title>Test</title></head><body>Hello</body></html>"#)?;
    let url = format!("file://{}", test_file.display());

    // Test 2: Navigate
    println!("\n2. Navigating to: {}", url);
    match tab.navigate_to(&url) {
        Ok(_) => println!("  ✓ Navigation returned successfully"),
        Err(e) => {
            println!("  ✗ Navigation failed: {:?}", e);
            return Ok(());
        }
    }

    // Small delay
    std::thread::sleep(Duration::from_millis(500));

    // Test 3: Evaluation immediately after navigation
    println!("\n3. Testing evaluation IMMEDIATELY after navigation:");
    match tab.evaluate("1+1", false) {
        Ok(r) => println!("  ✓ Success: {:?}", r.value),
        Err(e) => println!("  ✗ Error: {:?}", e),
    }

    // Test 4: Multiple evaluations
    println!("\n4. Testing multiple evaluations:");
    for i in 1..=3 {
        print!("  Attempt {}: ", i);
        match tab.evaluate("2+2", false) {
            Ok(r) => println!("✓ {:?}", r.value),
            Err(e) => {
                println!("✗ {:?}", e);
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    // Test 5: Try to get page info
    println!("\n5. Testing page info:");
    let url = tab.get_url();
    println!("  ✓ Page URL: {}", url);

    println!("\n=== Test Complete ===");
    Ok(())
}

#[test]
fn test_multiple_tabs_file_urls() -> Result<()> {
    println!("\n=== Testing Multiple Tabs with file:// URLs ===");

    let chrome_path = std::path::PathBuf::from(
        "/root/.local/share/headless-chrome/linux-1095492/chrome-linux/chrome"
    );

    let launch_opts = LaunchOptionsBuilder::default()
        .headless(true)
        .path(Some(chrome_path))
        .sandbox(false)
        .args(vec![
            OsStr::new("--disable-gpu"),
            OsStr::new("--no-sandbox"),
            OsStr::new("--disable-web-security"),
        ])
        .idle_browser_timeout(Duration::from_secs(60))
        .build()?;

    let browser = Browser::new(launch_opts)?;
    println!("✓ Browser launched");

    // Create test file
    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_multi_tab.html");
    std::fs::write(&test_file, r#"<!DOCTYPE html><html><body>Test</body></html>"#)?;
    let url = format!("file://{}", test_file.display());

    // Try creating multiple tabs
    for i in 1..=3 {
        println!("\n--- Tab {} ---", i);
        let tab = browser.new_tab()?;
        println!("✓ Tab created");

        match tab.evaluate("1+1", false) {
            Ok(r) => println!("✓ Eval before nav: {:?}", r.value),
            Err(e) => println!("✗ Eval before nav: {:?}", e),
        }

        match tab.navigate_to(&url) {
            Ok(_) => println!("✓ Navigation succeeded"),
            Err(e) => println!("✗ Navigation failed: {:?}", e),
        }

        std::thread::sleep(Duration::from_millis(200));

        match tab.evaluate("2+2", false) {
            Ok(r) => println!("✓ Eval after nav: {:?}", r.value),
            Err(e) => println!("✗ Eval after nav: {:?}", e),
        }
    }

    println!("\n=== Test Complete ===");
    Ok(())
}
