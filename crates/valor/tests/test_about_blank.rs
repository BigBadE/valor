mod chromium_compare;

use anyhow::Result;
use chromium_compare::browser::{setup_chrome_browser, TestType};

#[test]
fn test_about_blank_only() -> Result<()> {
    println!("\n=== Testing about:blank ONLY (no file:// navigation) ===");

    let browser = setup_chrome_browser(TestType::Layout)?;
    println!("✓ Browser launched");

    let tab = browser.new_tab()?;
    println!("✓ Tab created (defaults to about:blank)");

    // Don't navigate anywhere - tab starts on about:blank
    println!("Evaluating on about:blank...");
    let result = tab.evaluate("1+1", false)?;
    println!("✓ Result: {:?}", result.value);

    println!("Evaluating again...");
    let result2 = tab.evaluate("document.title", false)?;
    println!("✓ Result: {:?}", result2.value);

    println!("Third evaluation...");
    let result3 = tab.evaluate("2*3", false)?;
    println!("✓ Result: {:?}", result3.value);

    println!("\n=== All evaluations PASSED ===\n");
    Ok(())
}
