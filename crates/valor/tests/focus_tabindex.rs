use anyhow::Error;
use tokio::runtime::Runtime;

mod common;

/// Verify focus traversal using tabindex ordering, then wrap-around.
#[test]
fn focus_traversal_tabindex() -> Result<(), Error> {
    let _ = env_logger::builder().is_test(true).try_init();
    let rt = Runtime::new()?;
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests").join("fixtures").join("layout").join("focus").join("01_tabindex.html");
    let url = common::to_file_url(&path)?;
    let mut page = common::create_page(&rt, url)?;

    // Drive parsing to completion
    let finished = common::update_until_finished_simple(&rt, &mut page)?;
    assert!(finished, "page parsing did not finish in time");

    // Order should be a (1), b (2), c (3)
    let n1 = page.focus_next().expect("first focusable");
    let n2 = page.focus_next().expect("second focusable");
    let n3 = page.focus_next().expect("third focusable");
    // Wrap around
    let n4 = page.focus_next().expect("wrap focusable");

    // Query the DOM index directly for ids
    let key_a = page.get_element_by_id("a").expect("id=a key");
    let key_b = page.get_element_by_id("b").expect("id=b key");
    let key_c = page.get_element_by_id("c").expect("id=c key");

    assert_eq!(n1, key_a, "first focus should be id=a");
    assert_eq!(n2, key_b, "second focus should be id=b");
    assert_eq!(n3, key_c, "third focus should be id=c");
    assert_eq!(n4, key_a, "wrap-around focus should return to id=a");

    Ok(())
}
