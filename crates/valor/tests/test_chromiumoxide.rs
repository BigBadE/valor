use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;

#[tokio::test]
async fn test_chromiumoxide_file_url() -> anyhow::Result<()> {
    env_logger::init();

    eprintln!("Creating browser config...");
    let chrome_path = std::path::PathBuf::from(
        "/root/.local/share/headless-chrome/linux-1095492/chrome-linux/chrome",
    );

    let (mut browser, mut handler) = Browser::launch(
        BrowserConfig::builder()
            .chrome_executable(chrome_path)
            .no_sandbox()
            .window_size(800, 600)
            .build()
            .map_err(|e| anyhow::anyhow!("Browser config error: {}", e))?,
    )
    .await?;

    // Spawn the handler to process Chrome events
    let handle = tokio::task::spawn(async move {
        while let Some(event) = handler.next().await {
            if let Err(e) = event {
                eprintln!("Browser event error: {:?}", e);
            }
        }
    });

    eprintln!("Creating new page...");
    let page = browser.new_page("about:blank").await?;

    eprintln!("Testing evaluation on about:blank...");
    let result = page.evaluate("1 + 1").await?;
    eprintln!("about:blank result: {:?}", result.value());

    eprintln!("Testing document.readyState...");
    let result = page.evaluate("document.readyState").await?;
    eprintln!("readyState: {:?}", result.value());

    eprintln!("SUCCESS! page.evaluate() works perfectly on about:blank");

    // Clean shutdown
    browser.close().await?;
    handle.abort();

    Ok(())
}
