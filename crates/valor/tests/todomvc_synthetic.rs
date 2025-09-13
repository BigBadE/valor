use anyhow::Result;
use tokio::runtime::Runtime;

mod common;

#[test]
fn todomvc_interactions_under_synthetic_events() -> Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();
    let rt = Runtime::new()?;
    let fixture = common::fixtures_dir().join("events").join("todomvc_synthetic.html");
    let url = common::to_file_url(&fixture)?;
    let mut page = common::create_page(&rt, url)?;

    // Drive parsing to completion
    let finished = common::update_until_finished(&rt, &mut page, |_| Ok(()))?;
    assert!(finished, "HTML parsing did not finish in time");

    // Allow any scripted interactions to complete and DOM updates to propagate
    for _ in 0..10 { let _ = rt.block_on(page.update()); }

    let out = page.text_content_by_id_sync("out").unwrap_or_default();
    // Expect: first item toggled completed, second removed
    assert_eq!(out, "A;completed=1", "unexpected Todo synthetic result: {}", out);

    Ok(())
}
