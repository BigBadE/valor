# CSS Values & Units Module Level 3 — Implementation Checklist

Spec: <https://www.w3.org/TR/css-values-3/>

This document mirrors the Values & Units spec chapters and tracks implementation status in `crates/css/modules/values_units/`.

- [x] §4 Numbers
  - File: `chapter_4_numbers.rs`
  - API: `parse_number()`
- [x] §5 Percentages
  - File: `chapter_5_percentages.rs`
  - API: `parse_percentage()`
- [x] §6 Dimensions — Lengths (subset)
  - File: `chapter_6_dimensions.rs`
  - API: `parse_length()`, `compute_length_px()`
  - Units supported: `px`, `em`, `rem`, `vw`, `vh`
  - Notes: `em`/`rem` resolved via provided font sizes; viewport units require viewport size.
- [ ] §7 Calc (calc())
  - Pending: parsing and evaluation
- [ ] §8 Relative lengths (full set)
  - Pending: `ex`, `ch`, `cap`, `ic`, `lh`, `rlh`
- [ ] §9 Colors (moved to color spec)
  - Pending: integrate subset back lint-clean (`#rgb[a]`, `rgb()/rgba()`, named)
- [ ] §10 Angles
- [ ] §11 Times
- [ ] §12 Frequencies
- [ ] §13 Resolutions
- [ ] §14 Flex (fr) unit
- [ ] §15 Ranges

Integration hooks
- [x] Length normalization helper `compute_length_px()` that converts supported units into px given `font_size_px`, `root_font_size_px`, and optional `Viewport`.
