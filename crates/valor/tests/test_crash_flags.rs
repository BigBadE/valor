// Test different Chrome flags to find workaround for text rendering crash
use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;
use std::path::Path;
use std::pin::pin;
use std::time::Duration;

#[tokio::test]
async fn test_flags_for_crash_workaround() -> Result<(), Box<dyn std::error::Error>> {
    let chrome_path = Path::new("/opt/google/chrome/google-chrome");

    // HTML that triggers the crash (contains text)
    let crash_html = r#"<!DOCTYPE html><html><body><div>Hello World</div></body></html>"#;

    // Different flag combinations to test
    let flag_tests = vec![
        ("Baseline (no special flags)", vec![]),
        ("Disable GPU", vec!["--disable-gpu"]),
        ("Disable software rasterizer", vec!["--disable-software-rasterizer"]),
        ("Disable font subpixel", vec!["--disable-font-subpixel-positioning"]),
        ("Disable LCD text", vec!["--disable-lcd-text"]),
        ("Use SwiftShader", vec!["--use-gl=swiftshader"]),
        ("Disable accelerated 2D canvas", vec!["--disable-accelerated-2d-canvas"]),
        ("Disable GPU compositing", vec!["--disable-gpu-compositing"]),
        ("Combined text flags", vec!["--disable-font-subpixel-positioning", "--disable-lcd-text"]),
        ("All rendering flags", vec!["--disable-gpu", "--disable-software-rasterizer", "--disable-accelerated-2d-canvas"]),
    ];

    for (name, extra_flags) in flag_tests {
        eprintln!("\n========================================");
        eprintln!("Testing: {}", name);
        eprintln!("Flags: {:?}", extra_flags);
        eprintln!("========================================");

        let mut config_builder = BrowserConfig::builder()
            .chrome_executable(chrome_path)
            .no_sandbox()
            .window_size(800, 600);

        for flag in &extra_flags {
            config_builder = config_builder.arg(*flag);
        }

        let config = config_builder.build()?;

        let (browser, mut handler) = match Browser::launch(config).await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("❌ Browser launch FAILED: {}", e);
                continue;
            }
        };

        // Spawn handler task
        tokio::spawn(async move {
            let mut handler = pin!(handler);
            loop {
                futures::future::poll_fn(|cx| {
                    use futures::Stream;
                    use std::task::Poll;
                    match handler.as_mut().poll_next(cx) {
                        Poll::Ready(Some(Ok(_))) => Poll::Ready(()),
                        Poll::Ready(Some(Err(_))) => Poll::Ready(()),
                        Poll::Ready(None) => Poll::Ready(()),
                        Poll::Pending => Poll::Pending,
                    }
                }).await;
                tokio::task::yield_now().await;
            }
        });

        tokio::time::sleep(Duration::from_millis(500)).await;

        let page = match browser.new_page("about:blank").await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("❌ Page creation FAILED: {}", e);
                drop(browser);
                tokio::time::sleep(Duration::from_millis(500)).await;
                continue;
            }
        };

        match page.set_content(crash_html).await {
            Ok(_) => eprintln!("✅ set_content() succeeded"),
            Err(e) => {
                eprintln!("❌ set_content() FAILED: {}", e);
                let _ = page.close().await;
                drop(browser);
                tokio::time::sleep(Duration::from_millis(500)).await;
                continue;
            }
        }

        tokio::time::sleep(Duration::from_secs(1)).await;

        let eval_result = tokio::time::timeout(
            Duration::from_secs(3),
            page.evaluate("document.body.textContent")
        ).await;

        match eval_result {
            Ok(Ok(result)) => {
                eprintln!("✅✅✅ SUCCESS! evaluate() worked - NO CRASH with these flags!");
                eprintln!("    Result: {:?}", result);
            }
            Ok(Err(e)) => eprintln!("❌ evaluate() error: {}", e),
            Err(_) => eprintln!("⏱️ evaluate() TIMEOUT - crashed"),
        }

        let _ = page.close().await;
        drop(browser);
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    eprintln!("\n========================================");
    eprintln!("Flag test complete!");
    eprintln!("========================================");

    Ok(())
}
