use anyhow::Error;
use crate::common::{create_page, update_until_finished_simple};
use tokio::runtime::Runtime;

mod common;

/// Verify that after the initial full layout, a subsequent update tick with no DOM/style
/// changes results in zero reflowed nodes (layout is skipped).
#[test]
fn layout_is_skipped_on_noop_tick() -> Result<(), Error> {
    // Initialize logger for visibility when running tests locally
    let _ = env_logger::builder().is_test(true).try_init();

    // Load a simple fixture page
    let rt = Runtime::new()?;
    // Use the JS fixture already in the repository
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests").join("fixtures").join("layout").join("basics").join("01_auto_width.html");
    let url = common::to_file_url(&path)?;
    let mut page = create_page(&rt, url)?;

    // Drive parsing & initial layout to completion
    let finished = update_until_finished_simple(&rt, &mut page)?;
    assert!(finished, "page parsing did not finish in time");

    // Read last reflow count (should be > 0 after initial compute)
    let first_count = page.layouter_perf_nodes_reflowed_last();
    assert!(first_count > 0, "expected initial layout to reflow some nodes, got {}", first_count);

    // Run one more update tick with no changes
    rt.block_on(page.update())?;

    // Ensure that the layouter reports zero nodes reflowed on the no-op tick
    let second_count = page.layouter_perf_nodes_reflowed_last();
    assert_eq!(second_count, 0, "expected no reflow on no-op tick, got {}", second_count);
    Ok(())
}
