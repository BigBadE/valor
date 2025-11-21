mod chromium_compare;

use anyhow::Result;
use chromium_compare::browser::{setup_chrome_browser, TestType};
use std::time::Duration;

#[test]
fn test_wait_for_element_after_navigate() -> Result<()> {
    println!("\n=== Testing wait_for_element instead of evaluate ===");

    let browser = setup_chrome_browser(TestType::Layout)?;
    println!("✓ Browser launched");

    let tab = browser.new_tab()?;
    println!("✓ Tab created");

    // Create HTML with a specific element
    std::fs::write("/tmp/test_elem.html", r#"<!DOCTYPE html>
<html><head><title>Test</title></head>
<body><div id="test">Hello World</div></body></html>"#)?;

    println!("\nNavigating to file://...");
    tab.navigate_to("file:///tmp/test_elem.html")?;
    println!("✓ navigate_to completed");

    std::thread::sleep(Duration::from_secs(2));

    println!("\nTrying wait_for_element instead of evaluate...");
    let start = std::time::Instant::now();
    match tab.wait_for_element("#test") {
        Ok(elem) => {
            println!("✓ wait_for_element WORKED! (took {:?})", start.elapsed());

            // Try to get the element's text
            match elem.get_description() {
                Ok(desc) => println!("  Element: {:?}", desc),
                Err(e) => println!("  get_description failed: {:?}", e),
            }
        }
        Err(e) => {
            println!("✗ wait_for_element failed: {:?}", e);
            return Err(e.into());
        }
    }

    println!("\n=== wait_for_element test COMPLETE ===\n");
    Ok(())
}
