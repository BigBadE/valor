# Layouter — Spec Coverage Map (CSS 2.2)

Primary spec: https://www.w3.org/TR/CSS22/

This document maps spec sections to implementation entry points in this crate. If a topic is not implemented, it is marked as TODO with a brief note.

## 8 Visual formatting model

- 8.1 Box model
  - Implemented: Box sides computation used throughout.
  - Code: `crates/css/modules/layouter/src/visual_formatting/horizontal.rs` (consumes sizes), `crates/css/modules/layouter/src/sizing.rs`.
- 8.3.1 Collapsing margins
  - Implemented (block context leading/top/between/sibling basics + fidelity):
    - Leading group scan and application: `visual_formatting/vertical.rs::apply_leading_top_collapse()` and helpers.
    - Effective margins through structurally-empty chains: `vertical.rs::effective_child_top_margin()`, `vertical.rs::effective_child_bottom_margin()`.
    - Parent–first-child at root: `visual_formatting/root.rs::compute_root_y_after_top_collapse()`.
    - BFC boundary stops in propagation (overflow!=visible, float!=none, position!=static): `vertical.rs::establishes_bfc()` and call sites.
    - Clear handling: first non-empty child with `clear` is excluded from parent-edge leading group, and siblings do not collapse with `clear`: `vertical.rs::scan_leading_group()`, `lib.rs::compute_collapsed_vertical_margin()`.
  - TODO:
    - Full BFC boundary handling (overflow, floats, positioned): TODO in `vertical.rs`.
    - Clearance interactions: TODO in `vertical.rs`.
    - Negative-leading behavior audit and test coverage: TODO in `vertical.rs` and tests.
- 8.3.2 Horizontal margins/padding/border (block-level)
  - Implemented (non-replaced blocks): auto margin resolution, min/max.
  - Code: `visual_formatting/horizontal.rs::solve_block_horizontal()`.
  - TODO: Replaced elements, percentage resolution nuances.

## 9 Visual formatting: block formatting contexts

- 9.4.1 Block formatting context basics
  - Implemented (simplified BFC, block children layout loop):
    - Entry: `orchestrator/mod.rs::layout_root_impl()`, `lib.rs::layout_block_children()`.
    - Placement loop: `lib.rs::place_block_children_loop()`.
  - TODO: Proper BFC creation rules (overflow, floats, positioned) — tracked in `vertical.rs`.

## 10 Visual formatting: widths, heights, and positioning

- 10.3.3 Block-level, non-replaced elements in normal flow
  - Implemented: `visual_formatting/horizontal.rs::solve_block_horizontal()`.
- 10.6 Calculating heights and margins
  - Implemented (simplified):
    - Used height: `visual_formatting/height.rs::compute_used_height()` delegates to `dimensions/mod.rs::compute_used_height_impl()`.
    - Content height aggregation for roots and blocks: `dimensions/mod.rs::compute_root_heights_impl()`, `dimensions/mod.rs::compute_child_content_height_impl()`.
  - TODO: Negative bottom margin combination and BFC boundary cases.
- 9.4.3 Relative positioning
  - Implemented (basic): `lib.rs::apply_relative_offsets()`, applied in `lib.rs::prepare_child_position()`.
  - TODO: Static/absolute/fixed positioning and stacking contexts.

## Display tree normalization

- Display: contents flattening
  - Implemented: `box_tree.rs::flatten_display_children()`.
  - Used by: `vertical.rs` margin-collapsing traversal, child enumeration.
  - TODO: Ensure root helpers always use flattened children (see Root: below).

## Root handling

- Parent–first-child top margin collapse at the root
  - Implemented: `visual_formatting/root.rs::compute_root_y_after_top_collapse()`.
  - TODO: Use flattened children consistently (pending update) and incorporate BFC checks.

## Core layouter entry points (orchestration)

- `orchestrator/mod.rs::compute_layout_impl()` — top-level driver used by tests.
- `orchestrator/mod.rs::layout_root_impl()` — picks layout root and runs child layout.
- `lib.rs::layout_block_children()` — runs leading-group handling and child placement loop.
- `lib.rs::layout_one_block_child()` — per-child layout (width/height/margins/rect commit).

## Data types and utilities

- Rectangles and metrics: `types.rs`.
- Box sides: `css_box::compute_box_sides()`.
- Styles: `style_engine::ComputedStyle`.

## Testing and harness

- Auto-discovered fixtures: `crates/valor/tests/fixtures/layout/**`.
- XFAIL mechanism: add `VALOR_XFAIL` (case-insensitive) to skip comparison.
- Chromium comparer: Reuses a single Tokio runtime and tab; compares rect geometry and a subset of computed styles.

## Refactors and TODO map

- Break down large files (>500 lines):
  - TODO: Split `lib.rs` into `types.rs` (types), `layouter/` submodules (placement, heights, introspection).
- Full structurally-empty chain fidelity:
  - TODO: Implement BFC boundary checks and `clear` handling in `vertical.rs`.
  - TODO: Add unit tests for negative-leading collapse and `display: contents` at root.
