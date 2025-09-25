# Layouter — Spec Coverage Map (CSS 2.2)

Primary spec: https://www.w3.org/TR/CSS22/

## Scope and maturity

- Status: [MVP] transitioning to [Production].
- Non-production items:
  - [Approximation] Minimal BFC detection in vertical margin propagation.
  - [Approximation] Heuristic structural-emptiness checks for internal top/bottom collapse.
  - [MVP] No inline formatting context (no line boxes), no anonymous block synthesis yet.
  - [MVP] Relative positioning only; absolute/fixed/sticky are out of scope.
  - [MVP] Clearance scaffolding present; full floats and precise clearance interactions are pending.

## One-to-one spec mapping (sorted by spec order)

- 8.1 Box model — CSS 2.2
  - Status: [x] [Production]
  - Spec: https://www.w3.org/TR/CSS22/box.html
  - Code:
    - `src/visual_formatting/horizontal.rs` — consumes sizes for block solving.
    - `src/sizing.rs` — helpers.
  - Fixtures:
    - Covered indirectly by all block layout fixtures (width/border/padding resolution), e.g., `crates/css/modules/box/tests/fixtures/layout/box/margins_padding_borders.html`.

- 8.3.1 Collapsing margins — CSS 2.2
  - Status: [x] [Production]
  - Spec: https://www.w3.org/TR/CSS22/box.html#collapsing-margins
  - Code:
    - `src/visual_formatting/vertical.rs::apply_leading_top_collapse()` — leading group computation and application.
    - `src/visual_formatting/vertical.rs::effective_child_top_margin()` — internal propagation via structurally-empty chains.
    - `src/visual_formatting/vertical.rs::effective_child_bottom_margin()` — bottom-side propagation.
    - `src/lib.rs::compute_collapsed_vertical_margin()` — first-child (parent-edge/not) and sibling collapsing; for a non-collapsible parent edge (e.g., a BFC), the first placed child uses its own top margin (no absorption of any prior leading group) to avoid double application.
    - `src/lib.rs::collapse_margins_pair()` and `src/lib.rs::collapse_margins_list()` — algebra of extremes.
    - `src/lib.rs::compute_margin_bottom_out()` and `src/lib.rs::compute_first_placed_empty_margin_bottom()` — empty/internal collapse and outgoing bottom.
  - Notes:
    - BFC detection improved: `overflow != visible`, `float != none`, `position != static`, and `display: flex/inline-flex` establish a new BFC for collapsing logic.
    - Structural emptiness refined: boxes that establish a BFC are not considered empty for internal top/bottom propagation; propagation does not cross BFC boundaries.
    - Clearance interactions now align for BFC-boundary cases; complex multi-float scenarios remain [MVP] in the floats model.
  - Fixtures:
    - `crates/css/modules/box/tests/fixtures/layout/basics/03_margin_collapsing.html`
    - `crates/css/modules/box/tests/fixtures/layout/box/margin_collapse_basic.html`
    - `crates/css/modules/box/tests/fixtures/layout/box/margin_collapse_border_top.html`
    - `crates/css/modules/box/tests/fixtures/layout/box/margin_collapse_empty_block.html`
    - `crates/css/modules/box/tests/fixtures/layout/box/margin_collapse_negative_last_bottom.html`
    - `crates/css/modules/box/tests/fixtures/layout/box/margin_collapse_nested.html`
    - `crates/css/modules/box/tests/fixtures/layout/box/margins_padding_borders.html`

- 9.4.1 Block formatting context — CSS 2.2
  - Status: [x] [Production]
  - Spec: https://www.w3.org/TR/CSS22/visuren.html#block-formatting
  - Code:
    - `src/orchestrator/mod.rs::layout_root_impl()` — entry for BFC layout.
    - `src/lib.rs::layout_block_children()` — leading group + placement loop.
    - `src/lib.rs::place_block_children_loop()` — sibling iteration and propagation.
    - `src/lib.rs::compute_clearance_floor_for_child()` — clears ignore external float floors when the child establishes a BFC.
    - `src/lib.rs::compute_float_bands_for_y()` — horizontal avoidance bands; masked when the parent establishes a BFC.
    - `src/lib.rs::build_parent_edge_context()` — determines parent-edge collapsibility using real box sides and BFC establishment.
  - Notes:
    - BFC boundary nullifies influence of external floats vertically (clearance floors) and horizontally (avoidance bands).
  - Fixtures:
    - All block layout fixtures under `crates/css/modules/box/tests/fixtures/layout/**`.
    - `crates/valor/tests/fixtures/layout/clearance/04_clear_left_inside_bfc_ignores_external_floats.html`.

- 9.4.3 Relative positioning — CSS 2.2
  - Status: [x] [MVP]
  - Spec: https://www.w3.org/TR/CSS22/visuren.html#relative-positioning
  - Code:
    - `src/lib.rs::apply_relative_offsets()` — applied in `src/lib.rs::prepare_child_position()`.
  - Notes:
    - [MVP] Absolute/fixed/sticky not implemented.
  - Fixtures:
    - N/A in current fixture set (add targeted relative-position tests).

- 9.5 Floats (clearance and avoidance) — CSS 2.2
  - Status: [x] [Production] for clearance floors and block-level horizontal avoidance bands; [ ] [MVP] for full float geometry/line wrapping.
  - Spec: https://www.w3.org/TR/CSS22/visuren.html#floats
  - Code:
    - `src/lib.rs::update_clearance_floors_for_float()` — per-side floors from float bottom + positive `margin-bottom`.
    - `src/lib.rs::compute_clearance_floor_for_child()` — floor selection for `clear: left|right|both` with BFC suppression.
    - `src/lib.rs::compute_float_bands_for_y()` — horizontal avoidance bands (left/right) from prior floats overlapping the child’s y.
    - `src/lib.rs::prepare_child_position()` — applies bands and raises to clearance floor when `clear != none`.
  - Notes:
    - [Approximation] Bands are per-y maxima per side; full shape/outside wrap and IFC line interaction are planned.
    - [Fallback] `style_engine::apply_float_clear_overrides_from_sheet()` also accepts `overflow`/`float`/`clear` from simple `#id { ... }` rules for fixtures until core exposes these properties in computed styles.
  - Fixtures:
    - `crates/valor/tests/fixtures/layout/clearance/01_clear_left_after_float_left.html`
    - `crates/valor/tests/fixtures/layout/clearance/02_clear_right_after_float_right.html`
    - `crates/valor/tests/fixtures/layout/clearance/03_clear_both_after_left_and_right_floats.html`
    - `crates/valor/tests/fixtures/layout/clearance/04_clear_left_inside_bfc_ignores_external_floats.html`
  - [TODO] Additional fixtures to add for coverage:
    - Stacked left floats (left + left) and a block below with `clear: left`.
    - Stacked right floats (right + right) and a block below with `clear: right`.
    - Mixed left/right stacking and blocks with `clear: both`.
    - Nested BFCs with internal floats that must not influence outer siblings.

- 10.3.3 Block-level, non-replaced elements in normal flow — CSS 2.2
  - Status: [x] [Production]
  - Spec: https://www.w3.org/TR/CSS22/visudet.html#blockwidth
  - Code:
    - `src/visual_formatting/horizontal.rs::solve_block_horizontal()` — used width and margins.
    - `src/lib.rs::prepare_child_position()` — reduces available width by float-avoidance bands and shifts x by left band.
  - Fixtures:
    - `crates/css/modules/box/tests/fixtures/layout/box/margins_padding_borders.html`
    - `crates/valor/tests/fixtures/layout/clearance/0{1,2,3}_*.html` (width reduction past floats).

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

---

## Additional formatting contexts and models (planned)

The following sections track work beyond the current MVP block layout. Each entry follows the same Status/Spec/Code/Notes/Fixtures format and will be promoted as work lands.

### Inline formatting context (IFC)

- Status: [TODO]
- Spec: CSS 2.2 §9.4.2 inline formatting context; CSS Text Level 3 (whitespace, line breaking): https://www.w3.org/TR/css-text-3/
- Code:
  - Planned: `src/inline/mod.rs`, `src/inline/line_builder.rs`, integration hooks in `src/lib.rs` to produce line boxes and baselines; text shaping via a text subsystem.
- Notes:
  - Current layouter performs display flattening and block-only placement. No line boxes, bidi, or baseline alignment yet.
- Fixtures:
  - `crates/css/modules/text/tests/fixtures/layout/inline/**` (to be added): whitespace collapsing, soft wraps, bidi reordering, inline-block baseline alignment.

### Floats formatting model

- Status: [TODO]
- Spec: CSS 2.2 §9.5 Floats, clearance, interaction with normal flow: https://www.w3.org/TR/CSS22/visuren.html#floats
- Code:
  - Planned: `src/floats/mod.rs` (float placement, float area), integration with block placement loop (`place_block_children_loop()`), clearance computation.
- Notes:
  - Will upgrade the Clearance section to [Production] once float placement and multi-float interactions are implemented and tested.
- Fixtures:
  - `crates/css/modules/box/tests/fixtures/layout/float/**` (to be added): left/right floats, stacked floats, negative margins around floats, clearance with nested BFCs.

### Positioning (absolute, fixed, sticky)

- Status: [TODO] (relative is MVP)
- Spec: CSS 2.2 positioning: https://www.w3.org/TR/CSS22/visuren.html#positioning-scheme; Sticky positioning: https://www.w3.org/TR/css-position-3/
- Code:
  - Planned: `src/positioning/mod.rs` (containing block resolution, offset calculations), stacking context interaction.
- Notes:
  - Requires scroll/viewport containers and interaction with overflow/clip for fixed/sticky.
- Fixtures:
  - `crates/css/modules/position/tests/fixtures/layout/**` (to be added): abs offset resolution, fixed to viewport, sticky constraints in nested scrollers.

### Flexbox formatting context

- Status: [TODO]
- Spec: CSS Flexible Box Layout Module Level 1: https://www.w3.org/TR/css-flexbox-1/
- Code:
  - Planned: `src/flex/mod.rs` with main/cross-axis layout, min-size:auto behavior, percentage resolution, intrinsic sizing; integrated into child collection.
- Notes:
  - `style_engine::ComputedStyle` already carries flex properties; layouter currently treats flex containers as blocks.
- Fixtures:
  - `crates/css/modules/flexbox/tests/fixtures/layout/**` (to be added): row/column, wrap, min-size:auto, align/justify variants.

### Replaced elements sizing

- Status: [TODO]
- Spec: CSS 2.2 §10. replaced elements sizing; CSS Images and object-fit/aspect-ratio (subset): https://www.w3.org/TR/css-images-3/, https://www.w3.org/TR/css-sizing-3/#aspect-ratio
- Code:
  - Planned: `src/replaced/mod.rs` for intrinsic width/height, aspect-ratio, min/max constraints.
- Notes:
  - Needed for images/video and for accurate percentage/min-max interaction.
- Fixtures:
  - `crates/css/modules/images/tests/fixtures/layout/replaced/**` (to be added): intrinsic size, aspect-ratio, percentage constraints.

### Overflow, clipping, and scrolling

- Status: [TODO]
- Spec: CSS Overflow Module Level 3: https://www.w3.org/TR/css-overflow-3/
- Code:
  - Planned: clipping rectangles in display list; scroll container metrics and coordinate space; integration with hit testing.
- Notes:
  - Current fixtures include `overflow: hidden` basics; no scrollers or scrollbars yet.
- Fixtures:
  - `crates/css/modules/display/tests/fixtures/layout/clip/**` (existing basics) and `.../scroll/**` (to be added): nested scrollers, sticky + overflow, clip/contain interactions.
  - Module ownership for current diffs: these belong to the `display` module (`crates/css/modules/display/`).

### Stacking contexts and painting order (layout interplay)

- Status: [TODO]
- Spec: CSS 2.2 painting order; CSS Transforms/Opacity creating stacking contexts: https://www.w3.org/TR/CSS22/zindex.html
- Code:
  - Planned: layer/stacking metadata in layout outputs to inform painting/compositing.
- Notes:
  - Required for correct z-index ordering with positioned/opacity/transform.
- Fixtures:
  - `crates/css/modules/transforms/tests/fixtures/layout/stacking/**` (to be added).

 

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
- Clearance and floats are partial [MVP] — production once floats are complete.

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

---

## Verbatim Spec Appendix (optional)

Legal notice (required if embedding spec text):

```
$name_of_software: $distribution_URI
Copyright © [$year-of-software] World Wide Web Consortium. All Rights Reserved. This work is distributed under the W3C® Software and Document License [1] in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
[1] https://www.w3.org/Consortium/Legal/copyright-software
```

Begin embedded normative text below (exclude Abstract, Status, general Introduction). Keep chapters in spec order, and clearly indicate the source spec version and URL.
