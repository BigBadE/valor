use anyhow::Result;
use tokio::runtime::Runtime;

mod common;

#[test]
fn classic_external_script_executes() -> Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();
    let rt = Runtime::new()?;
    let fixture = common::fixtures_dir().join("scripts").join("scripts_classic_external.html");
    let url = common::to_file_url(&fixture)?;
    let mut page = common::create_page(&rt, url)?;

    // Drive parsing to completion
    let finished = common::update_until_finished(&rt, &mut page, |_| Ok(()))?;
    assert!(finished, "HTML parsing did not finish in time");

    // Allow any script-enqueued DOM mutations to propagate
    for _ in 0..6 { let _ = rt.block_on(page.update()); }

    let out = page.text_content_by_id_sync("out").unwrap_or_default();
    assert_eq!(out, "ext", "external classic script should have run, got: {}", out);
    Ok(())
}

#[test]
fn classic_defer_runs_before_domcontentloaded() -> Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();
    let rt = Runtime::new()?;
    let fixture = common::fixtures_dir().join("scripts").join("defer_domcontentloaded.html");
    let url = common::to_file_url(&fixture)?;
    let mut page = common::create_page(&rt, url)?;

    // Drive parsing to completion
    let finished = common::update_until_finished(&rt, &mut page, |_| Ok(()))?;
    assert!(finished, "HTML parsing did not finish in time");

    // A couple of extra ticks for safety
    for _ in 0..3 { let _ = rt.block_on(page.update()); }

    let out = page.text_content_by_id_sync("out").unwrap_or_default();
    assert_eq!(out, "1", "deferred script must execute before DOMContentLoaded, got: {}", out);
    Ok(())
}
