// Test Chrome with maximum debug logging to see crash details
use chromiumoxide::browser::{Browser, BrowserConfig};
use std::path::Path;
use std::pin::pin;
use std::time::Duration;

#[tokio::test]
async fn test_chrome_with_debug_logging() -> Result<(), Box<dyn std::error::Error>> {
    // Set up Rust logging
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .is_test(true)
        .try_init();

    let chrome_path = Path::new("/opt/google/chrome/google-chrome");

    // Create config with extensive Chrome logging
    let config = BrowserConfig::builder()
        .chrome_executable(chrome_path)
        .no_sandbox()
        .window_size(800, 600)
        // Enable Chrome's internal logging
        .arg("--enable-logging")
        .arg("--v=1")  // Verbose level 1
        .arg("--log-file=/tmp/chrome_debug.log")
        // Disable shared memory usage to test theory
        .arg("--disable-dev-shm-usage")
        // Try software rendering
        .arg("--disable-gpu")
        .build()?;

    eprintln!("\n=== Launching Chrome with debug logging ===");
    eprintln!("Chrome log will be written to: /tmp/chrome_debug.log");

    let (browser, handler) = Browser::launch(config).await?;

    // Spawn handler task
    let handler_task = tokio::spawn(async move {
        let mut handler = pin!(handler);
        loop {
            futures::future::poll_fn(|cx| {
                use futures::Stream;
                use futures::StreamExt;
                use std::task::Poll;
                match handler.as_mut().poll_next(cx) {
                    Poll::Ready(Some(Ok(_))) => Poll::Ready(()),
                    Poll::Ready(Some(Err(e))) => {
                        eprintln!("[HANDLER ERROR]: {:?}", e);
                        Poll::Ready(())
                    }
                    Poll::Ready(None) => Poll::Ready(()),
                    Poll::Pending => Poll::Pending,
                }
            }).await;
            tokio::task::yield_now().await;
        }
    });

    tokio::time::sleep(Duration::from_millis(500)).await;

    eprintln!("\n=== Creating new page ===");
    let page = browser.new_page("about:blank").await?;

    eprintln!("\n=== Setting content with text ===");
    let html = r#"<!DOCTYPE html><html><body><div>Hello World</div></body></html>"#;

    match page.set_content(html).await {
        Ok(_) => eprintln!("✅ set_content() succeeded"),
        Err(e) => {
            eprintln!("❌ set_content() failed: {}", e);
            handler_task.abort();
            return Ok(());
        }
    }

    eprintln!("\n=== Waiting 2 seconds for Chrome to process ===");
    tokio::time::sleep(Duration::from_secs(2)).await;

    eprintln!("\n=== Attempting evaluate ===");
    let eval_result = tokio::time::timeout(
        Duration::from_secs(5),
        page.evaluate("document.body.textContent")
    ).await;

    match eval_result {
        Ok(Ok(result)) => eprintln!("✅ evaluate() SUCCESS: {:?}", result),
        Ok(Err(e)) => eprintln!("❌ evaluate() error: {}", e),
        Err(_) => eprintln!("⏱️ evaluate() TIMEOUT"),
    }

    handler_task.abort();

    eprintln!("\n=== Test complete, check /tmp/chrome_debug.log for details ===");

    Ok(())
}
