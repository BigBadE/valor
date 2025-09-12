use anyhow::Error;
use tokio::runtime::Runtime;

mod common;

/// Verify that the AX snapshot JSON contains expected roles for simple elements.
#[test]
fn ax_tree_contains_basic_roles() -> Result<(), Error> {
    let _ = env_logger::builder().is_test(true).try_init();
    let rt = Runtime::new()?;
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests").join("fixtures").join("layout").join("ax").join("01_basic.html");
    let url = common::to_file_url(&path)?;
    let mut page = common::create_page(&rt, url)?;

    // Drive parsing to completion
    let finished = common::update_until_finished_simple(&rt, &mut page)?;
    assert!(finished, "page parsing did not finish in time");

    let ax = page.ax_tree_snapshot_string();
    assert!(ax.contains("\"role\":\"button\""), "AX should include a button role: {}", ax);
    assert!(ax.contains("\"role\":\"img\""), "AX should include an img role: {}", ax);
    assert!(ax.contains("Do the thing"), "AX should include aria-label as name: {}", ax);
    assert!(ax.contains("Logo"), "AX should include alt text as name: {}", ax);
    Ok(())
}
