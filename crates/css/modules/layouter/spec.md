### Selector support note (tests reset)

- We apply a small CSS reset in layout tests that relies on the universal selector `*` to set `box-sizing: border-box`.
- The selector engine in `crates/css/modules/core/` now supports the universal selector per Selectors Level 4 §2.2 (https://www.w3.org/TR/selectors-4/#universal-selector), so rules like `*, *::before, *::after { box-sizing: border-box; }` match elements.
- Pseudo-elements are not yet implemented; the universal selector still applies to real elements, which is sufficient for the reset to affect layout geometry in tests.
# Layouter — Module Spec Checklist

Spec: https://www.w3.org/TR/CSS22/visuren.html#block-formatting

## Scope
- Authoritative layouter for the Valor page pipeline, wired into `HtmlPage` via `DOMMirror<Layouter>`.
- Provides a pragmatic block formatting flow sufficient for initial fixtures, display list generation, and diagnostics.
- Geometry emitted by this layouter is consumed by the renderer and tests; it is the single source of truth for layout in-page.
- The layouter relies on the universal selector `*` for CSS resets in tests, supported by the selector engine in `crates/css/modules/core/` per Selectors Level 4 §2.2 (https://www.w3.org/TR/selectors-4/#universal-selector).

Out of scope for this phase:
- Inline formatting contexts, line boxes, text shaping.
- Floats, positioning, multi-column, fragmentation.
- Flex/grid layout.
- Percentage resolution, min/max constraints, replaced elements.

## Checklist (mapping)
- [x] 9.4.1 Block formatting context basics — establish vertical flow and box tree
  - [x] `Layouter::apply_update()` builds element tree from DOM updates
  - [x] `Layouter::snapshot()` returns stable snapshot of node kinds and children
- [x] 10.3/10.6 Block width/height determination (MVP simplification)
  - [x] `Layouter::compute_layout()` stacks direct children of the chosen root; computes basic content width from container minus horizontal padding/borders (margins are excluded per §8.1/§10.1)
  - [x] Treat `auto` sizes as fill-available; heights default to content-size 0 in MVP
- [~] 10.3.3 Width: over-constrained resolution
  - [~] Basic horizontal accounting (margin/padding/border) for width; full over-constraint resolution is TODO
- [x] 8.3 Margin collapsing
  - [x] Pairwise vertical margin collapsing for direct-flow blocks (handles positive/negative and mixed signs)
  - [x] Parent–first-child top-margin collapse applied when the parent has no top border/padding; collapsed positive margin offsets the parent’s Y and is excluded from the parent’s used height
- [ ] 9.5 Floats
- [x] 9.4.3 Relative positioning offsets (basic)
  - [x] Apply `top/left/right/bottom` adjustments when `position: relative`

## Parsing/Inputs
- Inputs come from `StyleEngine` computed snapshot: `ComputedStyle` fields include `display`, `margin`, `padding`, and limited flex-related fields.
- DOM structure is provided via mirrored `InsertElement` updates; attributes subset tracked: `id`, `class`, `style`.

## Algorithms/Overview
- Naive block layout:
  - Traverse first block descendant under `NodeKey::ROOT` (typically `html`/`body`), prefer `body` when `html`.
  - Compute container content width (ICB width minus container padding/border; margins are outside and do not reduce the containing block) and emit a root rect.
  - Stack direct element children vertically.
  - X = container content start + margin-left; Y accumulates with collapsed vertical margins (pairwise logic for previous-bottom vs current-top). For the root, apply parent–first-child top-margin collapse by offsetting Y and excluding that amount from the root’s used height.
  - Width = container content width minus horizontal margins (with min/max). For `width: auto` blocks, resolve to fill-available content width per §10.3.3. Transparent empty boxes preserve this width even when their height collapses to 0.
  - Height = used border-box height per Sizing rules below (auto height clamps with min/max in MVP).
  - Spec references (CSS 2.2):
    - Block width computation: §10.3.3 (over-constrained resolution), §10.3.1 (non-replaced block elements in normal flow)
    - Block height computation: §10.6.3 (non-replaced elements in normal flow), with simplifications
    - Box model and margins/borders/padding: §8.1, §8.3 (margin collapsing)
  - Apply `position: relative` offsets to computed x/y.

## Sizing
Implementation highlights (see `crates/css/modules/layouter/src/lib.rs`):
- `choose_layout_root()` selects the first block under `#document`, preferring `body` under `html`.
- `compute_container_metrics()` derives padding/border/margin from the root style and computes available content width as ICB width minus padding/border (margins excluded; CSS 2.2 §8.1, §10.1).
- `sizing.rs` implements used-size helpers that follow Box Sizing L3 rules; `layout_block_children()` calls them.
- `collapse_margins_pair()` implements two-value vertical margin collapsing (positive/negative/mixed rules).
- `apply_relative_offsets()` adjusts x/y for `position: relative` using `top/left/right/bottom`.

## Caching/Optimization
- None in MVP. Counters are recorded for diagnostics only.

## Integration
- Upstream: `style_engine` for `ComputedStyle` and stylesheet.
- In-page wiring: `HtmlPage` constructs `DOMMirror<Layouter>` and drives layout during updates (see `crates/page_handler/src/state.rs`).
  - Layout geometry and snapshots are accessed via `HtmlPage` helpers like `layouter_geometry_mut()` and `layouter_snapshot()`.
  - Renderer consumes layouter geometry to build retained display lists (see `crates/page_handler/src/display_api.rs`).
- Downstream tests: `layouter_snapshot_smoke.rs`, `layouter_chromium_compare.rs`, and graphics tests use the layouter’s geometry and structure via the page API.

## Sizing (Box Sizing L3)

- Source: CSS Box Sizing Module Level 3 — `box-sizing`.
  - content-box: width/height apply to the content box.
  - border-box: width/height include padding and border.
- CSS 2.2 §8.1 Box model: padding and border are outside the content box.

### Used border-box width
- Inputs: `ComputedStyle` (width/min/max, padding, border, box-sizing) and fill-available border-box width (container width minus horizontal margins).
- Steps:
  1. Convert specified width to border-box space:
     - content-box: specified + horizontal padding + border.
     - border-box: specified as-is.
  2. Convert min/max similarly to border-box space.
  3. Start from specified border-box width, or fill-available width if `auto`.
  4. Clamp with min/max (in border-box space).
  5. Clamp to fill-available width and >= 0.

### Used border-box height
- Inputs: `ComputedStyle` (height/min/max, padding, border, box-sizing).
- Steps:
  1. Convert specified height to border-box space (as above).
  2. Convert min/max similarly.
  3. If specified height is present: use it (clamped by min/max in border-box space).
  4. If `auto`: compute content-based height + padding + border; if empty but has visible inline content, use a default line-height baseline; then clamp with min/max in border-box space, and >= 0.

Notes:
- Correctness depends on `ComputedStyle.box_sizing` reflecting author styles (from CSS core). Inline `box-sizing` is respected via overrides; author/UA styles require core support.

### Rect model (border-box)
- `LayoutRect` represents the border-box. Chromium-side JSON captures `getBoundingClientRect()`, which is also border-box. Equality checks should therefore compare border-box values.

### Defaults normalization
- Border defaults: if `border-style: solid` and a side’s width is unspecified, default that side to `3px`.
- Flex defaults: if `flex-shrink` is unspecified, default to `1` (Chromium default) for comparison stability.

## Future work
- Implement margin-collapsing and over-constrained width resolution per CSS 2.2.
- Add inline formatting contexts and line box construction.
- Introduce absolute/relative positioning, floats, and BFC creation rules.
- Replace fixed viewport fallback with actual initial containing block sizing and percent resolution.
