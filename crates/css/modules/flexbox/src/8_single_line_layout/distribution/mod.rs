//! Flex item sizing and distribution algorithms.
//!
//! Contains grow/shrink algorithms, auto margin resolution, and main-axis positioning.

pub mod auto_margins;
pub mod flex_algorithm;
pub mod main_axis;

pub use auto_margins::{quantize_layout_floor, resolve_auto_margins_and_outer};
pub use flex_algorithm::{clamp, distribute_grow, distribute_shrink};
pub use main_axis::{
    MainOffsetPlan, accumulate_main_offsets, build_main_placements, clamp_first_offset_if_needed,
    justify_params,
};
