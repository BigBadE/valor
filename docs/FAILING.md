# Failing fixtures (tracked via `.fail` files)

This list is auto-sourced from the repository's current set of `.fail` fixtures. Paths are repository-relative. Use `.fail` to temporarily skip a fixture when the behavior is not yet implemented or parity is knowingly off.

- Discovery rules (current harness):
  - Layout and graphics fixtures are auto-discovered under any crate's `tests/fixtures/**/` subfolders.
  - A `.fail` extension suppresses a fixture until the underlying feature is implemented.

Below, fixtures are grouped by area with a short reason so we know when to re-enable them.

## CSS: Box

- crates/css/modules/box/tests/fixtures/layout/box/clearance_breaks_collapse.fail — Clearance and margin-collapsing behavior incomplete.
- crates/css/modules/box/tests/fixtures/layout/box/margins_padding_borders.fail — Box edge computations not yet spec-accurate.
- crates/css/modules/box/tests/fixtures/layout/box/bfc_flex_no_parent_collapse.fail — Flex item main-size (flex-basis:auto for empty items) not yet aligned with spec; container is BFC so no parent/first-child collapse. Pending minimal flex sizing.
- crates/css/modules/box/tests/fixtures/layout/box/margin_collapse_empty_block.fail — Collapsing across empty block chains requires full propagation and correct max/most-negative arithmetic. Pending dedicated implementation.

## CSS: Display (Inline formatting)

- crates/css/modules/display/tests/fixtures/layout/inline/04_block_inline_partition.fail — Inline formatting context/anonymous block partitioning incomplete.
- crates/css/modules/display/tests/fixtures/layout/inline/12_anonymous_blocks.fail — Anonymous block generation not finalized.

## CSS: Flexbox

- crates/css/modules/flexbox/tests/fixtures/layout/flex/06_flex_row.fail — Flex layout parity pending.
- crates/css/modules/flexbox/tests/fixtures/layout/flex/07_flex_grow.fail — Flex sizing/grow behavior pending.
- crates/css/modules/flexbox/tests/fixtures/layout/flex/08_flex_shrink.fail — Flex shrink behavior pending.
- crates/css/modules/flexbox/tests/fixtures/layout/flex/11_align_items_center.fail — Align-items normalization/parity pending.
- crates/css/modules/flexbox/tests/fixtures/layout/flex/12_justify_content_center.fail — Justify-content parity pending.
- crates/css/modules/flexbox/tests/fixtures/layout/flex/13_wrap_basic.fail — Flex wrapping parity pending.
- crates/css/modules/flexbox/tests/fixtures/layout/flex/14_justify_space_between.fail — Justify-content space-between parity pending.
- crates/css/modules/flexbox/tests/fixtures/layout/flex/15_flex_row_basic.fail — Flex layout baseline parity pending.
- crates/css/modules/flexbox/tests/fixtures/layout/flex/16_flex_sizing_grow_shrink.fail — Combined grow/shrink sizing parity pending.

## CSS: Positioning

- crates/css/modules/position/tests/fixtures/layout/positioning/01_absolute_basic.fail — Absolute positioning behavior pending.
- crates/css/modules/position/tests/fixtures/layout/positioning/02_fixed_basic.fail — Fixed positioning behavior pending.
- crates/css/modules/position/tests/fixtures/layout/positioning/09_absolute_position.fail — Absolute positioning edge-cases pending.
- crates/css/modules/position/tests/fixtures/layout/positioning/10_fixed_position.fail — Fixed positioning edge-cases pending.

## CSS: Sizing

- crates/css/modules/sizing/tests/fixtures/layout/basics/01_auto_width.fail — Auto size resolution not fully spec-accurate.
- crates/css/modules/sizing/tests/fixtures/layout/basics/02_auto_width.fail — Auto size resolution not fully spec-accurate.
- crates/css/modules/sizing/tests/fixtures/layout/basics/02_percent_width.fail — Percentage size resolution not fully spec-accurate.
- crates/css/modules/sizing/tests/fixtures/layout/basics/03_percent_width.fail — Percentage size resolution not fully spec-accurate.

## Valor: Cross-module scenarios

- crates/valor/tests/fixtures/layout/display_normalization/index.fail — Display normalization relies on flex/inline parity.
- crates/valor/tests/fixtures/layout/flex_props/index.fail — Align-items normalization + flex layout parity pending.
- crates/valor/tests/fixtures/layout/overflow_visibility/index.fail — Overflow behavior/parity pending.
- crates/valor/tests/fixtures/layout/selectors/selectors_descendant_vs_child.fail — Requires flex geometry parity for comparison harness.
- crates/valor/tests/fixtures/layout/selectors/selectors_id_class_type.fail — Depends on flex layout parity for expected geometry.

Notes:

- Remove the `.fail` extension once the corresponding feature reaches parity and the Chromium comparer agrees.
- Keep reasons concise and focused on missing behavior. If a fixture no longer needs to fail, delete its entry here and rename the file back to `.html`.
