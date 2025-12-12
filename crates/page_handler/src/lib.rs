//! Page handler subsystem for Valor browser engine.
//!
//! This crate orchestrates HTML parsing, CSS styling, layout computation, and rendering
//! for a single web page. It coordinates DOM updates, JavaScript execution, and event
//! dispatch while maintaining synchronized mirrors for style computation, layout, and
//! rendering subsystems.

// Core page state and orchestration
pub mod core;

// Rendering pipeline modules
pub mod rendering;

// Input and interaction
pub mod input;

// Utilities and support
pub mod utilities;

// Internal helpers
mod internal;

// Re-export main types for convenience
pub use core::pipeline::{Pipeline, PipelineConfig, RenderingMode};
pub use core::state::{HtmlPage, UpdateOutcome};

pub use input::events::KeyMods;
pub use input::focus;
pub use input::selection;

pub use utilities::accessibility;
pub use utilities::config::ValorConfig;
pub use utilities::scheduler::FrameScheduler;
pub use utilities::snapshots::{IRect, LayoutNodeKind};
pub use utilities::telemetry::PerfCounters;
