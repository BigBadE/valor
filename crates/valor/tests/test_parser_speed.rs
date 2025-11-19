use anyhow::Result;
use page_handler::config::ValorConfig;
use page_handler::state::HtmlPage;
use std::time::Instant;
use url::Url;

#[tokio::test(flavor = "current_thread")]
async fn test_parser_performance() -> Result<()> {
    env_logger::builder().is_test(true).try_init().ok();

    let start = Instant::now();

    // Simple HTML content
    let html = r#"
        <!DOCTYPE html>
        <html>
        <head><title>Test</title></head>
        <body>
            <div style="margin: 10px; padding: 20px;">Hello World</div>
        </body>
        </html>
    "#;

    // Create temp file
    let temp_path = std::env::temp_dir().join("test_parser_speed.html");
    std::fs::write(&temp_path, html)?;

    let url = Url::from_file_path(&temp_path).unwrap();
    let handle = tokio::runtime::Handle::current();
    let config = ValorConfig::from_env();

    eprintln!("Creating page...");
    let create_start = Instant::now();
    let mut page = HtmlPage::new(&handle, url, config).await?;
    eprintln!("Page created in {:?}", create_start.elapsed());

    eprintln!("Updating until finished...");
    let update_start = Instant::now();
    let max_iterations = 10000;
    let mut iterations = 0;
    while !page.parsing_finished() && iterations < max_iterations {
        page.update().await?;
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        iterations += 1;
    }
    page.update().await?; // Final update
    eprintln!("Updates finished in {:?} ({} iterations)", update_start.elapsed(), iterations);
    eprintln!("Parsing finished: {}", page.parsing_finished());

    eprintln!("TOTAL TIME: {:?}", start.elapsed());

    assert!(page.parsing_finished(), "Parser should have finished");
    assert!(start.elapsed().as_millis() < 5000, "Parser took too long: {:?}", start.elapsed());

    Ok(())
}
