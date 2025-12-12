//! Cross-axis alignment and baseline support.

pub mod alignment;
pub mod baseline;

pub use alignment::{BaselineAdjustCtx, adjust_cross_for_baseline};
pub use baseline::BaselineMetrics;
pub use baseline::compute_line_baseline_ref;
