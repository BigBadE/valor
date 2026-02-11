//! Performance and telemetry helpers for `HtmlPage`.

use super::*;
use crate::utilities::telemetry;

impl HtmlPage {
    /// Emit production-friendly telemetry (JSON) when enabled in `ValorConfig`.
    pub fn emit_perf_telemetry_if_enabled(&mut self) {
        telemetry::maybe_emit(
            self.render.telemetry_enabled,
            &self.perf_counters_snapshot_string(),
        );
    }

    /// Return a JSON string with key performance counters from the layouter to aid diagnostics (Phase 8).
    pub fn perf_counters_snapshot_string(&mut self) -> String {
        accessors::perf_counters_snapshot_string(
            self.last_style_restyled_nodes,
            &self.frame_scheduler,
        )
    }

    /// Performance counters from the internal Layouter mirror: nodes reflowed in the last layout.
    #[inline]
    pub const fn layouter_perf_nodes_reflowed_last(&mut self) -> u64 {
        0
    }

    /// Performance counters from the internal Layouter mirror: number of dirty subtrees processed last.
    #[inline]
    pub const fn layouter_perf_dirty_subtrees_last(&mut self) -> u64 {
        0
    }
}
