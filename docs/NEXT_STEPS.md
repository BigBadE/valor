# NEXT STEPS — Main Page HTML/CSS Feature Audit

This document audits the HTML/CSS features used by `assets/chrome/index.html` and maps them to current engine support in this codebase. It lists what is implemented, partial, or missing, and prescribes concrete next steps for each missing/partial item.

Sources reviewed:
- Page UI: `assets/chrome/index.html`, `assets/chrome/app.js`
- Engine model and layout:
  - `crates/css/orchestrator/src/style_model.rs`
  - `crates/css/modules/display/src/lib.rs`
  - `crates/css/modules/display/src/2_box_layout_modes/part_2_7_transformations.rs`
  - `crates/css/modules/core/src/**` (box model sizing/width/height/margin collapse helpers)
  - `docs/SPEEDOMETER_CSS_MODULE_CHECKLIST.md`
  - Stubs: `crates/css/modules/flexbox/src/lib.rs`, `crates/css/modules/position/src/lib.rs`, `crates/css/modules/fonts/src/lib.rs`

## Summary — Features used by the page

The main page uses:
- display: `block`, `flex`, `inline-block`
- Flexbox: `flex: 1`, `align-items: center`, `gap`
- Positioning: `position: fixed`
- `z-index`
- Box model: `box-sizing: border-box`, margin, padding, border (width/style/color), `border-radius`
- Dimensions: `width`, `height`, `min-width`, `height: 100%` (on `html, body`)
- Colors: `background`/`background-color`, `color`
- Fonts: `font` shorthand (`14px/1.4 -apple-system, …`) and `font: inherit` on inputs/buttons
- Pseudo selectors/elements: `:active`, `::placeholder`
- Misc: `cursor: pointer`, `user-select: none`, `transform: translateY(1px)`

## Status overview

- Implemented
  - display: `block`/`inline`/`none`/`contents` (Display 3 normalization and used-display mapping)
  - Box model: margin/padding/border (width/style/color), `box-sizing`
  - Dimensions: basic `width`/`height`/`min-*`/`max-*` (px) used-value computation
  - Colors: `color`, `background-color`
  - Typography: numeric `font-size`, `line-height` (numeric) parsing/computation
  - Flexbox [Production]: §§2–4 and §§7–9 — axes resolution; single- and multi-line layout (row/column); per-line `justify-content` (start/center/end/space-between/around/evenly); `gap` (row/column, px and %); `align-items` (stretch/center/start/end); `align-content` (start/center/end/space-between/around/evenly; stretch implemented); auto margins distribution (single- and multi-line); absolutely-positioned flex children placement (§4.1)

- Partial / Caveats
  - `box-sizing` edge cases (tracked in docs)
  - Percent/relative heights (e.g., `height: 100%` on root): Sizing semantics incomplete
  - `z-index`: parsed/stored; full stacking/painting order not finalized
  - Flexbox: overflow Hidden now clips at padding box and clamps content height; remaining: scroll/auto behaviors and advanced writing modes

- Missing / Not wired
  - Positioning: `position: fixed` (and abs/sticky)
  - `display: inline-block` semantics
  - `border-radius`
  - CSS transforms: `transform`
  - Pseudo-element `::placeholder`
  - Dynamic pseudo-class styling such as `:active`
  - `cursor`, `user-select`

## Evidence and references

- Model fields present for implemented/partial items: `crates/css/orchestrator/src/style_model.rs`
  - `display`, `position`, `z_index`, `overflow`, `margin`, `padding`, `border_*`, `box_sizing`, `width/height/min/max`, `font_size`, `line_height`, flex properties (present but not laid out), etc.
- Display normalization and used-value transform (blockification/inlinification):
  - `crates/css/modules/display/src/lib.rs`
  - `crates/css/modules/display/src/2_box_layout_modes/part_2_7_transformations.rs`
- Box/size helpers and layout bits (subset):
  - `crates/css/modules/core/src/10_visual_details/part_10_3_3_block_widths.rs`
  - `crates/css/modules/core/src/10_visual_details/part_10_6_3_height_of_blocks.rs`
  - `crates/css/modules/core/src/8_box_model/part_8_3_1_collapsing_margins.rs`
- Project roadmap and current completion:
  - `docs/SPEEDOMETER_CSS_MODULE_CHECKLIST.md` (Flexbox and Position unchecked; Sizing unchecked)
- Stubs indicating not implemented yet:
  - Position: `crates/css/modules/position/src/lib.rs`
  - Fonts (broader): `crates/css/modules/fonts/src/lib.rs`

## Detailed checklist and next steps

### 1) Flexbox (display:flex, flex: 1, align-items, gap)
- Status: [Production] for §§2–4 and §§7–9 — axes resolution; single- and multi-line layout with wrapping; per-line `justify-content`; cross-axis `align-items`; cross-axis packing via `align-content` (including stretch); CSS gaps (px and %); auto margins distribution on single- and multi-line; absolutely-positioned flex children placement (§4.1); and `overflow: hidden` support (padding-box clipping + content-height clamp).
- Impacted selectors:
  - `.row { display: flex; gap: 8px; align-items: center; height: 40px; }`
  - `.addr { flex: 1; … }`
- Current behavior: Flex containers lay out children per the [Production] set above. Remaining gaps: overflow scroll/auto behaviors; advanced writing modes.
- References: Flexbox module and spec mapping at `crates/css/modules/flexbox/` (`spec.md`), with fields in `css/orchestrator/src/style_model.rs`.
- Next steps:
  - Overflow scroll/auto in flex context: finalize behavior and add graphics baselines. (Fixture added: `flex/50_overflow_hidden_clamp.html` green.)
  - Advanced writing modes (vertical, rtl) and axis resolution generalization.
  - Fixtures: advanced writing modes; overflow scenarios (baseline capture for `50_overflow_hidden_clamp.html`).

### 2) Positioning (position: fixed; also absolute/sticky roadmap)
- Status: Missing (module stub)
- Impacted selectors:
  - `.topbar { position: fixed; top: 0; left: 0; right: 0; z-index: 9999; … }`
- Current behavior: Header is laid out in normal flow; not pinned to viewport.
- References: `css/modules/position/src/lib.rs` (stub), checklist Phase C (unchecked).
- Next steps:
  - Implement `position: relative` offset application in block layout (simple pass).
  - Implement `position: absolute` with nearest positioned ancestor; take out-of-flow and place.
  - Implement `position: fixed` anchored to viewport with its own containing block.
  - Defer `sticky` until basics are green.

### 3) Inline-block semantics
- Status: Missing
- Impacted usage:
  - `.btn` declared as `display: inline-block` behavior (it currently uses `display: inline-block` semantics via typical button expectations).
- Current behavior: Engine models `Inline`/`Block` but not full `inline-block` specifics.
- Next steps:
  - Introduce `InlineBlock` in the display model (or emulate via anonymous block around inline-replaced-like box) and basic size/shrink-to-fit behavior.
  - Add minimal fixtures for replaced/inline-block sizing in block formatting context.

### 4) Border radius
- Status: Missing
- Impacted selectors:
  - `.btn`, `.addr` use `border-radius: 4px;`
- Current behavior: Borders render square.
- References: `css/modules/backgrounds_borders/` exists but not fully implemented.
- Next steps:
  - Add `border_radius` to `ComputedStyle` and parsing support.
  - Update painter to clip and draw rounded corners (even without anti-alias perfection initially).

### 5) CSS transforms (translate)
- Status: Missing
- Impacted selectors:
  - `.btn:active { transform: translateY(1px); }`
- Current behavior: No visual translation occurs.
- Next steps:
  - Introduce a minimal `transform` property in the model for translate/scale/rotate (subset).
  - Apply transforms in painting with a simple 2D matrix; layout can remain unaffected for MVP.

### 6) Pseudo-element ::placeholder
- Status: Missing
- Impacted selectors:
  - `.addr::placeholder { color: #9ca3af; }`
- Current behavior: Placeholder text uses default styling.
- Next steps:
  - Extend selector engine/style application to generate computed styles for pseudo-elements where applicable.
  - Introduce placeholder color mapping as a special-case first, then generalize.

### 7) Pseudo-class :active (dynamic styling)
- Status: Missing (dynamic state mapping not wired)
- Impacted selectors:
  - `.btn:active { transform: translateY(1px); }`
- Current behavior: No active-state styling effect.
- Next steps:
  - Add dynamic state hooks from the event system → style engine to recompute or override pseudo-class matches (`:active`, `:hover`, `:focus`).
  - Start with `:active` to toggle a style override during pointer down.

### 8) Cursor, user-select
- Status: Missing
- Impacted selectors:
  - `.btn { cursor: pointer; user-select: none; }`
- Current behavior: No effect.
- Next steps:
  - Add `cursor` and `user_select` to `ComputedStyle` and plumb to the embedding to change pointer cursor and text selection behavior.
  - For MVP, accept and ignore at layout/paint but expose to host UI for actual behavior.

### 9) Percent/relative heights and root 100% height
- Status: Partial / Incomplete
- Impacted selectors:
  - `html, body { height: 100%; }`
- Current behavior: Percent heights may not resolve as per spec in all contexts.
- References: Sizing module unchecked; `10_6_3_height_of_blocks.rs` exists for some block cases.
- Next steps:
  - Implement `css_sizing` percent resolution rules for block and flex contexts.
  - Ensure root viewport-percentage handling for `html/body` works (establish containing block from the viewport).

### 10) z-index and stacking contexts
- Status: Partial
- Impacted selectors:
  - `.topbar { z-index: 9999; }`
- Current behavior: `z_index` is parsed/stored but full stacking/painting order may not be reliable.
- Next steps:
  - Define stacking context creation (positioned, opacity/transform when added later, root) and implement a painter pass that sorts by z-index within contexts.
  - Add fixtures covering overlap and z-ordering.

### 11) Fonts and shorthand application
- Status: Partial (font-family system and metrics pending broader Fonts module)
- Impacted selectors:
  - `body { font: 14px/1.4 -apple-system, BlinkMacSystemFont, … }`
  - `input, button { font: inherit; }`
- Current behavior: Numeric size/line-height supported; family fallback and metrics are limited.
- Next steps:
  - Implement a minimal font-family resolver with generic families; wire text metrics sufficient for line boxes.
  - Honor `font: inherit` for inputs/buttons in computed style.

## Recommended implementation order

1) Flexbox polish (overflow scroll/auto behaviors and advanced writing modes)
2) Positioning MVP (relative/absolute, then fixed) — enables fixed top bar.
3) z-index stacking contexts — ensures header occludes content.
4) Percent sizing/Sizing basics — stabilizes `height: 100%` and min/max.
5) Visual polish: `border-radius`, `transform` subset.
6) Pseudo elements/classes (`::placeholder`, `:active`) and cursor/user-select integration.

## Acceptance criteria for the main page

- Top bar is fixed to the top with correct z-order over scrolled content.
- Flex layout correctly aligns buttons, address field stretches with `flex: 1`, and `gap` spacing is applied.
- Inputs/buttons inherit font size/family from body; placeholder color is applied.
- Border radius and pressed translation render as specified.
- `html, body { height: 100% }` behaves consistently with viewport sizing.

