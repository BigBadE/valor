// Test Chrome with verbose logging to see crash details
use chromiumoxide::browser::{Browser, BrowserConfig};
use std::path::Path;
use std::pin::pin;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let chrome_path = Path::new("/opt/google/chrome/google-chrome");

    let config = BrowserConfig::builder()
        .chrome_executable(chrome_path)
        .no_sandbox()
        .window_size(800, 600)
        .arg("--enable-logging")
        .arg("--v=1")  // Verbose level 1
        .build()?;

    let (browser, handler) = Browser::launch(config).await?;

    // Spawn handler task
    tokio::spawn(async move {
        let mut handler = pin!(handler);
        loop {
            futures::future::poll_fn(|cx| {
                use futures::Stream;
                use futures::StreamExt;
                use std::task::Poll;
                match handler.as_mut().poll_next(cx) {
                    Poll::Ready(Some(Ok(_))) => Poll::Ready(()),
                    Poll::Ready(Some(Err(e))) => {
                        eprintln!("Handler error: {:?}", e);
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

    let page = browser.new_page("about:blank").await?;

    eprintln!("Testing with text content...");
    let html = r#"<!DOCTYPE html><html><body><div>Hello World</div></body></html>"#;

    match page.set_content(html).await {
        Ok(_) => eprintln!("✅ set_content() succeeded"),
        Err(e) => {
            eprintln!("❌ set_content() failed: {}", e);
            return Ok(());
        }
    }

    tokio::time::sleep(Duration::from_secs(2)).await;

    eprintln!("Attempting evaluate...");
    let eval_result = tokio::time::timeout(
        Duration::from_secs(5),
        page.evaluate("document.body.textContent")
    ).await;

    match eval_result {
        Ok(Ok(result)) => eprintln!("✅ evaluate() SUCCESS: {:?}", result),
        Ok(Err(e)) => eprintln!("❌ evaluate() error: {}", e),
        Err(_) => eprintln!("⏱️ evaluate() TIMEOUT"),
    }

    Ok(())
}
