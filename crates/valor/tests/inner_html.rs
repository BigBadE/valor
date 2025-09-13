use anyhow::Result;
use tokio::runtime::Runtime;

mod common;

#[test]
fn inner_html_setter_getter_basic() -> Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();
    let rt = Runtime::new()?;
    let fixture = common::fixtures_dir().join("dom").join("inner_html_basic.html");
    let url = common::to_file_url(&fixture)?;
    let mut page = common::create_page(&rt, url)?;

    // Drive parsing to completion
    let finished = common::update_until_finished(&rt, &mut page, |_| Ok(()))?;
    assert!(finished, "HTML parsing did not finish in time");

    // Allow DOM updates triggered by innerHTML script to propagate
    for _ in 0..10 { let _ = rt.block_on(page.update()); }

    let text = page.text_content_by_id_sync("out").unwrap_or_default();
    // Expect: A,1,<span id="a" class="x y">A</span><b>B</b>
    assert!(text.starts_with("A,1,"), "unexpected prefix: {}", text);
    assert!(text.contains("<spanid=\"a\"class=\"xy\">A</span><b>B</b>"), "unexpected innerHTML: {}", text);

    Ok(())
}
