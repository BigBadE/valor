# CSS Box Model — Module Spec Checklist

Spec: <https://www.w3.org/TR/css-box-3/>

## Scope (MVP for Speedometer)
- Represent and compute the core box model values needed for static block layouts:
  - margin, padding, border widths/styles/colors
  - box-sizing and its impact on used width/height
- Exclude advanced painting and corners for MVP.

Out of scope (for now):
- Border-radius, box-shadow
- Backgrounds and painting details (tracked in `css_backgrounds_borders`)
- Margin-collapsing edge cases (multi-block flow specifics)

## Checklist (mapping to code)
- [x] Computed style model fields present
  - [x] `crates/css/modules/core/src/style_model.rs`: margin/padding/border, box-sizing, width/height/min/max
- [x] Used value computation helpers (MVP)
  - [x] `compute_box_sides()` — `crates/css/modules/box/src/lib.rs`
- [x] Integration with layout
  - [x] `layouter` consumes resolved box metrics in block layout via `compute_box_sides()`
  - [x] Hooks present for margin-collapsing (pairwise) via `collapse_margins_pair()`
  - [x] Parent content origin for block layout (margin/border/padding accounted)
  - [x] Root top margin collapsing with first child when no padding/border-top
  - [x] Auto height from children: span of child border-boxes; add container padding/border
  - [x] Negative horizontal margins affect x-position

## Chapter mapping (CSS Box Model L3 and related CSS 2.2 sections)

- [x] § Box sizing (L3)
  - [x] Used width in border-box space respecting `box-sizing`
    - `crates/css/modules/layouter/src/sizing.rs::used_border_box_width()`
  - [x] Used height in border-box space respecting `box-sizing`
    - `crates/css/modules/layouter/src/sizing.rs::used_border_box_height()`
  - [x] Conversion between content-box/border-box for min/max/used
    - Implemented inside the above helpers

- [x] § Padding edge sizes (CSS 2.2 §8.1 referenced)
  - [x] Padding contributes to border-box and reduces child content width
    - `crates/css/modules/layouter/src/lib.rs::compute_container_metrics()`
    - `crates/css/modules/layouter/src/lib.rs::layout_one_block_child()` (child content width)

- [x] § Border widths (CSS 2.2 §8.1 referenced)
  - [x] Border contributes to border-box and reduces child content width
    - `crates/css/modules/layouter/src/lib.rs::compute_container_metrics()`
    - `crates/css/modules/layouter/src/lib.rs::layout_one_block_child()`

- [x] § Margins (CSS 2.2 §8.3)
  - [x] Horizontal margins affect x-position and available width
    - `crates/css/modules/layouter/src/lib.rs::layout_one_block_child()`
  - [x] Vertical margin collapsing (pairwise MVP)
    - `crates/css/modules/layouter/src/lib.rs::collapse_margins_pair()` and callers
  - [x] Root/parent top collapsing with first child when no padding/border-top
  - [ ] Full set of collapsing cases (empty blocks, nested, multi-way)

- [ ] § Logical properties (inline/block start/end) mapping
  - Pending; using physical `top/right/bottom/left` only

- [ ] § Percentages and auto
  - [ ] Percentage margins/padding resolution
  - [ ] Auto margins for block formatting (defer; flexbox will handle its own auto margins)

- [ ] § Border styles and colors (integration with painting)
  - Widths/styles/colors are carried in `ComputedStyle` but rendering is out-of-scope here

## Parsing/Inputs overview
- Parsing for lengths and percentages delegated to `css_values_units`.
- Box model properties enter through cascade/computed style in `css_core`; this module focuses on normalization/used-value helpers consumed by `layouter`.

## Algorithms overview (MVP subset)
- Used margin/padding/border widths:
  - Resolve each longhand to px using `values_units` and font/viewport context as needed.
- Box sizing:
  - `content-box`: used width/height exclude padding and border
  - `border-box`: used width/height include padding and border

## Integration
- Upstream:
  - `css_core` provides `ComputedStyle` with box model fields.
  - `css_values_units` resolves numeric values.
- Downstream:
  - `layouter` consumes margins/padding/borders and box-sizing in:
    - `sizing::used_border_box_width()` / `sizing::used_border_box_height()`
    - `compute_container_metrics()` (container padding/border)
    - `layout_one_block_child()` (positions, available widths, recursion with content metrics)
  - Future: a `css_box` helper layer can centralize used-value resolution before layout.

## Future work
- Margin collapsing rules for adjacent block-level siblings (beyond pairwise).
- Integration with backgrounds/borders painting order.
- Border styles beyond width/color (e.g., dotted/dashed rendering; can be deferred to painting).
- Logical properties (inline-start/end, block-start/end) mapping to physical sides.

## Test status (fixtures)
- Passing:
  - `box/margin_collapse_basic.html`
  - `box/margins_padding_borders.html`
