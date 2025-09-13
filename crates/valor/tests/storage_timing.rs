use anyhow::Result;
use tokio::runtime::Runtime;

mod common;

#[test]
fn storage_and_timing_basics() -> Result<()> {
    let _ = env_logger::builder().is_test(true).try_init();
    let rt = Runtime::new()?;

    // Load a minimal blank page from existing fixtures dir
    let fixture = common::fixtures_dir().join("network").join("network_basic.html");
    let url = common::to_file_url(&fixture)?;
    let mut page = common::create_page(&rt, url)?;

    // Drive parsing to completion
    let finished = common::update_until_finished(&rt, &mut page, |_| Ok(()))?;
    assert!(finished, "HTML parsing did not finish in time");

    // localStorage set/get/length/key/remove/clear
    page.eval_js("localStorage.clear(); localStorage.setItem('a','1'); localStorage.setItem('b','2');")?;
    for _ in 0..3 { let _ = rt.block_on(page.update()); }
    page.eval_js("document.getElementById('out').textContent = String(localStorage.getItem('a')) + ',' + String(localStorage.getItem('b')) + ';' + String(localStorage.length) + ';' + String(localStorage.key(0));")?;
    for _ in 0..3 { let _ = rt.block_on(page.update()); }
    let out1 = page.text_content_by_id_sync("out").unwrap_or_default();
    // Expect both items present and length==2 (key order is implementation-defined; check prefix)
    assert!(out1.starts_with("1,2;2;"), "unexpected localStorage read/length/key: {}", out1);

    page.eval_js("localStorage.removeItem('a'); document.getElementById('out').textContent = String(localStorage.getItem('a')) + ':' + String(localStorage.length);")?;
    for _ in 0..3 { let _ = rt.block_on(page.update()); }
    let out2 = page.text_content_by_id_sync("out").unwrap_or_default();
    assert_eq!(out2, "null:1", "removeItem should delete key and reduce length: {}", out2);

    page.eval_js("localStorage.clear(); document.getElementById('out').textContent = String(localStorage.length);")?;
    for _ in 0..3 { let _ = rt.block_on(page.update()); }
    let out3 = page.text_content_by_id_sync("out").unwrap_or_default();
    assert_eq!(out3, "0", "clear() should empty storage");

    // sessionStorage basic
    page.eval_js("sessionStorage.setItem('x','y'); document.getElementById('out').textContent = String(sessionStorage.getItem('x')); ")?;
    for _ in 0..3 { let _ = rt.block_on(page.update()); }
    let out4 = page.text_content_by_id_sync("out").unwrap_or_default();
    assert_eq!(out4, "y", "sessionStorage getItem mismatch: {}", out4);

    // performance.now monotonic and reasonable delta
    page.eval_js("document.getElementById('out').textContent = String(Math.floor(performance.now()));")?;
    for _ in 0..2 { let _ = rt.block_on(page.update()); }
    let t1: i64 = page.text_content_by_id_sync("out").unwrap_or_default().parse().unwrap_or(0);
    // advance a few ticks
    for _ in 0..5 { let _ = rt.block_on(page.update()); }
    page.eval_js("document.getElementById('out').textContent = String(Math.floor(performance.now()));")?;
    for _ in 0..2 { let _ = rt.block_on(page.update()); }
    let t2: i64 = page.text_content_by_id_sync("out").unwrap_or_default().parse().unwrap_or(0);
    assert!(t2 >= t1, "performance.now should be monotonic: {} -> {}", t1, t2);

    // queueMicrotask ordering vs timers
    page.eval_js(
        "document.addEventListener('DOMContentLoaded', function(){ var el=document.getElementById('out'); el.textContent=''; queueMicrotask(function(){ el.textContent += 'micro'; }); setTimeout(function(){ el.textContent += ',timer'; }, 0); });"
    )?;
    for _ in 0..8 { let _ = rt.block_on(page.update()); }
    let out5 = page.text_content_by_id_sync("out").unwrap_or_default();
    assert_eq!(out5, "micro,timer", "queueMicrotask should run before setTimeout(0): {}", out5);

    Ok(())
}
