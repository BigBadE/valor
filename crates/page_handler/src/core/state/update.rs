//! Update cycle logic for page state.

use crate::core::incremental_layout::IncrementalLayoutEngine;
use crate::core::pipeline::Pipeline;
use crate::internal::runtime::JsRuntime as _;
use crate::utilities::scheduler::FrameScheduler;
use anyhow::Error;
use css::{CSSMirror, Orchestrator};
use html::dom::DOM;
use html::parser::HTMLParser;
use js::{DOMMirror, DOMSubscriber, NodeKey, SharedDomIndex};
use js_engine_v8::V8Engine;
use log::info;
use renderer::Renderer;
use tracing::info_span;
use url::Url;

/// Structured outcome of a single `update()` tick. Extend as needed.
pub struct UpdateOutcome {
    pub redraw_needed: bool,
}

/// Run a single update tick for the page.
///
/// # Errors
///
/// Returns an error if any update step fails (DOM loading, JS execution, CSS processing, or layout).
pub async fn perform_update_cycle(
    dom: &mut DOM,
    loader: &mut Option<HTMLParser>,
    dom_index_mirror: &mut DOMMirror<js::DomIndex>,
    dom_index_shared: &SharedDomIndex,
    css_mirror: &mut DOMMirror<CSSMirror>,
    orchestrator_mirror: &mut DOMMirror<Orchestrator>,
    renderer_mirror: &mut DOMMirror<Renderer>,
    incremental_layout: &mut IncrementalLayoutEngine,
    js_engine: &mut V8Engine,
    host_context: &js::HostContext,
    module_resolver: &mut Box<dyn js::ModuleResolver>,
    script_rx: &mut tokio::sync::mpsc::UnboundedReceiver<html::parser::ScriptJob>,
    script_counter: &mut u64,
    url: &Url,
    frame_scheduler: &FrameScheduler,
    lifecycle: &mut super::LifecycleFlags,
    render: &mut super::RenderFlags,
    last_style_restyled_nodes: u64,
    _pipeline: &Pipeline,
) -> Result<UpdateOutcome, Error> {
    let _span = info_span!("page.update").entered();

    // Finalize DOM loading if the loader has finished
    super::dom_processing::finalize_dom_loading_if_needed(loader).await?;

    // Drive a single JS timers tick via runtime
    let mut runtime = crate::internal::runtime::DefaultJsRuntime;
    // Build minimal page wrapper for runtime tick
    let mut page = crate::internal::runtime::PageForRuntime {
        js_engine,
        script_rx,
        script_counter,
        module_resolver,
        url,
        host_context,
        dom_index_mirror,
        loader,
    };
    runtime.tick_timers_once(&mut page);

    // Apply any pending DOM updates and get them for incremental engine
    let dom_updates = dom.update()?;
    // Apply DOM updates to incremental engine
    incremental_layout.apply_dom_updates(&dom_updates);
    // Keep the DOM index mirror in sync before any JS queries (e.g., getElementById)
    dom_index_mirror.try_update_sync()?;

    // Execute pending scripts and DOMContentLoaded sequencing via runtime after DOM drained
    runtime.drive_after_dom_update(&mut page).await?;

    // Process CSS and style updates
    let style_changed = super::style_layout::process_css_and_styles(
        css_mirror,
        orchestrator_mirror,
        incremental_layout,
        loader.as_ref(),
        &mut lifecycle.style_nodes_rebuilt_after_load,
    )?;

    // Compute layout and forward dirty rectangles
    compute_layout(incremental_layout, style_changed, frame_scheduler, render);

    // Drain renderer mirror after DOM broadcast so the scene graph stays in sync (non-blocking)
    renderer_mirror.try_update_sync()?;

    // Emit optional production telemetry for this tick per config
    emit_perf_telemetry_if_enabled(render.telemetry_enabled, last_style_restyled_nodes, frame_scheduler);

    let outcome = UpdateOutcome {
        redraw_needed: render.needs_redraw,
    };
    Ok(outcome)
}

/// Compute layout if needed.
fn compute_layout(
    incremental_layout: &mut IncrementalLayoutEngine,
    _style_changed: bool,
    frame_scheduler: &FrameScheduler,
    render: &mut super::RenderFlags,
) {
    let _span = info_span!("page.compute_layout").entered();

    // Check if incremental engine has dirty nodes or if geometry is empty
    let has_dirty = incremental_layout.has_dirty_nodes();
    let geometry_empty = incremental_layout.rects().is_empty();
    let should_layout = has_dirty || geometry_empty;

    if !should_layout && !frame_scheduler.allow() {
        frame_scheduler.incr_deferred();
        log::trace!("Layout deferred: frame budget not met");
        return;
    }

    if should_layout && frame_scheduler.allow() {
        // Use incremental layout engine
        if let Ok(layout_results) = incremental_layout.compute_layouts() {
            // Log layout performance metrics
            let node_count = layout_results.len();
            info!("Layout: processed={node_count} nodes");

            // Request a redraw after any successful layout pass
            render.needs_redraw = true;
        }
    } else {
        log::trace!("Layout skipped: no DOM/style changes in this tick");
    }
}

/// Emit production-friendly telemetry (JSON) when enabled in `ValorConfig`.
fn emit_perf_telemetry_if_enabled(
    telemetry_enabled: bool,
    last_style_restyled_nodes: u64,
    frame_scheduler: &FrameScheduler,
) {
    use crate::utilities::telemetry::{self as telemetry_mod, PerfCounters};

    if !telemetry_enabled {
        return;
    }

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

    let json = telemetry_mod::perf_counters_json(&counters);
    telemetry_mod::maybe_emit(telemetry_enabled, &json);
}
