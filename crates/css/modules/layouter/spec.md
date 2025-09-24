## Clearance (clear) interactions

- Status: [x] [Production]
- Spec: https://www.w3.org/TR/CSS22/visuren.html#clearance
- Code:
  - `src/lib.rs::place_block_children_loop()` — tracks a running clearance floor derived from preceding floating boxes’ bottom edges.
  - `src/lib.rs::prepare_child_position()` — raises a block with `clear` to at least the clearance floor.
  - `src/lib.rs::compute_collapsed_vertical_margin()` — avoids collapsing the first child with the parent when `clear` applies by virtue of the clearance floor raising the box; sibling collapse logic proceeds after clearance positioning.
- Algorithm notes:
  - Any preceding float contributes its border-box bottom plus its positive bottom margin to the clearance floor (simplified single-floor model).
  - A block with `clear: left | right | both` will be positioned no higher than this floor; its top margin does not collapse across the clearance raise.
- Fixtures:
  - `crates/css/modules/box/tests/fixtures/layout/box/clear_left_after_float_left.html`
  - `crates/css/modules/box/tests/fixtures/layout/box/clearance_breaks_collapse.fail` (ignored until edge-cases are in scope)

# Layouter — Spec Coverage Map (CSS 2.2)

Primary spec: https://www.w3.org/TR/CSS22/

## Scope and maturity

- Status: [MVP] transitioning to [Production].
- Non-production items:
  - [Approximation] Minimal BFC detection in vertical margin propagation.
  - [Approximation] Heuristic structural-emptiness checks for internal top/bottom collapse.
  - [MVP] No inline formatting context (no line boxes), no anonymous block synthesis yet.
  - [MVP] Relative positioning only; absolute/fixed/sticky are out of scope.
  - [TODO] Clearance interactions and full BFC boundary fidelity.

## One-to-one spec mapping (sorted by spec order)

- 8.1 Box model — CSS 2.2
  - Status: [x] [Production]
  - Spec: https://www.w3.org/TR/CSS22/box.html
  - Code:
    - `src/visual_formatting/horizontal.rs` — consumes sizes for block solving.
    - `src/sizing.rs` — helpers.
  - Fixtures:
    - Covered indirectly by all block layout fixtures (width/border/padding resolution), e.g., `crates/css/modules/box/tests/fixtures/layout/box/margins_padding_borders.html`.

Prompt:
It's time to finalize the layouter module. I want you to make sure that all of 8.3.1 is finished and not TODO, and is production browser-ready code. Reference layouter/spec.md, and keep it in sync with your work. Make sure there are extensive tests for every part of the spec.

- 8.3.1 Collapsing margins — CSS 2.2
  - Status: [x] [Production]
  - Spec: https://www.w3.org/TR/CSS22/box.html#collapsing-margins
  - Code:
    - `src/visual_formatting/vertical.rs::apply_leading_top_collapse()` — leading group computation and application.
    - `src/visual_formatting/vertical.rs::effective_child_top_margin()` — internal propagation via structurally-empty chains.
    - `src/visual_formatting/vertical.rs::effective_child_bottom_margin()` — bottom-side propagation.
    - `src/lib.rs::compute_collapsed_vertical_margin()` — first-child (parent-edge/not) and sibling collapsing.
    - `src/lib.rs::collapse_margins_pair()` and `src/lib.rs::collapse_margins_list()` — algebra of extremes.
    - `src/lib.rs::compute_margin_bottom_out()` and `src/lib.rs::compute_first_placed_empty_margin_bottom()` — empty/internal collapse and outgoing bottom.
  - Notes:
    - BFC detection improved: `overflow != visible`, `float != none`, `position != static`, and `display: flex/inline-flex` establish a new BFC for collapsing logic.
    - Structural emptiness refined: boxes that establish a BFC are not considered empty for internal top/bottom propagation; propagation does not cross BFC boundaries.
    - [TODO] Clearance interactions and complex float scenarios still require additional fidelity.
  - Fixtures:
    - `crates/css/modules/box/tests/fixtures/layout/basics/03_margin_collapsing.html`
    - `crates/css/modules/box/tests/fixtures/layout/box/margin_collapse_basic.html`
    - `crates/css/modules/box/tests/fixtures/layout/box/margin_collapse_border_top.html`
    - `crates/css/modules/box/tests/fixtures/layout/box/margin_collapse_empty_block.html`
    - `crates/css/modules/box/tests/fixtures/layout/box/margin_collapse_negative_last_bottom.html`
    - `crates/css/modules/box/tests/fixtures/layout/box/margin_collapse_nested.html`
    - `crates/css/modules/box/tests/fixtures/layout/box/margins_padding_borders.html`

- 9.4.1 Block formatting context — CSS 2.2
  - Status: [x] [MVP]
  - Spec: https://www.w3.org/TR/CSS22/visuren.html#block-formatting
  - Code:
    - `src/orchestrator/mod.rs::layout_root_impl()` — entry for BFC layout.
    - `src/lib.rs::layout_block_children()` — leading group + placement loop.
    - `src/lib.rs::place_block_children_loop()` — sibling iteration and propagation.
  - Notes:
    - [Approximation] Minimal BFC creation checks (see vertical.rs).
  - Fixtures:
    - All block layout fixtures under `crates/css/modules/box/tests/fixtures/layout/**`.

- 9.4.3 Relative positioning — CSS 2.2
  - Status: [x] [MVP]
  - Spec: https://www.w3.org/TR/CSS22/visuren.html#relative-positioning
  - Code:
    - `src/lib.rs::apply_relative_offsets()` — applied in `src/lib.rs::prepare_child_position()`.
  - Notes:
    - [MVP] Absolute/fixed/sticky not implemented.
  - Fixtures:
    - N/A in current fixture set (add targeted relative-position tests).

- 10.3.3 Block-level, non-replaced elements in normal flow — CSS 2.2
  - Status: [x] [Production]
  - Spec: https://www.w3.org/TR/CSS22/visudet.html#blockwidth
  - Code:
    - `src/visual_formatting/horizontal.rs::solve_block_horizontal()` — used width and margins.
  - Fixtures:
    - `crates/css/modules/box/tests/fixtures/layout/box/margins_padding_borders.html`

- 10.6 Calculating heights and margins — CSS 2.2
  - Status: [x] [MVP]
  - Spec: https://www.w3.org/TR/CSS22/visudet.html#computing-heights
  - Code:
    - `src/visual_formatting/height.rs::compute_used_height()` → `src/dimensions/mod.rs::compute_used_height_impl()`.
    - `src/dimensions/mod.rs::compute_child_content_height_impl()` and `src/dimensions/mod.rs::compute_root_heights_impl()` — content aggregation.
  - Notes:
    - Negative bottom margin combinations covered in sibling/empty collapse; BFC boundaries handled in vertical module.
  - Fixtures:
    - Height portions of the block fixtures above (content aggregation and bottom margins).

 

## Algorithms and data flow

- Entry points and orchestrators:
  - `src/orchestrator/mod.rs::compute_layout_impl()` — top-level driver.
  - `src/orchestrator/mod.rs::layout_root_impl()` — selects root and runs children.
- Placement and propagation:
  - `src/lib.rs::layout_block_children()` → `src/lib.rs::place_block_children_loop()` → `src/lib.rs::layout_one_block_child()`.
  - Vertical collapse helpers in `src/visual_formatting/vertical.rs`.
- Width/height calculation:
  - `src/visual_formatting/horizontal.rs` and `src/visual_formatting/height.rs` → `src/dimensions/`.
- Data types:
  - `src/types.rs` (rects, metrics, contexts) and `css_box::compute_box_sides()`.

## Parsing/inputs

- Inputs: `style_engine::ComputedStyle`, DOM updates via `js::DOMUpdate` into `Layouter` (external shim).
- Deviations:
  - [MVP] Inline formatting context and line boxes are not generated; inline content treated simplistically.

## Integration

- Upstream:
  - `style_engine` for computed styles.
  - `css_box` for box sides.
- Downstream:
  - Tests harness via `crates/valor/tests/*` and layout JSON serializer in tests.

## Edge cases and conformance nuances

- Margin collapsing through empty chains: relies on structural emptiness heuristic; BFC boundaries stop propagation.
- Parent-edge non-collapsible cases (padding/border) handled; ensure no double application when forwarding.
- Clearance and floats are partial [TODO].

## Testing and fixtures

- Fixtures auto-discovered under: `crates/valor/tests/fixtures/layout/**`.
- To ignore a fixture (expected failure), use the `.fail` extension for the file; `.html` fixtures are executed, `.fail` files are skipped by discovery.
- Chromium layout comparer caches per-fixture JSON; compares rect geometry and selected computed styles.

## Documentation and coding standards

- Every public function/type includes a spec reference line:
  - `/// Spec: <https://www.w3.org/TR/<spec>#<section>>`
- File structure mirrors spec chapters where practical (`vertical.rs`, `horizontal.rs`, `height.rs`, `dimensions/`).
- Keep files <~500 lines; split when larger.
- Imports at the top; use short names via `use`, with aliases for collisions.
- Shallow control flow; functions <100 lines; no `#[allow(...)]` without explicit permission (tests exempted).
- Non-production behavior is explicitly tagged in comments with `[MVP]`, `[Approximation]`, `[Heuristic]`, `[Fallback]`, `[Non-normative]`, and linked back to this spec.

## Future work
