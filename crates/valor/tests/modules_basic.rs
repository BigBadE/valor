use anyhow::Result;
use tokio::runtime::Runtime;

mod common;

#[test]
fn modules_inline_and_external_side_effects() -> Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();
    let rt = Runtime::new()?;

    // Inline module should execute (deferred) before DOMContentLoaded
    let inline_fixture = common::fixtures_dir().join("modules").join("inline_basic.html");
    let url_inline = common::to_file_url(&inline_fixture)?;
    let mut page_inline = common::create_page(&rt, url_inline)?;
    let finished_inline = common::update_until_finished(&rt, &mut page_inline, |_| Ok(()))?;
    assert!(finished_inline, "Inline module page parsing did not finish in time");
    for _ in 0..5 { let _ = rt.block_on(page_inline.update()); }
    let out_inline = page_inline.text_content_by_id_sync("out").unwrap_or_default();
    assert_eq!(out_inline, "mod_inline", "inline module side effect not observed: {}", out_inline);

    // External module (file://) should execute as well
    let ext_fixture = common::fixtures_dir().join("modules").join("external_basic.html");
    let url_ext = common::to_file_url(&ext_fixture)?;
    let mut page_ext = common::create_page(&rt, url_ext)?;
    let finished_ext = common::update_until_finished(&rt, &mut page_ext, |_| Ok(()))?;
    assert!(finished_ext, "External module page parsing did not finish in time");
    for _ in 0..6 { let _ = rt.block_on(page_ext.update()); }
    let out_ext = page_ext.text_content_by_id_sync("out").unwrap_or_default();
    assert_eq!(out_ext, "mod_ext", "external module side effect not observed: {}", out_ext);

    Ok(())
}
