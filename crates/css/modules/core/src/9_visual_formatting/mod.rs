//! CSS 2.2 Chapter 9 — Visual formatting model (spec-mirrored folder)
//!
//! This module mirrors the spec chapter to satisfy the enforced spec-driven folder structure.
//! Implementation lives in:
//! - `orchestrator` for layout entry and root selection.
//! - `lib.rs` for the block children placement loop and helpers.
//! - `visual_formatting::vertical` for BFC/leading collapse.
//! - `visual_formatting::horizontal` for horizontal solving interactions.
//!
//! See `crates/css/modules/layouter/spec.md` mapping entries for §9.*.

// No code here by design; this module provides navigational anchors to the spec.

pub mod part_9_4_1_block_formatting_context;
pub mod part_9_5_floats;
