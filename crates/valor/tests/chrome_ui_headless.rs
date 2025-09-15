use anyhow::Result;
use tokio::runtime::Runtime;
use url::Url;
use valor::factory::create_chrome_and_content;

mod common;

#[test]
fn chrome_ui_headless_initializes_and_parses() -> Result<()> {
    // Follow existing test style: init logger in test mode
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init();

    let rt = Runtime::new()?;

    // Use the shared factory to construct chrome + content like main does
    let init = create_chrome_and_content(&rt, Url::parse("https://example.com/")?)?;
    let mut chrome_page = init.chrome_page;

    // Drive parsing to completion
    let finished = common::update_until_finished(&rt, &mut chrome_page, |_| Ok(()))?;
    assert!(finished, "Chrome page parsing did not finish in time");

    // Sanity-check that expected chrome UI elements are present
    let go_text = chrome_page
        .text_content_by_id_sync("go")
        .unwrap_or_default();
    assert_eq!(
        go_text, "Go",
        "Expected chrome UI to include a Go button with text 'Go'"
    );

    // Replace the default content page with a deterministic local fixture for stable assertions
    let fixture = common::fixtures_dir().join("dom").join("core_basic.html");
    let content_url = common::to_file_url(&fixture)?;
    let mut content_page = common::create_page(&rt, content_url)?;

    // Drive parsing to completion
    let content_finished = common::update_until_finished(&rt, &mut content_page, |_| Ok(()))?;
    assert!(
        content_finished,
        "Content page parsing did not finish in time"
    );

    // Assert we can read expected text from the fixture to confirm the DOM executed
    let out = content_page
        .text_content_by_id_sync("out")
        .unwrap_or_default();
    assert!(
        !out.is_empty(),
        "Expected non-empty textContent from #out in content page"
    );

    Ok(())
}
