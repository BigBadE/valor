use anyhow::Result;
use tokio::runtime::Runtime;

mod common;

#[test]
fn fetch_and_xhr_basic() -> Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();
    let rt = Runtime::new()?;

    // Load a minimal page
    let page_fixture = common::fixtures_dir().join("network").join("network_basic.html");
    let url = common::to_file_url(&page_fixture)?;
    let mut page = common::create_page(&rt, url)?;

    // Drive parsing to completion
    let finished = common::update_until_finished(&rt, &mut page, |_| Ok(()))?;
    assert!(finished, "HTML parsing did not finish in time");

    // Fetch text
    let sample_txt = common::fixtures_dir().join("network").join("sample.txt");
    let sample_txt_url = common::to_file_url(&sample_txt)?;
    let js_fetch_text = format!(
        "fetch('{}').then(r=>r.text()).then(t=>document.getElementById('out').textContent=t).catch(()=>document.getElementById('out').textContent='err');",
        sample_txt_url
    );
    page.eval_js(&js_fetch_text)?;
    for _ in 0..10 { let _ = rt.block_on(page.update()); }
    let out = page.text_content_by_id_sync("out").unwrap_or_default();
    assert_eq!(out, "hello world", "fetch().text() content mismatch: {}", out);

    // Fetch json
    let sample_json = common::fixtures_dir().join("network").join("sample.json");
    let sample_json_url = common::to_file_url(&sample_json)?;
    let js_fetch_json = format!(
        "fetch('{}').then(r=>r.json()).then(obj=>document.getElementById('out').textContent=obj.msg+':'+obj.n).catch(()=>document.getElementById('out').textContent='err');",
        sample_json_url
    );
    page.eval_js(&js_fetch_json)?;
    for _ in 0..10 { let _ = rt.block_on(page.update()); }
    let out2 = page.text_content_by_id_sync("out").unwrap_or_default();
    assert_eq!(out2, "hi:42", "fetch().json() mismatch: {}", out2);

    // XMLHttpRequest text
    let js_xhr = format!(
        "(function(){{var x=new XMLHttpRequest();x.open('GET','{}');x.onreadystatechange=function(){{if(x.readyState===4)document.getElementById('out').textContent=x.responseText;}};x.send();}})();",
        sample_txt_url
    );
    page.eval_js(&js_xhr)?;
    for _ in 0..10 { let _ = rt.block_on(page.update()); }
    let out3 = page.text_content_by_id_sync("out").unwrap_or_default();
    assert_eq!(out3, "hello world", "XMLHttpRequest responseText mismatch: {}", out3);

    // Disallowed origin should reject
    let js_block = "fetch('https://example.com/').then(()=>document.getElementById('out').textContent='ok').catch(()=>document.getElementById('out').textContent='err');";
    page.eval_js(js_block)?;
    for _ in 0..10 { let _ = rt.block_on(page.update()); }
    let out4 = page.text_content_by_id_sync("out").unwrap_or_default();
    assert_eq!(out4, "err", "Disallowed https origin should reject");

    Ok(())
}
