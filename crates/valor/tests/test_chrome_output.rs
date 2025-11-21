use anyhow::Result;
use headless_chrome::{Browser, LaunchOptions};
use std::time::Duration;
use std::process::Command;

#[test]
fn test_check_chrome_running() -> Result<()> {
    println!("\n=== Launching Chrome and checking process ===");

    let browser = Browser::new(LaunchOptions::default())?;
    println!("✓ Browser launched");

    // Give it a moment
    std::thread::sleep(Duration::from_millis(500));

    // Check Chrome processes
    println!("\nChrome processes running:");
    let output = Command::new("ps")
        .args(&["aux"])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("chrome") && !line.contains("grep") {
            println!("  {}", line);
        }
    }

    let tab = browser.new_tab()?;
    println!("\n✓ Tab created");

    // Test on about:blank first
    println!("\nTesting evaluate on about:blank...");
    let result = tab.evaluate("1+1", false)?;
    println!("✓ about:blank eval worked: {:?}", result.value);

    // Now try navigate
    println!("\nNavigating to file:///tmp/test.html...");
    std::fs::write("/tmp/test.html", "<html><body>Test</body></html>")?;

    let start = std::time::Instant::now();
    tab.navigate_to("file:///tmp/test.html")?;
    println!("✓ navigate_to returned after {:?}", start.elapsed());

    println!("\nWaiting 1 second...");
    std::thread::sleep(Duration::from_secs(1));

    println!("\nAttempting evaluate() with 10s timeout...");
    println!("(This will likely hang - press Ctrl+C after 10s if it does)");

    let start = std::time::Instant::now();
    match tab.evaluate("document.body.innerHTML", false) {
        Ok(r) => println!("✓ evaluate worked after {:?}: {:?}", start.elapsed(), r.value),
        Err(e) => println!("✗ evaluate failed after {:?}: {:?}", start.elapsed(), e),
    }

    Ok(())
}
