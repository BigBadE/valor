# Layouter — Module Spec Checklist

Spec: https://www.w3.org/TR/CSS22/visuren.html#block-formatting

## Scope
- Authoritative layouter for the Valor page pipeline, wired into `HtmlPage` via `DOMMirror<Layouter>`.
- Provides a pragmatic block formatting flow sufficient for initial fixtures, display list generation, and diagnostics.
- Geometry emitted by this layouter is consumed by the renderer and tests; it is the single source of truth for layout in-page.

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
  - [x] `Layouter::compute_layout()` stacks direct children of the chosen root; computes basic content width from container minus horizontal margins/padding/borders
  - [x] Treat `auto` sizes as fill-available; heights default to content-size 0 in MVP
- [~] 10.3.3 Width: over-constrained resolution
  - [~] Basic horizontal accounting (margin/padding/border) for width; full over-constraint resolution is TODO
- [~] 8.3 Margin collapsing
  - [~] Pairwise vertical margin collapsing for direct-flow blocks (handles positive/negative and mixed signs); container-top collapsing when no padding/border
- [ ] 9.5 Floats
- [x] 9.4.3 Relative positioning offsets (basic)
  - [x] Apply `top/left/right/bottom` adjustments when `position: relative`

## Parsing/Inputs
- Inputs come from `StyleEngine` computed snapshot: `ComputedStyle` fields include `display`, `margin`, `padding`, and limited flex-related fields.
- DOM structure is provided via mirrored `InsertElement` updates; attributes subset tracked: `id`, `class`, `style`.

## Algorithms/Overview
- Naive block layout:
  - Traverse first block descendant under `NodeKey::ROOT` (typically `html`/`body`), prefer `body` when `html`.
  - Compute container content width (ICB width minus container margin/border/padding) and emit a root rect.
  - Stack direct element children vertically.
  - X = container content start + margin-left; Y accumulates with collapsed vertical margins (pairwise logic for previous-bottom vs current-top).
  - Width = container content width minus horizontal margins (with min/max); declared width/height are treated as border-box sizes; auto height clamps with min/max.
  - Spec references (CSS 2.2):
    - Block width computation: §10.3.3 (over-constrained resolution), §10.3.1 (non-replaced block elements in normal flow)
    - Block height computation: §10.6.3 (non-replaced elements in normal flow), with simplifications
    - Box model and margins/borders/padding: §8.1, §8.3 (margin collapsing)
  - Apply `position: relative` offsets to computed x/y.

Implementation highlights (see `crates/css/modules/layouter/src/lib.rs`):
- `choose_layout_root()` selects the first block under `#document`, preferring `body` under `html`.
- `compute_container_metrics()` derives padding/border/margin and available content width from the root style.
- `resolve_used_border_box_width()` and `resolve_used_border_box_height()` implement simplified CSS 2.2 sizing with `box-sizing` and min/max.
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
