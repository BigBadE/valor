/// Telemetry utilities for formatting and emitting perf counters.
/// Kept independent of HtmlPage internals; callers pass in counters explicitly.
pub fn perf_counters_json(
    nodes_reflowed_last: u64,
    nodes_reflowed_total: u64,
    dirty_subtrees_last: u64,
    layout_time_last_ms: u64,
    layout_time_total_ms: u64,
    restyled_nodes_last: u64,
    spillover_deferred: u64,
    line_boxes_last: u64,
    shaped_runs_last: u64,
    early_outs_last: u64,
) -> String {
    format!(
        "{{\"nodes_reflowed_last\":{},\"nodes_reflowed_total\":{},\"dirty_subtrees_last\":{},\"layout_time_last_ms\":{},\"layout_time_total_ms\":{},\"restyled_nodes_last\":{},\"spillover_deferred\":{},\"line_boxes_last\":{},\"shaped_runs_last\":{},\"early_outs_last\":{}}}",
        nodes_reflowed_last,
        nodes_reflowed_total,
        dirty_subtrees_last,
        layout_time_last_ms,
        layout_time_total_ms,
        restyled_nodes_last,
        spillover_deferred,
        line_boxes_last,
        shaped_runs_last,
        early_outs_last
    )
}

pub fn maybe_emit(enabled: bool, json_line: &str) {
    if enabled {
        println!("{}", json_line);
    }
}
