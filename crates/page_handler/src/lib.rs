//! Page handler subsystem for Valor browser engine.
//!
//! This crate orchestrates HTML parsing, CSS styling, layout computation, and rendering
//! for a single web page. It coordinates DOM updates, JavaScript execution, and event
//! dispatch while maintaining synchronized mirrors for style computation, layout, and
//! rendering subsystems.

pub mod accessibility;
pub mod config;
/// Display API methods (now integrated into state.rs)
mod display_api;
/// Display list building helpers for retained mode
mod display_retained;
/// Embedded chrome assets for valor:// URL scheme
mod embedded_chrome;
pub mod events;
pub mod focus;
/// Layout manager for constraint-based layout
pub mod layout_manager;
/// Display list generation from layout and styles
mod paint;
/// JavaScript runtime abstraction and default runtime implementation
mod runtime;
pub mod scheduler;
pub mod selection;
pub mod snapshots;
/// CSS stacking context and z-index handling
mod stacking;
pub mod state;
pub mod telemetry;
/// URL streaming utilities for http, https, file, and valor:// schemes
mod url;
