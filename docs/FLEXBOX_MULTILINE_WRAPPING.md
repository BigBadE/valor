# Flexbox Multi-line Wrapping — MVP Plan

Status: Draft (planning) — Targeting Flexbox §9 line breaking and packing for multi-line containers.

Primary spec: https://www.w3.org/TR/css-flexbox-1/#line-breaking

## Goals (Phase 1)

- Non-wrapping remains default; multi-line activates only when `flex-wrap` is enabled (future wiring).
- Implement line breaking for `row` (HorizontalTb) with CSS gaps.
- Per-line main-axis layout reuses existing single-line path (sizes, justify, gaps).
- Compute per-line cross-size; container cross-size is max (single-line) or sum (multi-line) depending on direction and alignment rules (MVP: sum for wrap onto multiple rows).

## Data model additions (proposal)

- `FlexLine` (internal):
  - `items: Vec<FlexChild>` — item subset per line (stable input order preserved).
  - `main_total: f32` — sum of used main sizes (post grow/shrink) for the line.
  - `cross_max: f32` — maximum cross size amongst items in the line.

- `MultiLineResult` (internal):
  - `lines: Vec<FlexLinePlacement>` — each with per-item `FlexPlacement` plus `CrossPlacement` post per-line cross alignment.
  - `container_cross_used: f32` — computed cross extent.

No public API surface change required initially; core driver continues to consume `(FlexPlacement, CrossPlacement)` pairs.

## Algorithm sketch

1) Collection and sizing
   - Start from `FlexChild[]` and container inputs (direction, writing mode, container main size, main gap).
   - Measure/clamp `flex_basis` → hypothetical sizes (as today).

2) Line breaking
   - Iterate items, accumulating `cursor + size + gap_if_needed <= container_main_size`.
   - When next item would overflow, break line, start new line with `cursor` reset.
   - Include CSS `gap` only between adjacent items inside a line.

3) Per-line flexing
   - For each line independently, compute free space considering gaps and run grow/shrink (as in single-line).
   - Compute per-item main offsets using `accumulate_main_offsets` for the line content length.

4) Per-line cross-size
   - Cross for each item is derived from inputs (min/max clamped). Track `line.cross_max = max(items.cross_size)`.
   - MVP: Align items within line cross using `align_single_line_cross` with `container_cross=line.cross_max`.

5) Container cross-size
   - MVP: Sum of `line.cross_max` across lines.
   - Container main-size is given/definite (as today for Row HorizontalTb); Column TBD later.

6) Line packing (between-lines)
   - MVP: Pack at cross-start (no extra spacing between lines). Future: distribute per `align-content`.

## Integration

- New internal helper `layout_multi_line(...)` that returns all placements.
- Core writer unchanged: receives a flattened vector of placements in document order.
- Wrap activation controlled by style (`flex-wrap`) once wired; until then, test-only entry points may be used to validate algorithm.

## Testing plan

- Fixtures:
  - wrap-basic: 3×50px items, container 120px, row wrap with 10px gap → lines: [2 items], [1 item].
  - wrap-justify: same but with `justify-content: space-between` per line.
  - wrap-cross: different item heights; verify per-line cross max and vertical stacking.
- Use `.fail` markers until feature is wired; green once enabled.

## Future phases

- `align-content` across lines (space-between/around/evenly, stretch).
- Baseline alignment across lines.
- Column/column-reverse support and vertical writing modes.
- Auto margins interaction across lines.
- Improved fragmentation/overflow coordination.
