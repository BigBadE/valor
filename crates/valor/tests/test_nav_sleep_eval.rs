mod chromium_compare;

use anyhow::Result;
use chromium_compare::browser::{setup_chrome_browser, navigate_and_prepare_tab, TestType};

#[test]
fn test_navigate_sleep_single_evaluate() -> Result<()> {
    println!("\n=== Testing navigate + 2s sleep + single evaluate ===");

    let browser = setup_chrome_browser(TestType::Layout)?;
    println!("✓ Browser launched");

    let tab = browser.new_tab()?;
    println!("✓ Tab created");

    // Create test file
    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_sleep_eval.html");
    std::fs::write(&test_file, r#"<!DOCTYPE html>
<html><head><title>Test</title></head>
<body><div id="test">Hello World</div></body></html>"#)?;

    println!("Calling navigate_and_prepare_tab (with 2s sleep inside)...");
    let start = std::time::Instant::now();
    navigate_and_prepare_tab(&tab, &test_file)?;
    println!("✓ navigate_and_prepare_tab completed (took {:?})", start.elapsed());

    // Now do SINGLE evaluation like the layout tests do
    println!("Evaluating layout extraction script...");
    let script = "JSON.stringify({title: document.title, test: document.getElementById('test').textContent})";
    let start = std::time::Instant::now();
    match tab.evaluate(script, true) {
        Ok(result) => {
            println!("✓ Evaluation SUCCESS (took {:?})", start.elapsed());
            println!("  Result: {:?}", result.value);
        }
        Err(e) => {
            println!("✗ Evaluation FAILED: {:?}", e);
            return Err(e.into());
        }
    }

    println!("=== Test PASSED ===\n");
    Ok(())
}
