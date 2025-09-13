use anyhow::Result;
use tokio::runtime::Runtime;

mod common;

#[test]
fn dom_core_basic_operations() -> Result<()> {
    // Initialize logger for visibility during test runs
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init();

    let rt = Runtime::new()?;
    let fixture = common::fixtures_dir().join("dom").join("core_basic.html");
    let url = common::to_file_url(&fixture)?;
    let mut page = common::create_page(&rt, url)?;

    // Drive parsing to completion
    let finished = common::update_until_finished(&rt, &mut page, |_| Ok(()))?;
    assert!(finished, "HTML parsing did not finish in time");

    // Run a few additional ticks to allow any timers/microtasks
    for _ in 0..5 {
        let _ = rt.block_on(page.update());
    }

    // Verify that the list was built and queries worked
    let out = page.text_content_by_id_sync("out").unwrap_or_default();
    let len = page.text_content_by_id_sync("len").unwrap_or_default();
    assert_eq!(out, "a|b|a", "joined .item text content incorrect: {}", out);
    assert_eq!(len, "3", "getElementsByClassName length incorrect: {}", len);

    Ok(())
}
