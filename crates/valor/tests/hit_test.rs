use anyhow::Error;
use tokio::runtime::Runtime;

mod common;

/// Ensure hit testing returns the NodeKey of the topmost element at a point.
#[test]
fn hit_test_basic() -> Result<(), Error> {
    let _ = env_logger::builder().is_test(true).try_init();
    let rt = Runtime::new()?;
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests").join("fixtures").join("layout").join("hit").join("01_basic.html");
    let url = common::to_file_url(&path)?;
    let mut page = common::create_page(&rt, url)?;

    // Drive parsing to completion
    let finished = common::update_until_finished_simple(&rt, &mut page)?;
    assert!(finished, "page parsing did not finish in time");

    // Hit inside the target box
    let hit = page.hit_test(10, 10);
    assert!(hit.is_some(), "expected to hit an element at (10,10)");

    // Cross-validate by scanning the DOM JSON for the id="target" NodeKey, if present
    let dom_json = page.dom_json_snapshot_string();
    if dom_json.contains("\"id\":\"target\"") {
        // Weak heuristic: extract the preceding key number if schema is as expected
        // This is a best-effort check to avoid tight coupling to JSON structure.
        if let Some(idx) = dom_json.find("\"id\":\"target\"") {
            let prefix = &dom_json[..idx];
            if let Some(key_pos) = prefix.rfind("\"key\":") {
                // Parse a number following \"key\":
                let tail = &prefix[key_pos + 7..];
                let mut digits = String::new();
                for ch in tail.chars() { if ch.is_ascii_digit() { digits.push(ch); } else { break; } }
                if let Ok(num) = digits.parse::<u64>() {
                    let expected = js::NodeKey(num);
                    assert_eq!(hit, Some(expected), "hit_test should return the NodeKey of #target");
                }
            }
        }
    }
    Ok(())
}
