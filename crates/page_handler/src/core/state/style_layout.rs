//! Style and layout computation helpers.

use crate::core::incremental_layout::IncrementalLayoutEngine;
use crate::utilities::snapshots::LayoutNodeKind;
use anyhow::Error;
use css::{CSSMirror, Orchestrator};
use html::parser::HTMLParser;
use js::{DOMMirror, NodeKey};
use log::trace;
use std::collections::HashMap;
use tracing::info_span;

/// Snapshot key layout-derived maps used by style and testing code.
/// Returns (`tags_by_key`, `element_children_by_key`, `raw_children_by_key`, `text_by_key`)
pub(super) type LayoutMapsSnapshot = (
    HashMap<NodeKey, String>,
    HashMap<NodeKey, Vec<NodeKey>>,
    HashMap<NodeKey, Vec<NodeKey>>,
    HashMap<NodeKey, String>,
);

/// Snapshot key layout-derived maps from incremental layout engine.
pub(super) fn snapshot_layout_maps(
    incremental_layout: &IncrementalLayoutEngine,
) -> LayoutMapsSnapshot {
    let lay_snapshot = incremental_layout.snapshot();
    let mut tags_by_key: HashMap<NodeKey, String> = HashMap::new();
    let mut raw_children: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();
    let mut text_by_key: HashMap<NodeKey, String> = HashMap::new();
    for (key, kind, children) in lay_snapshot {
        match kind {
            LayoutNodeKind::Block { tag } => {
                tags_by_key.insert(key, tag);
            }
            LayoutNodeKind::InlineText { text } => {
                text_by_key.insert(key, text);
            }
            LayoutNodeKind::Document => {}
        }
        raw_children.insert(key, children);
    }
    let mut element_children: HashMap<NodeKey, Vec<NodeKey>> = HashMap::new();
    let children_vec: Vec<_> = raw_children.clone().into_iter().collect();
    for (parent, kids) in children_vec {
        let filtered: Vec<NodeKey> = kids
            .into_iter()
            .filter(|child| tags_by_key.contains_key(child))
            .collect();
        if tags_by_key.contains_key(&parent) || parent == NodeKey::ROOT {
            element_children.insert(parent, filtered);
        }
    }
    (tags_by_key, element_children, raw_children, text_by_key)
}

/// Ensure `StyleEngine`'s node inventory is rebuilt once post-load for deterministic matching.
pub(super) fn maybe_rebuild_style_nodes_after_load(
    loader: Option<&HTMLParser>,
    style_nodes_rebuilt_after_load: &mut bool,
) {
    // Orchestrator does not require an explicit rebuild; no-op guard retained for symmetry.
    if loader.is_none() && !*style_nodes_rebuilt_after_load {
        *style_nodes_rebuilt_after_load = true;
        trace!("process_css_and_styles: orchestrator ready");
    }
}

/// Process CSS and style updates.
///
/// # Errors
///
/// Returns an error if CSS processing fails.
pub(super) fn process_css_and_styles(
    css_mirror: &mut DOMMirror<CSSMirror>,
    orchestrator_mirror: &mut DOMMirror<Orchestrator>,
    incremental_layout: &mut IncrementalLayoutEngine,
    loader: Option<&HTMLParser>,
    style_nodes_rebuilt_after_load: &mut bool,
) -> Result<bool, Error> {
    let _span = info_span!("page.process_css_and_styles").entered();
    // Ensure CSSMirror has applied any pending DOM updates so that inline <style>
    // rules are visible in the aggregated stylesheet for this tick.
    css_mirror.try_update_sync()?;

    // Get attributes from incremental layout
    let lay_attrs = incremental_layout.attrs_map().clone();
    trace!(
        "process_css_and_styles: layouter_attrs_count={} nodes",
        lay_attrs.len()
    );
    // Snapshot structure once and optionally rebuild StyleEngine's inventory
    let (tags_by_key, element_children, raw_children, text_by_key) =
        snapshot_layout_maps(incremental_layout);
    maybe_rebuild_style_nodes_after_load(loader, style_nodes_rebuilt_after_load);

    // Use CSSMirror's aggregated in-document stylesheet (rebuilds on <style> updates)
    let author_styles = css_mirror.mirror_mut().styles();

    // Drain any pending Orchestrator updates
    orchestrator_mirror.try_update_sync()?;

    // Replace the stylesheet in the Orchestrator
    orchestrator_mirror
        .mirror_mut()
        .replace_stylesheet(author_styles);

    // Process styles using Orchestrator
    let artifacts = orchestrator_mirror.mirror_mut().process_once()?;

    eprintln!(
        "Style processing: {} computed styles, changed={}",
        artifacts.computed_styles.len(),
        artifacts.styles_changed
    );

    // Apply computed styles to incremental layout
    incremental_layout.set_computed_styles(&artifacts.computed_styles);

    Ok(artifacts.styles_changed)
}
