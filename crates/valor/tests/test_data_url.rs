mod chromium_compare;

use anyhow::Result;
use chromium_compare::browser::{setup_chrome_browser, TestType};
use std::time::Duration;

#[test]
fn test_data_url_navigation() -> Result<()> {
    println!("\n=== Testing data: URL navigation ===");

    let browser = setup_chrome_browser(TestType::Layout)?;
    println!("✓ Browser launched");

    let tab = browser.new_tab()?;
    println!("✓ Tab created");

    // Try navigating to a data: URL instead of file:// URL
    let html_content = r#"<!DOCTYPE html>
<html><head><title>Data URL Test</title></head>
<body><div id="test">Hello from data URL!</div></body></html>"#;

    let data_url = format!("data:text/html,{}", urlencoding::encode(html_content));
    println!("Navigating to data: URL...");
    tab.navigate_to(&data_url)?;
    println!("✓ Navigation completed");

    // Small sleep to let page stabilize
    std::thread::sleep(Duration::from_millis(100));

    println!("Evaluating JavaScript...");
    let result = tab.evaluate("document.title", false)?;
    println!("✓ Title: {:?}", result.value);

    println!("Evaluating DOM query...");
    let result2 = tab.evaluate("document.getElementById('test').textContent", false)?;
    println!("✓ Content: {:?}", result2.value);

    println!("Evaluating complex script...");
    let script = r#"JSON.stringify({
        title: document.title,
        test: document.getElementById('test').textContent,
        body: document.body.innerHTML
    })"#;
    let result3 = tab.evaluate(script, true)?;
    println!("✓ JSON: {:?}", result3.value);

    println!("\n=== All data: URL tests PASSED ===\n");
    Ok(())
}
