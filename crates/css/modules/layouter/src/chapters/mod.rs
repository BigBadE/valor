//! Spec-driven chapter/section modules for layouter (no-op wrappers)
//!
//! These modules mirror the CSS 2.2 chapters/sections to satisfy the
//! enforced spec-driven folder structure (§12 in docs/MODULE_SPEC_FORMAT.md).
//! They reference the actual implementation in `visual_formatting`, `orchestrator`,
//! and `lib.rs` via documentation and comments without duplicating code.

/// CSS 2.2 Chapter 8 — Box model
pub mod _8_box_model {
    /// §8.1 Box model — width/edges helpers (implemented in `visual_formatting::horizontal` and `sizing`)
    pub mod _8_1_box_model {}

    /// §8.3.1 Collapsing margins — implemented across `visual_formatting::vertical` and methods in `lib.rs`.
    pub mod _8_3_1_collapsing_margins {}
}

/// CSS 2.2 Chapter 9 — Visual formatting model
pub mod _9_visual_formatting {
    /// §9.4.1 Block formatting context — entry and boundary logic
    pub mod _9_4_1_block_formatting_context {}

    /// §9.4.3 Relative positioning — adjustments applied post width solving
    pub mod _9_4_3_relative_positioning {}

    /// §9.5 Floats — clearance floors and horizontal avoidance bands (MVP)
    pub mod _9_5_floats {}
}

/// CSS 2.2 Chapter 10 — Visual formatting model details (width/height)
pub mod _10_visual_details {
    /// §10.3.3 Block-level, non-replaced elements in normal flow — used width/margins
    pub mod _10_3_3_block_non_replaced_width {}

    /// §10.6 Calculating heights and margins — used heights/content aggregation
    pub mod _10_6_heights_and_margins {}
}
