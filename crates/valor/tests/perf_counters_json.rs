use anyhow::Error;
use serde_json::Value;
use tokio::runtime::Runtime;

mod common;

/// Verify perf_counters_snapshot_string returns JSON with Phase 8 keys.
#[test]
fn perf_counters_snapshot_contains_phase8_keys() -> Result<(), Error> {
    let _ = env_logger::builder().is_test(true).try_init();
    let rt = Runtime::new()?;
    // Use a small, existing fixture
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("layout")
        .join("basics")
        .join("01_auto_width.html");
    let url = common::to_file_url(&path)?;
    let mut page = common::create_page(&rt, url)?;

    // Drive parsing to completion
    let finished = common::update_until_finished_simple(&rt, &mut page)?;
    assert!(finished, "page parsing did not finish in time");

    let s = page.perf_counters_snapshot_string();
    let v: Value = serde_json::from_str(&s).expect("valid JSON");
    // Required keys
    assert!(
        v.get("nodes_reflowed_last").is_some(),
        "missing nodes_reflowed_last: {s}"
    );
    assert!(
        v.get("nodes_reflowed_total").is_some(),
        "missing nodes_reflowed_total: {s}"
    );
    assert!(
        v.get("dirty_subtrees_last").is_some(),
        "missing dirty_subtrees_last: {s}"
    );
    assert!(
        v.get("layout_time_last_ms").is_some(),
        "missing layout_time_last_ms: {s}"
    );
    assert!(
        v.get("layout_time_total_ms").is_some(),
        "missing layout_time_total_ms: {s}"
    );
    assert!(
        v.get("restyled_nodes_last").is_some(),
        "missing restyled_nodes_last: {s}"
    );
    assert!(
        v.get("spillover_deferred").is_some(),
        "missing spillover_deferred: {s}"
    );
    Ok(())
}
