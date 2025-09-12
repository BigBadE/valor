use anyhow::Error;
use tokio::runtime::Runtime;
use wgpu_renderer::DisplayItem;

mod common;

/// Verify that when a selection rectangle is set, the retained display list
/// includes semi-transparent highlight rect(s) intersecting inline text boxes.
#[test]
fn selection_highlight_emits_translucent_rects() -> Result<(), Error> {
    let _ = env_logger::builder().is_test(true).try_init();
    let rt = Runtime::new()?;
    // Load a small page with a single line of text
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests").join("fixtures").join("layout").join("selection").join("01_basic.html");
    let url = common::to_file_url(&path)?;
    let mut page = common::create_page(&rt, url)?;

    // Drive parsing to completion
    let finished = common::update_until_finished_simple(&rt, &mut page)?;
    assert!(finished, "page parsing did not finish in time");

    // Set a selection rectangle overlapping the first line of text
    page.selection_set(0, 0, 200, 40);

    // Build retained display list and scan for translucent rects
    let list = page.display_list_retained_snapshot()?;
    let has_translucent = list.items.iter().any(|it| match it {
        DisplayItem::Rect { color, .. } => color[3] < 1.0 && *color != [0.0, 0.0, 0.0, 0.0],
        _ => false,
    });
    assert!(has_translucent, "expected at least one semi-transparent selection highlight rect in retained DL");
    Ok(())
}
