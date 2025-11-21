use anyhow::Result;
use headless_chrome::{Browser, LaunchOptions};
use headless_chrome::protocol::cdp::Page;
use std::time::Duration;

#[test]
fn test_raw_cdp_navigate() -> Result<()> {
    println!("\n=== Testing with RAW CDP commands ===");

    let chrome_path = std::path::PathBuf::from(
        "/root/.local/share/headless-chrome/linux-1095492/chrome-linux/chrome"
    );
    let launch_opts = headless_chrome::LaunchOptionsBuilder::default()
        .headless(true)
        .path(Some(chrome_path))
        .sandbox(false)
        .build()?;
    let browser = Browser::new(launch_opts)?;
    println!("✓ Browser launched");

    let tab = browser.new_tab()?;
    println!("✓ Tab created");

    // Test on about:blank first
    println!("\nEvaluating on about:blank...");
    let result = tab.evaluate("1+1", false)?;
    println!("✓ about:blank works: {:?}", result.value);

    // Now try using RAW CDP Navigate command instead of navigate_to()
    std::fs::write("/tmp/test_raw.html", "<html><body>Test</body></html>")?;
    let url = "file:///tmp/test_raw.html";

    println!("\nCalling RAW CDP Page.navigate...");
    let nav_result = tab.call_method(Page::Navigate {
        url: url.to_string(),
        referrer: None,
        transition_Type: None,
        frame_id: None,
        referrer_policy: None,
    })?;
    println!("✓ CDP Navigate returned: frame_id={:?}", nav_result.frame_id);

    std::thread::sleep(Duration::from_secs(2));

    println!("\nEvaluating after RAW CDP navigate...");
    let start = std::time::Instant::now();
    match tab.evaluate("1+1", false) {
        Ok(r) => {
            println!("✓ RAW CDP SUCCESS! Result: {:?} (took {:?})", r.value, start.elapsed());
        }
        Err(e) => {
            println!("✗ RAW CDP also fails: {:?}", e);
            return Err(e.into());
        }
    }

    Ok(())
}
