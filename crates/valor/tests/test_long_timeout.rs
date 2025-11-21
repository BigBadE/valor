use anyhow::Result;
use headless_chrome::{Browser, LaunchOptionsBuilder};
use std::time::Duration;

#[test]
fn test_very_long_idle_timeout() -> Result<()> {
    println!("\n=== Testing with VERY LONG idle timeout ===");

    let launch_opts = LaunchOptionsBuilder::default()
        .headless(true)
        .idle_browser_timeout(Duration::from_secs(600)) // 10 minutes
        .build()?;

    let browser = Browser::new(launch_opts)?;
    println!("✓ Browser launched with 600s idle timeout");

    let tab = browser.new_tab()?;
    println!("✓ Tab created");

    // Test on about:blank
    println!("\nTesting on about:blank...");
    let result = tab.evaluate("1+1", false)?;
    println!("✓ about:blank works: {:?}", result.value);

    // Create test file
    std::fs::write("/tmp/test_timeout.html", "<html><body>Test</body></html>")?;

    // Try navigate
    println!("\nNavigating to file://...");
    tab.navigate_to("file:///tmp/test_timeout.html")?;
    println!("✓ navigate_to completed");

    std::thread::sleep(Duration::from_secs(2));

    println!("\nEvaluating (should have 600s timeout)...");
    let start = std::time::Instant::now();
    match tab.evaluate("1+1", false) {
        Ok(r) => {
            println!("✓ WORKED with long timeout! Result: {:?} (took {:?})", r.value, start.elapsed());
        }
        Err(e) => {
            println!("✗ Still failed even with 600s timeout: {:?}", e);
            return Err(e.into());
        }
    }

    Ok(())
}
