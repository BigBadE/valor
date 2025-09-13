use anyhow::Result;
use tokio::runtime::Runtime;

mod common;

#[test]
fn mutation_observer_attributes_basic() -> Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();
    let rt = Runtime::new()?;
    let fixture = common::fixtures_dir().join("mutation").join("attributes_basic.html");
    let url = common::to_file_url(&fixture)?;
    let mut page = common::create_page(&rt, url)?;

    // Drive parsing to completion
    let finished = common::update_until_finished(&rt, &mut page, |_| Ok(()))?;
    assert!(finished, "HTML parsing did not finish in time");

    // A couple of extra ticks for safety and to flush any pending jobs
    for _ in 0..3 { let _ = rt.block_on(page.update()); }

    let out = page.text_content_by_id_sync("out").unwrap_or_default();
    // Expect three attribute records: data-x (old ""), data-x (old "1"), class (old "")
    assert_eq!(out, "attr:data-x:|attr:data-x:1|attr:class:", "Unexpected MO attributes summary: {}", out);
    Ok(())
}

#[test]
fn mutation_observer_childlist_and_subtree() -> Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();
    let rt = Runtime::new()?;
    // Inline HTML via data URL is not supported by loader; provide as a small fixture file
    let fixture = common::fixtures_dir().join("mutation").join("childlist_subtree.html");
    let url = common::to_file_url(&fixture)?;
    let mut page = common::create_page(&rt, url)?;

    let finished = common::update_until_finished(&rt, &mut page, |_| Ok(()))?;
    assert!(finished, "HTML parsing did not finish in time");

    for _ in 0..3 { let _ = rt.block_on(page.update()); }

    let out = page.text_content_by_id_sync("out").unwrap_or_default();
    // Expect a summary like: add,add,remove,attr where attr came from subtree (child attribute change)
    assert_eq!(out, "childList+childList+childList+attributes", "Unexpected MO childList/subtree summary: {}", out);
    Ok(())
}
