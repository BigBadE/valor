//! GPU-centric renderer for the rewrite architecture.
//!
//! # Architecture Overview
//!
//! This renderer uses a GPU-centric model where:
//! - **CPU stores**: Formula relationships, root values, interest tracking
//! - **GPU stores**: All computed layout values, element render data
//! - **Communication**: Only deltas flow from CPU to GPU
//!
//! ## Data Flow
//!
//! 1. CSS changes trigger formula graph updates on CPU
//! 2. Root value changes (viewport, explicit sizes) produce deltas
//! 3. Deltas are batched and sent to GPU
//! 4. GPU applies deltas through formula relationships
//! 5. GPU determines damage regions and redraws affected tiles
//!
//! ## GPU Buffers
//!
//! - **Formula Buffer**: Encodes relationships (Alias, Offset, Scaled, Affine)
//! - **Value Buffer**: Current computed values per node/axis
//! - **Element Buffer**: Render data (colors, borders, etc.) per node
//! - **Interest Buffer**: Visibility flags per node
//!
//! ## Incremental Updates
//!
//! When a root changes by delta D:
//! - Alias children: apply D unchanged
//! - Offset children (value + C): apply D unchanged
//! - Scaled children (value * S): apply D * S
//! - Affine children (value * S + C): apply D * S
//!
//! The GPU traverses the formula graph and applies transformed deltas,
//! marking affected tiles for redraw.

pub mod renderer;

pub use renderer::{ComputedBox, LayoutState, Renderer};
