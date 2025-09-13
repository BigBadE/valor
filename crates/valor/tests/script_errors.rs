use anyhow::Result;
use tokio::runtime::Runtime;

mod common;

#[test]
fn script_error_calls_window_onerror() -> Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();
    let rt = Runtime::new()?;
    let fixture = common::fixtures_dir().join("scripts").join("error_propagation.html");
    let url = common::to_file_url(&fixture)?;
    let mut page = common::create_page(&rt, url)?;

    // Drive parsing to completion
    let finished = common::update_until_finished(&rt, &mut page, |_| Ok(()))?;
    assert!(finished, "HTML parsing did not finish in time");

    // Run a few extra ticks to flush any microtasks/updates
    for _ in 0..5 { let _ = rt.block_on(page.update()); }

    let out = page.text_content_by_id_sync("out").unwrap_or_default();
    assert!(out.to_lowercase().contains("boom"), "window.onerror should receive error message containing 'boom', got: {}", out);

    Ok(())
}
