//! CSS 2.2 Chapter 8 — Box model (spec-mirrored folder)
//!
//! This module mirrors the spec chapter to satisfy the enforced spec-driven folder structure.
//! Implementation lives in:
//! - `visual_formatting::horizontal` for width/margins (e.g., `solve_block_horizontal`).
//! - `sizing` helpers.
//! - `visual_formatting::vertical` for margin collapsing interactions.
//!
//! See also `crates/css/modules/layouter/spec.md` mapping entries for §8.*.

// No code here by design; this module provides navigational anchors to the spec.

pub mod part_8_3_1_collapsing_margins;
