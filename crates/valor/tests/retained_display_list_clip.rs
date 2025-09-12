use anyhow::Error;
use tokio::runtime::Runtime;
use wgpu_renderer::DisplayItem;

mod common;

/// Verify that the retained display list builder emits a clip scope (BeginClip/EndClip)
/// when an element has overflow:hidden.
#[test]
fn retained_display_list_emits_clip_for_overflow_hidden() -> Result<(), Error> {
    let _ = env_logger::builder().is_test(true).try_init();
    let rt = Runtime::new()?;
    // Load our fixture
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests").join("fixtures").join("layout").join("clip").join("01_overflow_hidden.html");
    let url = common::to_file_url(&path)?;
    let mut page = common::create_page(&rt, url)?;

    // Drive parsing to completion
    let finished = common::update_until_finished_simple(&rt, &mut page)?;
    assert!(finished, "page parsing did not finish in time");

    // Build retained display list and scan for clip items
    let list = page.display_list_retained_snapshot()?;
    let has_clip_begin = list.items.iter().any(|it| matches!(it, DisplayItem::BeginClip { .. }));
    let has_clip_end = list.items.iter().any(|it| matches!(it, DisplayItem::EndClip));
    assert!(has_clip_begin && has_clip_end, "expected a clip scope (BeginClip/EndClip) in retained display list");
    Ok(())
}
