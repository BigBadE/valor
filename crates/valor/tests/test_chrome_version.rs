use anyhow::Result;
use headless_chrome::{Browser, LaunchOptionsBuilder};
use std::time::Duration;

#[test]
fn test_newer_chrome_version() -> Result<()> {
    println!("\n=== Testing with Chrome 120 (linux-1217362) ===");

    // Try the newer Chrome version
    let chrome_path = std::path::PathBuf::from(
        "/root/.local/share/headless-chrome/linux-1217362/chrome"
    );

    let launch_opts = LaunchOptionsBuilder::default()
        .headless(true)
        .path(Some(chrome_path))
        .sandbox(false)
        .build()?;

    let browser = Browser::new(launch_opts)?;
    println!("✓ Browser launched (Chrome 120)");

    let tab = browser.new_tab()?;
    println!("✓ Tab created");

    // Test on about:blank
    println!("\nTesting on about:blank...");
    let result = tab.evaluate("1+1", false)?;
    println!("✓ about:blank works: {:?}", result.value);

    // Create test file
    std::fs::write("/tmp/test_newer.html", "<html><body>Test</body></html>")?;

    // Try navigate
    println!("\nNavigating to file:// with Chrome 120...");
    tab.navigate_to("file:///tmp/test_newer.html")?;
    println!("✓ navigate_to completed");

    std::thread::sleep(Duration::from_secs(2));

    println!("\nEvaluating with Chrome 120...");
    let start = std::time::Instant::now();
    match tab.evaluate("1+1", false) {
        Ok(r) => {
            println!("✓ Chrome 120 WORKS! Result: {:?} (took {:?})", r.value, start.elapsed());
            println!("\n=== Chrome 120 SUCCESS! ===\n");
        }
        Err(e) => {
            println!("✗ Chrome 120 also fails: {:?}", e);
            return Err(e.into());
        }
    }

    Ok(())
}
