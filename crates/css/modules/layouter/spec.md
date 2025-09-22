# Layouter — Spec-Oriented Module Overview

Spec: <https://www.w3.org/TR/CSS22/visuren.html#block-formatting>

## Modules (by CSS 2.2 section)
- `horizontal.rs` — §10.3.3. Width and horizontal margin resolution for non-replaced blocks.
- `vertical.rs` — §8.3.1. Vertical margin collapsing (leading group, parent–first-child/sibling behavior).
- `height.rs` — §10.6. Used height wrapper that delegates to the layouter’s implementation.
- `root.rs` — Root-level helpers (parent–first-child top-margin collapse placement).
- `box_tree.rs` — Display tree flattening and block child enumeration used by layout.
- `sizing.rs` — Used-size helpers consumed by width/height computations.

## Implemented
- §9.4.1 helpers (first/last block under a node, shallow navigation) are in `lib.rs` with spec notes.
- §10.3.3 width: specified/auto width paths, auto margin resolution, min/max in border-box space.
- §10.6 height: content-based height + padding/border with a default line-height fallback when empty.
- §8.3.1 vertical collapsing: leading group pre-scan, ancestor-aware application at parent edge vs forwarding.
- Relative positioning (basic): `top/left/right/bottom` offsets applied to final rects.

## Planned
- Inline formatting contexts and line boxes.
- Floats/positioned layout and BFC creation rules.
- Percentage resolution and replaced elements.
- Performance passes and caching.

## Testing and comparisons
- Fixtures under `crates/valor/tests/fixtures/layout/**` are auto-discovered.
- To XFAIL a fixture, include the substring `VALOR_XFAIL` (case-insensitive).
- Chromium comparer runs with one Tokio runtime/tab for speed; it compares border-box rects and a subset of computed styles.

## Notes
- Box sizing: conversions are in border-box space; see `sizing.rs` and spec §8.1/§10.1.
- `LayoutRect` is border-box; comparisons align with Chromium’s `getBoundingClientRect()`.
