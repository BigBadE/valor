use anyhow::Result;
use tokio::runtime::Runtime;

mod common;

#[test]
fn timers_and_microtasks_ordering() -> Result<()> {
    // Initialize logger for visibility during test runs
    let _ = env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init();

    let rt = Runtime::new()?;
    let fixture = common::fixtures_dir().join("runtime").join("timers_basic.html");
    let url = common::to_file_url(&fixture)?;
    let mut page = common::create_page(&rt, url)?;

    // Drive parsing to completion
    let finished = common::update_until_finished(&rt, &mut page, |_| Ok(()))?;
    assert!(finished, "HTML parsing did not finish in time");

    // Run a few additional ticks to allow timers to fire
    for _ in 0..10 {
        let _ = rt.block_on(page.update());
    }

    // Verify that microtask ran before timer: expected text "microtask,timer"
    let text = page.text_content_by_id_sync("out").unwrap_or_default();
    assert_eq!(text, "microtask,timer", "textContent should reflect microtask then timer order, got: {}", text);

    Ok(())
}
