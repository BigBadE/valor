use anyhow::Error;
use tokio::runtime::Runtime;

mod common;

/// Ensure display_list_retained_snapshot() is side-effect free and does not
/// trigger rebuilds or infinite loops. It should be cheap to call repeatedly
/// and must not deadlock. As an approximate guard, call it multiple times and
/// verify it returns and that computed styles size remains stable.
#[test]
fn retained_snapshot_is_side_effect_free_and_cheap() -> Result<(), Error> {
    let _ = env_logger::builder().is_test(true).try_init();
    let rt = Runtime::new()?;

    // Use the existing clip fixture since it includes inline <style>.
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("layout")
        .join("clip")
        .join("01_overflow_hidden.html");
    let url = common::to_file_url(&path)?;
    let mut page = common::create_page(&rt, url)?;

    // Drive parsing to completion
    let finished = common::update_until_finished_simple(&rt, &mut page)?;
    assert!(finished, "page parsing did not finish in time");

    // Take a baseline of computed styles size
    let styles_before = page.computed_styles_snapshot()?;
    let count_before = styles_before.len();

    // Call retained snapshot multiple times; this used to cause a loop when it
    // attempted to rebuild StyleEngine nodes or re-merge styles.
    for _ in 0..5 {
        let _list = page.display_list_retained_snapshot()?;
    }

    // Styles should remain stable in size (no side-effects during snapshot)
    let styles_after = page.computed_styles_snapshot()?;
    let count_after = styles_after.len();
    assert_eq!(
        count_before, count_after,
        "computed styles size changed across retained snapshots"
    );

    Ok(())
}
