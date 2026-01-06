//! Accessor methods and snapshot utilities.

use crate::core::incremental_layout::{IncrementalLayoutEngine, LayoutTreeSnapshot};
use crate::input::selection;
use crate::rendering::paint;
use crate::utilities::accessibility::ax_tree_snapshot_from;
use crate::utilities::scheduler::FrameScheduler;
use crate::utilities::telemetry::{self as telemetry_mod, PerfCounters};
use anyhow::Error;
use css::style_types::ComputedStyle as CssComputedStyle;
use css::{CSSMirror, Orchestrator};
use css_core::LayoutRect;
use html::dom::DOM;
use js::{DOMMirror, DomIndex, NodeKey, SharedDomIndex};
use renderer::{DisplayList, Renderer};
use std::collections::HashMap;

/// Synchronously fetch the textContent for an element by id using the `DomIndex` mirror.
/// This helper keeps the index in sync for tests that query immediately after updates.
pub(super) fn text_content_by_id_sync(
    id: &str,
    dom_index_mirror: &mut DOMMirror<DomIndex>,
    dom_index_shared: &SharedDomIndex,
) -> Option<String> {
    // Keep index mirror fresh for same-tick queries
    let _unused = dom_index_mirror.try_update_sync();
    if let Ok(guard) = dom_index_shared.lock()
        && let Some(key) = guard.get_element_by_id(id)
    {
        return Some(guard.get_text_content(key));
    }
    None
}

pub(super) type TagsMap = HashMap<NodeKey, String>;
pub(super) type ChildrenMap = HashMap<NodeKey, Vec<NodeKey>>;

/// Return a structure-only snapshot for tests: (`tags_by_key`, `element_children`).
///
/// This is derived from the DOM/layout snapshot the page can produce and does not
/// depend on any internal layouter mirrors.
pub(super) fn layout_structure_snapshot(
    incremental_layout: &IncrementalLayoutEngine,
    dom_index_mirror: &mut DOMMirror<DomIndex>,
) -> (TagsMap, ChildrenMap) {
    // Ensure DOM is drained to get the latest nodes
    let _unused = dom_index_mirror.try_update_sync();
    let (tags_by_key, element_children, _raw_children, _text_by_key) =
        super::style_layout::snapshot_layout_maps(incremental_layout);
    (tags_by_key, element_children)
}

/// Perform hit testing at the given coordinates.
pub(super) fn layouter_hit_test(x: i32, y: i32) -> Option<NodeKey> {
    // TODO: Implement hit testing in LayoutEngine
    let _ = (x, y);
    None
}

/// Drain mirrors and return a snapshot clone of computed styles per node.
///
/// Ensures the latest inline `<style>` collected by `CSSMirror` is forwarded to the
/// engine before taking the snapshot so callers that query immediately after
/// parsing completes (without another update tick) still see up-to-date styles.
///
/// # Errors
///
/// Returns an error if CSS synchronization or style processing fails.
pub(super) fn computed_styles_snapshot(
    css_mirror: &mut DOMMirror<CSSMirror>,
    orchestrator_mirror: &mut DOMMirror<Orchestrator>,
) -> Result<HashMap<NodeKey, CssComputedStyle>, Error> {
    // Ensure the latest inline <style> and orchestrator state are reflected
    css_mirror.try_update_sync()?;
    // Ensure the Orchestrator mirror has applied all pending DOM updates before processing
    orchestrator_mirror.try_update_sync()?;
    let sheet = css_mirror.mirror_mut().styles().clone();
    orchestrator_mirror.mirror_mut().replace_stylesheet(&sheet);
    let artifacts = orchestrator_mirror.mirror_mut().process_once()?;
    Ok(artifacts.computed_styles)
}

/// Return a JSON string with key performance counters from the layouter to aid diagnostics (Phase 8).
pub(super) fn perf_counters_snapshot_string(
    last_style_restyled_nodes: u64,
    frame_scheduler: &FrameScheduler,
) -> String {
    let counters = PerfCounters {
        nodes_reflowed_last: 0,
        nodes_reflowed_total: 0,
        dirty_subtrees_last: 0,
        layout_time_last_ms: 0,
        layout_time_total_ms: 0,
        restyled_nodes_last: last_style_restyled_nodes,
        spillover_deferred: frame_scheduler.deferred(),
        line_boxes_last: 0,
        shaped_runs_last: 0,
        early_outs_last: 0,
    };
    telemetry_mod::perf_counters_json(&counters)
}

/// Get a retained snapshot of the display list.
pub(super) fn display_list_retained_snapshot(
    renderer_mirror: &mut DOMMirror<Renderer>,
    incremental_layout: &mut IncrementalLayoutEngine,
) -> DisplayList {
    let _unused2 = renderer_mirror.try_update_sync();

    if let Err(err) = incremental_layout.compute_layouts() {
        tracing::warn!("Failed to compute layouts: {err}");
    }
    let rects = incremental_layout.rects();
    let styles = incremental_layout.computed_styles();
    let snapshot = incremental_layout.snapshot();
    let attrs = incremental_layout.attrs_map();

    let display_list = paint::build_display_list(rects, &styles, &snapshot, attrs);

    // Log the first few rects to see their dimensions
    for (i, (node_key, rect)) in rects.iter().take(5).enumerate() {
        log::info!("  Rect {}: node={:?}, x={}, y={}, width={}, height={}", 
            i, node_key, rect.x, rect.y, rect.width, rect.height);
    }
    
    log::info!(
        "display_list_retained_snapshot: generated {} items from {} rects",
        display_list.items.len(),
        rects.len()
    );
    display_list
}

/// Return a list of selection rectangles by intersecting inline text boxes with a selection rect.
pub(super) fn selection_rects(
    incremental_layout: &mut IncrementalLayoutEngine,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
) -> Vec<LayoutRect> {
    if let Err(err) = incremental_layout.compute_layouts() {
        tracing::warn!("Failed to compute layouts: {err}");
    }
    let rects = incremental_layout.rects();
    let snapshot = incremental_layout.snapshot();
    selection::selection_rects(rects, &snapshot, (x0, y0, x1, y1))
}

/// Compute a caret rectangle at the given point: a thin bar within the inline text box, if any.
pub(super) fn caret_at(
    incremental_layout: &mut IncrementalLayoutEngine,
    x: i32,
    y: i32,
) -> Option<LayoutRect> {
    if let Err(err) = incremental_layout.compute_layouts() {
        tracing::warn!("Failed to compute layouts: {err}");
    }
    let rects = incremental_layout.rects().clone();
    let snapshot = incremental_layout.snapshot();
    let hit = layouter_hit_test(x, y);
    selection::caret_at(&rects, &snapshot, x, y, hit)
}

/// Return a minimal Accessibility (AX) tree snapshot as JSON.
pub(super) fn ax_tree_snapshot_string(incremental_layout: &IncrementalLayoutEngine) -> String {
    let snapshot = incremental_layout.snapshot();
    let attrs_map = incremental_layout.attrs_map();
    ax_tree_snapshot_from(snapshot, attrs_map)
}

/// Return the JSON snapshot of the current DOM tree.
pub(super) fn dom_json_snapshot_string(dom: &DOM) -> String {
    dom.to_json_string()
}

/// Get layout snapshot (node, kind, children) for serialization/testing
pub(super) fn layouter_snapshot(
    incremental_layout: &IncrementalLayoutEngine,
) -> LayoutTreeSnapshot {
    incremental_layout.snapshot()
}
