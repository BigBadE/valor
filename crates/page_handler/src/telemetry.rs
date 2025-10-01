//! Telemetry utilities for formatting and emitting performance counters.
//!
//! This module is kept independent of `HtmlPage` internals; callers pass
//! counter data explicitly for serialization and optional output.

/// Performance counters for layout and style computation.
///
/// These counters track work done during page rendering and can be
/// serialized to JSON for heads-up display or logging purposes.
#[derive(Debug, Clone, Copy)]
pub struct PerfCounters {
    /// Number of nodes reflowed in the last layout pass.
    pub nodes_reflowed_last: u64,
    /// Cumulative number of nodes reflowed since page load.
    pub nodes_reflowed_total: u64,
    /// Number of dirty subtrees detected in the last layout pass.
    pub dirty_subtrees_last: u64,
    /// Time spent in the last layout pass (milliseconds).
    pub layout_time_last_ms: u64,
    /// Cumulative time spent in layout since page load (milliseconds).
    pub layout_time_total_ms: u64,
    /// Number of nodes restyled in the last style pass.
    pub restyled_nodes_last: u64,
    /// Number of layout requests deferred due to frame budget limits.
    pub spillover_deferred: u64,
    /// Number of line boxes generated in the last layout pass.
    pub line_boxes_last: u64,
    /// Number of text shaping runs performed in the last layout pass.
    pub shaped_runs_last: u64,
    /// Number of early-out optimizations taken in the last layout pass.
    pub early_outs_last: u64,
}

/// Serializes performance counters to a compact JSON string.
///
/// The output is a single-line JSON object suitable for logging or
/// display in a heads-up overlay.
#[must_use]
pub fn perf_counters_json(counters: &PerfCounters) -> String {
    format!(
        "{{\"nodes_reflowed_last\":{},\"nodes_reflowed_total\":{},\"dirty_subtrees_last\":{},\"layout_time_last_ms\":{},\"layout_time_total_ms\":{},\"restyled_nodes_last\":{},\"spillover_deferred\":{},\"line_boxes_last\":{},\"shaped_runs_last\":{},\"early_outs_last\":{}}}",
        counters.nodes_reflowed_last,
        counters.nodes_reflowed_total,
        counters.dirty_subtrees_last,
        counters.layout_time_last_ms,
        counters.layout_time_total_ms,
        counters.restyled_nodes_last,
        counters.spillover_deferred,
        counters.line_boxes_last,
        counters.shaped_runs_last,
        counters.early_outs_last
    )
}

/// Conditionally emits a JSON line to stdout if telemetry is enabled.
///
/// # Arguments
///
/// * `enabled` - Whether telemetry output is active
/// * `json_line` - The JSON string to emit
pub fn maybe_emit(enabled: bool, json_line: &str) {
    if enabled {
        log::info!("{json_line}");
    }
}
