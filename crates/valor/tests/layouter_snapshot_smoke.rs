#![allow(clippy::missing_panics_doc, clippy::type_complexity)]

use anyhow::Result;
use js::NodeKey;
use layouter::Layouter;
use std::collections::HashMap;
use tokio::runtime::Runtime;
mod common;

#[test]
fn layouter_snapshot_contains_section_children() -> Result<()> {
    // Build runtime and page for an existing selectors fixture
    let rt = Runtime::new()?;
    let fx_dir = common::fixtures_dir().join("layout").join("selectors");
    let html = fx_dir.join("selectors_id_class_type.html");
    assert!(html.exists(), "fixture missing: {}", html.display());
    let url = common::to_file_url(&html)?;

    let mut page = common::create_page(&rt, url)?;
    let mut layouter_mirror = page.create_mirror(Layouter::new());

    // Drive until finished, ensuring layouter mirror drains each tick
    let finished = common::update_until_finished(&rt, &mut page, |_| {
        layouter_mirror.try_update_sync()?;
        Ok(())
    })?;
    assert!(finished, "page parsing did not finish");

    // Apply stylesheet and computed styles to the external layouter mirror and run layout
    let sheet = page.styles_snapshot()?;
    let computed = page.computed_styles_snapshot()?;
    {
        let layouter = layouter_mirror.mirror_mut();
        layouter.set_stylesheet(sheet);
        layouter.set_computed_styles(computed);
        let _ = layouter.compute_layout();
    }

    // Pull a fresh snapshot from layouter after layout
    let snapshot = layouter_mirror.mirror_mut().snapshot();
    let mut kind_by_key = HashMap::new();
    let mut children_by_key: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();
    for (k, kind, children) in snapshot.into_iter() {
        kind_by_key.insert(k, kind);
        children_by_key.insert(k, children);
    }

    // Choose root like the chromium harness: find the first block node under ROOT recursively.
    fn first_block(
        kind_by_key: &HashMap<NodeKey, layouter::LayoutNodeKind>,
        children_by_key: &HashMap<NodeKey, Vec<NodeKey>>,
        start: NodeKey,
    ) -> Option<NodeKey> {
        if let Some(layouter::LayoutNodeKind::Block { .. }) = kind_by_key.get(&start) {
            return Some(start);
        }
        if let Some(children) = children_by_key.get(&start) {
            for c in children.iter() {
                if let Some(found) = first_block(kind_by_key, children_by_key, *c) {
                    return Some(found);
                }
            }
        }
        None
    }
    let root_elem = first_block(&kind_by_key, &children_by_key, NodeKey::ROOT)
        .expect("a block element exists under ROOT");
    let root_key = match kind_by_key.get(&root_elem) {
        Some(layouter::LayoutNodeKind::Block { tag }) if tag.eq_ignore_ascii_case("html") => {
            children_by_key
                .get(&root_elem)
                .and_then(|kids| {
                    kids.iter().find_map(|c| match kind_by_key.get(c) {
                        Some(layouter::LayoutNodeKind::Block { tag })
                            if tag.eq_ignore_ascii_case("body") =>
                        {
                            Some(*c)
                        }
                        _ => None,
                    })
                })
                .unwrap_or(root_elem)
        }
        _ => root_elem,
    };

    // Under body, find section and verify it has three div children
    let section_key = children_by_key
        .get(&root_key)
        .and_then(|kids| {
            kids.iter().find_map(|c| match kind_by_key.get(c) {
                Some(layouter::LayoutNodeKind::Block { tag })
                    if tag.eq_ignore_ascii_case("section") =>
                {
                    Some(*c)
                }
                _ => None,
            })
        })
        .expect("section element present under root");

    let section_children = children_by_key
        .get(&section_key)
        .cloned()
        .unwrap_or_default();
    // Keep only element blocks
    let element_children: Vec<NodeKey> = section_children
        .into_iter()
        .filter(|c| {
            matches!(
                kind_by_key.get(c),
                Some(layouter::LayoutNodeKind::Block { .. })
            )
        })
        .collect();

    assert_eq!(
        element_children.len(),
        3,
        "expected three div children under section"
    );

    Ok(())
}
