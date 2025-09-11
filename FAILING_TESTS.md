Failing Tests Report for Valor

Date: 2025-09-10 20:27 (local)

Summary
- Command executed: cargo test --workspace -- --nocapture
- Total failing test targets: 1 (in crate: valor)
- Total passing test targets: all others across crates css, html, js, js_engine_v8, layouter, page_handler, style_engine
- Details: The single failing target is a comparison harness that runs many HTML layout fixtures against headless Chromium. After recent fixes, only 3 fixtures currently mismatch.

Failing Test Target
1) Crate: valor
   Test target: tests/layouter_chromium_compare.rs
   Test name: chromium_layout_test
   Reproduction: cargo test -p valor --test layouter_chromium_compare -- --nocapture

Per-Fixture Failures (from test log)
- crates/valor/tests/fixtures/layout/flex/06_flex_row.html
  Symptom: main-axis position/width mismatch for first item (#a). Expected x=0 and specified widths honored.
- crates/valor/tests/fixtures/layout/flex/07_flex_grow.html
  Symptom: flex grow distribution does not match Chromium’s computed widths for (1 1 0px, 2 1 0px, 1 1 0px).
- crates/valor/tests/fixtures/layout/flex/08_flex_shrink.html
  Symptom: shrink distribution for three 300px-basis items does not match Chromium.

Fixed Since Previous Report (removed from list)
- basics/01_auto_width.html — resolved by stabilizing Chrome device metrics and aligning layouter viewport width.
- basics/02_percent_width.html — same as above.
- basics/03_margin_collapsing.html — same as above.
- basics/05_display_none.html — same as above.
- inline/04_block_inline_partition.html — same as above.
- positioning/09_absolute_position.html — same as above.
- positioning/10_fixed_position.html — same as above.
- overflow/11_overflow_clipping.html — resolved by reporting used sizes in geometry output (clipping deferred to paint).

Updated Diagnosis for Remaining Failures
Flex layout discrepancies (3 fixtures)
- What we verified/changed:
  - Added parsing for the flex shorthand (flex: <grow> <shrink> <basis>) in style_engine, so flex-basis/grow/shrink should now be set from shorthand declarations.
  - Skipped whitespace-only text nodes when building flex item lists to avoid stray zero-width items.
  - Confirmed container content width is passed to the flex algorithm as content_width (padding/margins excluded).
- Likely causes now:
  1) Remaining math differences in free-space distribution and rounding. Our algorithm rounds per-item and then adjusts the last item; Chromium may accumulate fractional free space differently (e.g., distributing remainders left-to-right or using spec’s “flex factor” with clamping loops).
  2) Min/max constraints interaction: After shrink clamping, our final correction only adjusts the last item and may still over/underflow compared to Chromium’s iterative re-freezing algorithm.
  3) Initial main-axis offset (case 06_flex_row): If any non-zero justify-content default or left padding slips in, x for #a could be offset. We default justify-content to flex-start, but we should explicitly ensure no unintended left gaps are added when the sum of item sizes is less than container width.

Next Steps to Fix
1) Align rounding and distribution with spec steps:
   - Implement iterative “freezing” per CSS Flexbox spec: distribute, clamp to min/max, freeze clamped items, recompute free space, and iterate until convergence, with fractional handling deferred to final step to reduce bias.
   - Apply remainder distribution deterministically (e.g., to first unfrozen items) matching Chromium’s behavior.
2) Add targeted assertions in tests to dump computed flex properties and the intermediate sizes for these fixtures to pinpoint where divergence starts (basis vs grow vs rounding vs clamping).
3) Verify justify-content default is flex-start and ensure no container padding/margins or gaps are included unless specified.

Notes
- All other tests across the workspace currently pass.
- The comparison harness now launches Chrome with --force-device-scale-factor=1 and --disable-features=OverlayScrollbar to stabilize CSS pixel metrics; the layouter viewport is aligned to the observed clientWidth to avoid constant width deltas.
