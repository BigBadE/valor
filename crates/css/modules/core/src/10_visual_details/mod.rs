//! CSS 2.2 Chapter 10 — Visual formatting details (width/height) (spec-mirrored folder)
//!
//! This module mirrors the spec chapter to satisfy the enforced spec-driven folder structure.
//! Implementation lives in:
//! - `visual_formatting::horizontal` for §10.3.3 block width and margins.
//! - `visual_formatting::height` and `visual_formatting::dimensions` for §10.6.
//! - `lib.rs` glue that orchestrates calls and integrates results.
//!
//! See `crates/css/modules/layouter/spec.md` mapping entries for §10.*.

// No code here by design; this module provides navigational anchors to the spec.

pub mod part_10_1_containing_block;
pub mod part_10_3_3_block_widths;
pub mod part_10_6_3_height_of_blocks;
