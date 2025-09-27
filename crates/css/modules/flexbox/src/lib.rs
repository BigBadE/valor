//! CSS Flexible Box Layout Module Level 1 — Flex formatting context.
//! Spec: <https://www.w3.org/TR/css-flexbox-1/>

// Chapter modules mapped to the Flexbox Level 1 spec structure.
// Spec: §4 — Flex Formatting Context
#[path = "4_flex_formatting_context/mod.rs"]
mod chapter4;
// Spec: §5 — Flex Containers
#[path = "5_flex_containers/mod.rs"]
mod chapter5;
// Spec: §6 — Flex Items
#[path = "6_flex_items/mod.rs"]
mod chapter6;
// Spec: §7 — Axis and Order
#[path = "7_axis_and_order/mod.rs"]
mod chapter7;
// Spec: §8–9 — Single-line flex layout (subset)
#[path = "8_single_line_layout/mod.rs"]
/// Single-line flex layout and alignment helpers for MVP (§§8–9 subset)
mod chapter8;

// Public re-exports of minimal MVP helpers and types.
pub use chapter4::{DisplayKeyword, establishes_flex_formatting_context};
pub use chapter5::{FlexDirection, FlexWrap};
pub use chapter6::{FlexItem, ItemRef, ItemStyle, collect_flex_items};
pub use chapter7::{Axes, WritingMode, order_key, resolve_axes, sort_items_by_order_stable};
pub use chapter8::{
    AlignItems, CrossContext, CrossPlacement, FlexChild, FlexContainerInputs, FlexPlacement,
    JustifyContent, align_cross_for_items, align_single_line_cross, layout_single_line,
    layout_single_line_with_cross,
};
