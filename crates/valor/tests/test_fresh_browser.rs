mod chromium_compare;

use anyhow::Result;
use chromium_compare::browser::{setup_chrome_browser, TestType};
use chromium_compare::common::to_file_url;
use std::time::Duration;

#[test]
fn test_fresh_browser_each_eval() -> Result<()> {
    println!("\n=== Testing with FRESH browser (no reuse) ===");

    // Create test file
    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_fresh.html");
    std::fs::write(&test_file, r#"<!DOCTYPE html>
<html><head><title>Test</title></head>
<body><div id="test">Hello</div></body></html>"#)?;

    // Test 1: Fresh browser, fresh tab
    println!("\nTest 1: Fresh browser + tab");
    {
        let browser = setup_chrome_browser(TestType::Layout)?;
        println!("  ✓ Browser 1 launched");

        let tab = browser.new_tab()?;
        println!("  ✓ Tab created");

        let url = to_file_url(&test_file)?;
        tab.navigate_to(url.as_str())?;
        println!("  ✓ Navigated");

        std::thread::sleep(Duration::from_secs(2));
        println!("  ✓ Slept 2s");

        let result = tab.evaluate("1+1", false)?;
        println!("  ✓ Evaluated: {:?}", result.value);
    } // Browser drops here

    println!("\nTest 2: Another fresh browser + tab");
    {
        let browser = setup_chrome_browser(TestType::Layout)?;
        println!("  ✓ Browser 2 launched");

        let tab = browser.new_tab()?;
        println!("  ✓ Tab created");

        let url = to_file_url(&test_file)?;
        tab.navigate_to(url.as_str())?;
        println!("  ✓ Navigated");

        std::thread::sleep(Duration::from_secs(2));
        println!("  ✓ Slept 2s");

        let result = tab.evaluate("document.title", false)?;
        println!("  ✓ Evaluated: {:?}", result.value);
    }

    println!("\n=== Both tests PASSED ===\n");
    Ok(())
}
