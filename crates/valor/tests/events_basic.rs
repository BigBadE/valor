use anyhow::Result;
use tokio::runtime::Runtime;

mod common;

#[test]
fn events_capture_bubble_once() -> Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();
    let rt = Runtime::new()?;
    let fixture = common::fixtures_dir().join("events").join("events_basic.html");
    let url = common::to_file_url(&fixture)?;
    let mut page = common::create_page(&rt, url)?;

    // Drive parsing to completion
    let finished = common::update_until_finished(&rt, &mut page, |_| Ok(()))?;
    assert!(finished, "HTML parsing did not finish in time");

    // Allow any additional scripting to run
    for _ in 0..10 { let _ = rt.block_on(page.update()); }

    let text = page.text_content_by_id_sync("out").unwrap_or_default();
    assert_eq!(text, "p-capture,c-target,p-bubble,p-capture,c-target", "unexpected event order: {}", text);

    Ok(())
}
