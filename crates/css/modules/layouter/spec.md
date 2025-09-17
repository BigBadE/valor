# Layouter — Module Spec Checklist

Spec: https://www.w3.org/TR/CSS22/visuren.html#block-formatting

## Scope
- Minimal external layouter used by tests as a mirror of the page’s DOM structure and attributes.
- Provides a stub block formatting flow sufficient for fixtures bootstrapping and diagnostics.
- Geometry from this external layouter is NOT authoritative; Chromium comparer reads geometry from the page’s internal layouter.

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
  - [~] Simple sibling margin collapsing (previous-bottom vs current-top via max) for direct children of the root container
- [ ] 9.5 Floats
- [ ] 9.4.3 Relative positioning offsets

## Parsing/Inputs
- Inputs come from `StyleEngine` computed snapshot: `ComputedStyle` fields include `display`, `margin`, `padding`, and limited flex-related fields.
- DOM structure is provided via mirrored `InsertElement` updates; attributes subset tracked: `id`, `class`, `style`.

## Algorithms/Overview
- Naive block layout:
  - Traverse first block descendant under `NodeKey::ROOT` (typically `html`/`body`), prefer `body` when `html`.
  - Compute container content width (ICB width minus container margin/border/padding) and emit a root rect.
  - Stack direct element children vertically.
  - X = container content start + margin-left; Y accumulates with simple sibling margin collapsing (max of previous bottom vs current top).
  - Width = container content width minus horizontal margins; height = 0 (content-size not computed in MVP).

## Caching/Optimization
- None in MVP. Counters are recorded for diagnostics only.

## Integration
- Upstream: `style_engine` for `ComputedStyle` and stylesheet.
- Downstream: test harnesses `layouter_snapshot_smoke.rs` and `layouter_chromium_compare.rs` consume structure, attrs, and counters. Chromium comparer reads geometry from the page’s internal layouter via `HtmlPage::layouter_geometry_mut()`. The external layouter’s geometry is for diagnostics and spec mapping.

## Future work
- Implement margin-collapsing and over-constrained width resolution per CSS 2.2.
- Add inline formatting contexts and line box construction.
- Introduce absolute/relative positioning, floats, and BFC creation rules.
- Replace fixed viewport fallback with actual initial containing block sizing and percent resolution.
