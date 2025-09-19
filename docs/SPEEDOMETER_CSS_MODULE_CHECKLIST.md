# CSS Modules Completion Checklist for Speedometer

This is the recommended order to bring up CSS functionality to run Speedometer. Each step is small and shippable, with clear acceptance criteria.

## Phase A — Core pipeline and style basics
- [x] CSS orchestrator and Core glue
  - DOM updates → selectors/cascade → computed style → layout
  - Orchestrator returns: computed styles, node snapshot, rects, dirty rects
  - Invalidation for attribute/class/style changes
- [X] css_syntax + values/units parsing
  - Identifiers, numbers, percentages, px/em/rem, colors (hex/rgb[a]/basic names)
  - Tokenizer and basic error recovery
- [x] css_selectors
  - Type, class, id, attribute [attr=value], descendant/child/sibling combinators
  - Specificity and match caching on element changes
- [x] css_cascade
  - UA < user < author; importance; specificity; source order
  - Inheritance for core properties (font-size/family, color)
- [x] css_style_attr
  - Map style="" to inline declarations with author origin
  - Merge into cascade path
- [x] css_variables
  - MVP var() with fallback, inheritance, basic cycle detection
- [x] css_values_units
  - Compute normalized values: lengths (px/em/rem), percentages (defer where needed), colors, keyword enums
  - Font-size scaling (em/rem resolution)

## Phase B — Computed model and basic layout
- [x] Computed style model (core.style)
  - display, position, z-index, overflow
  - margin/padding/border widths/styles/colors
  - font-size, font-family
  - flex-basis/grow/shrink, align-items, justify-content, flex-direction, flex-wrap
  - width/height/min/max, box-sizing
- [ ] css_display + box model
  - [ ] Skip non-rendered nodes (display:none) and lift children for display:contents (helper in place; disabled pending fixtures)
  - [ ] Build formatting tree
  - [ ] Block/inline basics, whitespace collapse, simple inline flow boxes
- [ ] css_sizing
  - Percent width/height in block/flex contexts, min/max constraints

## Phase C — Flex and positioning
- [ ] css_flexbox
  - Main axis layout (basis, min/max, grow/shrink, auto margins)
  - Cross axis alignment (align-items/justify-content: start/center/end/stretch)
  - Wrapping optional (non-wrapping first)
- [ ] css_position
  - Static, relative (offsets), absolute (MVP: nearest positioned ancestor)

## Phase D — Responsiveness and text basics
- [ ] css_media_queries
  - min-width/max-width, orientation; re-evaluate on viewport resize
- [ ] css_text
  - Whitespace collapsing, naïve line breaking, bidi passthrough

## Phase E — Visual polish (subset)
- [ ] css_color + css_backgrounds_borders (subset)
  - background-color, border-color/width/style sufficient for diffs
- [ ] css_fonts (subset)
  - font-family parsing + generic fallback; @font-face not required

## Cross-cutting
- [x] Dynamic invalidation & CSSOM updates
  - Attribute/class toggles, inline style changes, stylesheet replace/append
  - Restyle + layout dirtiness propagation
- [x] Test harness per-module
  - Module fixtures under `tests/fixtures/` (auto-discovered)
  - Chromium compare for layout modules (display, flexbox, sizing, position)

## Suggested rollout
- Phase A: syntax + selectors + cascade + style_attr + values_units + variables + computed model → render simple blocks
- Phase B: display + box + sizing → static layouts
- Phase C: flexbox → modern app UIs
- Phase D: media queries + invalidation → responsive/app flows
- Phase E: position MVP + visual polish
