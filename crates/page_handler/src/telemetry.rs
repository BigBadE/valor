/// Telemetry utilities for formatting and emitting perf counters.
/// Kept independent of HtmlPage internals; callers pass in counters explicitly.
#[derive(Debug, Clone, Copy)]
pub struct PerfCounters {
    pub nodes_reflowed_last: u64,
    pub nodes_reflowed_total: u64,
    pub dirty_subtrees_last: u64,
    pub layout_time_last_ms: u64,
    pub layout_time_total_ms: u64,
    pub restyled_nodes_last: u64,
    pub spillover_deferred: u64,
    pub line_boxes_last: u64,
    pub shaped_runs_last: u64,
    pub early_outs_last: u64,
}

pub fn perf_counters_json(c: &PerfCounters) -> String {
    format!(
        "{{\"nodes_reflowed_last\":{},\"nodes_reflowed_total\":{},\"dirty_subtrees_last\":{},\"layout_time_last_ms\":{},\"layout_time_total_ms\":{},\"restyled_nodes_last\":{},\"spillover_deferred\":{},\"line_boxes_last\":{},\"shaped_runs_last\":{},\"early_outs_last\":{}}}",
        c.nodes_reflowed_last,
        c.nodes_reflowed_total,
        c.dirty_subtrees_last,
        c.layout_time_last_ms,
        c.layout_time_total_ms,
        c.restyled_nodes_last,
        c.spillover_deferred,
        c.line_boxes_last,
        c.shaped_runs_last,
        c.early_outs_last
    )
}

pub fn maybe_emit(enabled: bool, json_line: &str) {
    if enabled {
        println!("{}", json_line);
    }
}
