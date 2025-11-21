mod chromium_compare;

use anyhow::Result;
use chromium_compare::browser::{setup_chrome_browser, TestType};
use std::time::Duration;

#[test]
fn test_http_url_navigation() -> Result<()> {
    println!("\n=== Testing HTTP URL navigation ===");

    let browser = setup_chrome_browser(TestType::Layout)?;
    println!("✓ Browser launched");

    let tab = browser.new_tab()?;
    println!("✓ Tab created");

    // Try navigating to a real HTTP URL
    println!("Navigating to http://example.com...");
    tab.navigate_to("http://example.com")?;
    println!("✓ Navigation completed");

    // Small sleep
    std::thread::sleep(Duration::from_millis(500));

    println!("Evaluating JavaScript on http URL...");
    let start = std::time::Instant::now();
    match tab.evaluate("document.title", false) {
        Ok(result) => {
            println!("✓ evaluate() SUCCESS on http URL (took {:?})", start.elapsed());
            println!("  Title: {:?}", result.value);
        }
        Err(e) => {
            println!("✗ evaluate() FAILED on http URL: {:?}", e);
            return Err(e.into());
        }
    }

    println!("\n=== HTTP URL test PASSED ===\n");
    Ok(())
}
