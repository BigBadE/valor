// Test to systematically identify which HTML patterns cause Chrome SEGFAULT
use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;
use std::path::Path;
use std::pin::pin;
use std::time::Duration;

#[tokio::test]
async fn identify_crash_pattern() -> Result<(), Box<dyn std::error::Error>> {
    // Test cases from simple to complex
    let test_cases = vec![
        ("Empty", ""),
        ("Minimal", "<!DOCTYPE html><html><body></body></html>"),
        ("With div", "<!DOCTYPE html><html><body><div></div></body></html>"),
        ("With text", "<!DOCTYPE html><html><body><div>Hello</div></body></html>"),
        ("Basic style", r#"<!DOCTYPE html><html><head><style>div { display: block; }</style></head><body><div>Test</div></body></html>"#),
        ("With margin", r#"<!DOCTYPE html><html><head><style>div { margin: 10px; }</style></head><body><div>Test</div></body></html>"#),
        ("With padding", r#"<!DOCTYPE html><html><head><style>div { padding: 10px; }</style></head><body><div>Test</div></body></html>"#),
        ("With border", r#"<!DOCTYPE html><html><head><style>div { border: 1px solid black; }</style></head><body><div>Test</div></body></html>"#),
        ("Box model", r#"<!DOCTYPE html><html><head><style>div { margin: 10px; padding: 10px; border: 1px solid black; }</style></head><body><div>Test</div></body></html>"#),
        ("Nested divs", r#"<!DOCTYPE html><html><body><div><div><div>Test</div></div></div></body></html>"#),
        ("Multiple children", r#"<!DOCTYPE html><html><body><div><div>A</div><div>B</div><div>C</div></div></body></html>"#),
        ("Flexbox", r#"<!DOCTYPE html><html><head><style>div { display: flex; }</style></head><body><div><span>Test</span></div></body></html>"#),
        ("Grid", r#"<!DOCTYPE html><html><head><style>div { display: grid; }</style></head><body><div><span>Test</span></div></body></html>"#),
        ("Absolute pos", r#"<!DOCTYPE html><html><head><style>div { position: absolute; top: 10px; left: 10px; }</style></head><body><div>Test</div></body></html>"#),
        ("Fixture-like", r#"<!DOCTYPE html><html><head><style>body { margin: 0; padding: 10px; } div { display: block; width: 100px; }</style></head><body><div id="test">Hello</div></body></html>"#),
    ];

    let chrome_path = Path::new("/opt/google/chrome/google-chrome");

    let config = BrowserConfig::builder()
        .chrome_executable(chrome_path)
        .no_sandbox()
        .window_size(800, 600)
        .build()?;

    let (browser, mut handler) = Browser::launch(config).await?;

    // Spawn handler task
    tokio::spawn(async move {
        let mut handler = pin!(handler);
        loop {
            futures::future::poll_fn(|cx| {
                use futures::Stream;
                use std::task::Poll;
                match handler.as_mut().poll_next(cx) {
                    Poll::Ready(Some(Ok(_))) => Poll::Ready(()),
                    Poll::Ready(Some(Err(e))) => {
                        eprintln!("[HANDLER] Error: {:?}", e);
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

    for (name, html) in test_cases {
        eprintln!("\n========================================");
        eprintln!("Testing: {}", name);
        eprintln!("========================================");

        let page = match browser.new_page("about:blank").await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("❌ FAILED to create page: {}", e);
                continue;
            }
        };

        // Set content
        match page.set_content(html).await {
            Ok(_) => eprintln!("✅ set_content() succeeded"),
            Err(e) => {
                eprintln!("❌ set_content() FAILED: {}", e);
                let _ = page.close().await;
                continue;
            }
        }

        // Wait for potential crash
        tokio::time::sleep(Duration::from_millis(1000)).await;

        // Try simple evaluation
        let eval_result = tokio::time::timeout(
            Duration::from_secs(3),
            page.evaluate("1 + 1")
        ).await;

        match eval_result {
            Ok(Ok(_)) => eprintln!("✅ evaluate() succeeded - NO CRASH"),
            Ok(Err(e)) => eprintln!("❌ evaluate() failed: {}", e),
            Err(_) => eprintln!("⏱️ evaluate() TIMEOUT - likely crashed"),
        }

        let _ = page.close().await;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    eprintln!("\n========================================");
    eprintln!("Pattern test complete!");
    eprintln!("========================================");

    Ok(())
}
