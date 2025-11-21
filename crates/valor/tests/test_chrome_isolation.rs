use anyhow::Result;
use headless_chrome::{Browser, LaunchOptionsBuilder};
use std::ffi::OsStr;
use std::time::{Duration, Instant};

#[test]
fn test_chrome_simple_about_blank() -> Result<()> {
    println!("\n=== TEST 1: Simple evaluation on about:blank ===");

    let chrome_path = std::path::PathBuf::from(
        "/root/.local/share/headless-chrome/linux-1095492/chrome-linux/chrome"
    );

    let launch_opts = LaunchOptionsBuilder::default()
        .headless(true)
        .path(Some(chrome_path))
        .sandbox(false)
        .window_size(Some((800, 600)))
        .idle_browser_timeout(Duration::from_secs(30))
        .args(vec![
            OsStr::new("--disable-gpu"),
            OsStr::new("--no-sandbox"),
        ])
        .build()?;

    let browser = Browser::new(launch_opts)?;
    println!("✓ Browser launched");

    let tab = browser.new_tab()?;
    println!("✓ Tab created (about:blank)");

    // Test 1: Simple arithmetic
    let start = Instant::now();
    println!("Evaluating: 1+1");
    let result = tab.evaluate("1+1", true)?;
    println!("✓ Result: {:?} (took {:?})", result.value, start.elapsed());

    // Test 2: String return
    let start = Instant::now();
    println!("Evaluating: 'hello'");
    let result = tab.evaluate("'hello'", true)?;
    println!("✓ Result: {:?} (took {:?})", result.value, start.elapsed());

    // Test 3: document.title
    let start = Instant::now();
    println!("Evaluating: document.title");
    let result = tab.evaluate("document.title", true)?;
    println!("✓ Result: {:?} (took {:?})", result.value, start.elapsed());

    println!("=== TEST 1 PASSED ===\n");
    Ok(())
}

#[test]
fn test_chrome_navigate_to_file() -> Result<()> {
    println!("\n=== TEST 2: Navigate to file:// URL ===");

    // Create a simple HTML file
    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_chrome_simple.html");
    std::fs::write(&test_file, r#"<!DOCTYPE html>
<html>
<head><title>Test Page</title></head>
<body>
    <div id="test">Hello World</div>
</body>
</html>"#)?;
    println!("✓ Created test file: {}", test_file.display());

    let chrome_path = std::path::PathBuf::from(
        "/root/.local/share/headless-chrome/linux-1095492/chrome-linux/chrome"
    );

    let launch_opts = LaunchOptionsBuilder::default()
        .headless(true)
        .path(Some(chrome_path))
        .sandbox(false)
        .window_size(Some((800, 600)))
        .idle_browser_timeout(Duration::from_secs(30))
        .args(vec![
            OsStr::new("--disable-gpu"),
            OsStr::new("--no-sandbox"),
            OsStr::new("--disable-web-security"),
            OsStr::new("--allow-file-access-from-files"),
        ])
        .build()?;

    let browser = Browser::new(launch_opts)?;
    println!("✓ Browser launched");

    let tab = browser.new_tab()?;
    println!("✓ Tab created");

    let url = format!("file://{}", test_file.display());
    println!("Navigating to: {}", url);
    let start = Instant::now();
    tab.navigate_to(&url)?;
    println!("✓ Navigation complete (took {:?})", start.elapsed());

    // Wait for page to be ready
    println!("Waiting for document.readyState === 'complete'");
    let start = Instant::now();
    for attempt in 1..=10 {
        match tab.evaluate("document.readyState === 'complete'", false) {
            Ok(result) => {
                if result.value.as_ref().and_then(|v| v.as_bool()) == Some(true) {
                    println!("✓ Document ready (attempt {}, took {:?})", attempt, start.elapsed());
                    break;
                }
            }
            Err(e) => {
                println!("  Attempt {}: Error: {:?}", attempt, e);
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    // Test simple evaluation after navigation
    println!("Evaluating: 1+1");
    let start = Instant::now();
    let result = tab.evaluate("1+1", true)?;
    println!("✓ Result: {:?} (took {:?})", result.value, start.elapsed());

    // Test DOM access
    println!("Evaluating: document.getElementById('test').textContent");
    let start = Instant::now();
    let result = tab.evaluate("document.getElementById('test').textContent", true)?;
    println!("✓ Result: {:?} (took {:?})", result.value, start.elapsed());

    println!("=== TEST 2 PASSED ===\n");
    Ok(())
}

#[test]
fn test_chrome_complex_script() -> Result<()> {
    println!("\n=== TEST 3: Complex script on file:// URL ===");

    // Create a simple HTML file
    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("test_chrome_complex.html");
    std::fs::write(&test_file, r#"<!DOCTYPE html>
<html>
<head><title>Test Page</title></head>
<body>
    <div id="test" style="width: 100px; height: 50px;">Hello World</div>
</body>
</html>"#)?;
    println!("✓ Created test file: {}", test_file.display());

    let chrome_path = std::path::PathBuf::from(
        "/root/.local/share/headless-chrome/linux-1095492/chrome-linux/chrome"
    );

    let launch_opts = LaunchOptionsBuilder::default()
        .headless(true)
        .path(Some(chrome_path))
        .sandbox(false)
        .window_size(Some((800, 600)))
        .idle_browser_timeout(Duration::from_secs(30))
        .args(vec![
            OsStr::new("--disable-gpu"),
            OsStr::new("--no-sandbox"),
            OsStr::new("--disable-web-security"),
            OsStr::new("--allow-file-access-from-files"),
        ])
        .build()?;

    let browser = Browser::new(launch_opts)?;
    let tab = browser.new_tab()?;
    let url = format!("file://{}", test_file.display());
    tab.navigate_to(&url)?;

    // Wait for ready
    for _ in 1..=10 {
        if let Ok(result) = tab.evaluate("document.readyState === 'complete'", false) {
            if result.value.as_ref().and_then(|v| v.as_bool()) == Some(true) {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    // Test complex script (simplified layout extraction)
    let script = r#"(function() {
        var el = document.getElementById('test');
        var rect = el.getBoundingClientRect();
        var style = window.getComputedStyle(el);
        return JSON.stringify({
            tag: el.tagName.toLowerCase(),
            text: el.textContent,
            rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
            style: { width: style.width, height: style.height }
        });
    })()"#;

    println!("Evaluating complex script...");
    let start = Instant::now();
    let result = tab.evaluate(script, true)?;
    println!("✓ Result: {:?} (took {:?})", result.value, start.elapsed());

    println!("=== TEST 3 PASSED ===\n");
    Ok(())
}
