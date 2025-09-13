use anyhow::Result;
use tokio::runtime::Runtime;

mod common;

#[test]
fn events_common_and_microtask_ordering() -> Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();
    let rt = Runtime::new()?;
    let fixture = common::fixtures_dir().join("events").join("events_common.html");
    let url = common::to_file_url(&fixture)?;
    let mut page = common::create_page(&rt, url)?;

    // Drive parsing to completion
    let finished = common::update_until_finished(&rt, &mut page, |_| Ok(()))?;
    assert!(finished, "HTML parsing did not finish in time");

    // Run a few additional ticks to allow the setTimeout(0) to fire and microtasks to flush.
    for _ in 0..8 { let _ = rt.block_on(page.update()); }

    let text = page.text_content_by_id_sync("out").unwrap_or_default();
    assert_eq!(
        text,
        "c-target-click,p-bubble-click,p-capture-keydown,c-target-keydown,input,change,submit,micro",
        "unexpected events/microtask order: {}",
        text
    );
    Ok(())
}
