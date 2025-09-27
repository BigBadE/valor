# CSS Flexible Box Layout — Spec Coverage Map (Level 1)

Primary spec: https://www.w3.org/TR/css-flexbox-1/

## Scope and maturity

- Status:
  - [Production] §§2–4 (terminology, container detection, item collection)
  - [MVP] §§7–9 initial subset: axes resolution, single-line main-axis layout, justify-content (start/center/end/space-between/around/evenly), CSS main-axis gaps, cross-axis align-items (stretch/center/flex-start/flex-end)
- Notes:
  - [Heuristic] Cross-axis Stretch applies when item cross-size is auto/unspecified (≤ 0); otherwise we preserve the item’s cross-size.
  - [Approximation] Single-line layout only; multi-line wrapping and packing not yet implemented.
  - [TODO] Baseline alignment; auto margins interaction in main-axis distribution; min/max interactions; overflow; advanced writing modes; percentage gaps beyond trivial; absolutely-positioned flex children.
  - Out of scope (for now): fragmentation.

## One-to-one spec mapping

Per-section mapping to concrete code symbols. Keep aligned with code changes.

- §7 Axes
  - Code: `css_flexbox::resolve_axes`
  - File: `crates/css/modules/flexbox/src/7_axis_and_order/mod.rs`
  - Status: [Production]

- §8 Cross-axis alignment (single-line)
  - Code: `css_flexbox::align_single_line_cross`, `css_flexbox::align_cross_for_items`
  - File: `crates/css/modules/flexbox/src/8_single_line_layout/mod.rs`
  - Status: [MVP] — stretch heuristic as noted

- §9 Main-axis layout (single-line)
  - Code: `css_flexbox::layout_single_line`, `css_flexbox::layout_single_line_with_cross`
  - Helpers: `css_flexbox::justify_params`, `css_flexbox::accumulate_main_offsets`, `css_flexbox::clamp_first_offset_if_needed`
  - Types: `css_flexbox::FlexContainerInputs`, `css_flexbox::FlexChild`, `css_flexbox::FlexPlacement`, `css_flexbox::JustifyContent`, `css_flexbox::AlignItems`
  - File: `crates/css/modules/flexbox/src/8_single_line_layout/mod.rs`
  - Status: [MVP] — single-line only; Start/Center/End/Space* modes supported; no pre-gap at main-start invariant enforced for Start/SpaceBetween when axis not reversed.

## Algorithms and data flow

- Entry points:
  - `css_flexbox::flex_context::establishes_flex_formatting_context(display)`
  - `css_flexbox::flex_items::collect_flex_items(children)`
  - `css_flexbox::resolve_axes(direction, writing_mode)`
  - `css_flexbox::order_key(order, original_index)` / `css_flexbox::sort_items_by_order_stable(items)`
  - `css_flexbox::layout_single_line(container_inputs, justify_content, items)`
  - `css_flexbox::align_single_line_cross(align_items, container_cross, item_cross, min_cross, max_cross)`
- Helpers:
  - Axis tuple resolution (main/cross, start/end)
  - Simple item filter for in-flow flex items
- Invariants:
  - A node establishes a flex formatting context iff `display` is `flex` or `inline-flex`. See §4 and §5.

## Parsing/inputs (if applicable)

- Inputs are computed styles and normalized child lists produced by Display and Cascade modules. No direct token parsing here.

## Integration

- Upstream:
  - Display 3 normalization for `display` values; Writing Modes for axis mapping.
- Downstream (Core §10.6.3 entry points and flex integration):
  - `css_core::compute_child_content_height` flex branch
    - File: `crates/css/modules/core/src/10_visual_details/part_10_6_3_height_of_blocks.rs`
  - `css_core::container_layout_context` (origin, axes, container main size, gap)
  - `css_core::build_flex_item_inputs` (maps computed styles → `FlexChild`, cross constraints)
  - `css_core::justify_align_context` (maps `ComputedStyle` → `JustifyContent`, `AlignItems`, container cross size)
  - `css_core::write_pairs_and_measure` (applies `FlexPlacement`/`CrossPlacement` to rects)

## Edge cases

- `display: contents` descendants are not flex items (handled during item collection by only including element block nodes; see `collect_item_shells` in Core integration).
- No pre-gap at main-start for `justify-content: start` and `space-between` on non-reverse axes; invariant enforced post-accumulation.
- Direction reversal flips accumulation order; unit tests cover reverse/center behavior.
- Cross-axis Stretch heuristic only when item cross-size is auto/unspecified (≤ 0).

## Testing and fixtures

- Fixtures path: `crates/css/modules/flexbox/tests/fixtures/layout/`
- Harness: Chromium compare runner (headless) via `valor` test; epsilon tolerance ≈ `3*EPSILON`.
- Current pass status (green):
  - `flex/gap.html` — `justify-content: start` with `column-gap`
  - `flex/14_justify_space_between.html` — `justify-content: space-between`
  - `flex/11_align_items_center.html` — cross-axis center alignment
- Use `.fail` suffix for intentionally unsupported multi-line or advanced cases during development.

## Documentation and coding standards

- Doc comments include spec links; modules mirror spec chapters; imports at top; functions short and focused.

## Future work

- [ ] Multi-line wrapping and packing (§9): line breaking, line cross-size computation, line alignment, main-axis distribution across lines.
- [ ] Baseline alignment (§8, §9): shared baseline groups and first/last baseline alignment.
- [ ] Auto margins in main-axis distribution (§9.4): absorb free space and interaction with justify modes.
- [ ] Min/max and preferred-size constraints interactions with flexing (CSS Sizing): clamp during grow/shrink; finalize constraints.
- [ ] Overflow handling and scroll containers in flex context.
- [ ] Advanced writing modes (vertical, rtl) and axis resolution generalization.
- [ ] Percentage gaps and sizes beyond trivial cases; resolution against container sizes.
- [ ] Absolutely-positioned flex children: static-position behaviors and alignment container handling.

---

## Verbatim Spec (integrated)

Legal notice (required when embedding spec text):

```
$valor: https://github.com/BigBadE/valor
Copyright © [2025] World Wide Web Consortium. All Rights Reserved. This work is distributed under the W3C® Software and Document License [1] in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
[1] https://www.w3.org/Consortium/Legal/copyright-software
```

Begin embedded normative mapping below. Keep chapters in spec order and exclude non-normative front matter. Each section inserts a status/mapping block, followed by a details block for verbatim text.

<!-- BEGIN VERBATIM SPEC: DO NOT EDIT BELOW. This block is auto-generated by scripts/vendor_display_spec.ps1 -->

<h2 class="heading settled" data-level="2" id="box-model"><span class="secno">2. </span><span class="content"> Flex Layout Box Model and Terminology</span><a class="self-link" href="#box-model"></a></h2>
<div data-valor-status="box-model">
  <p><strong>Status:</strong> [Production]</p>
  <p><strong>Code:</strong> <code>css_flexbox::FlexDirection</code>, <code>css_flexbox::FlexWrap</code></p>
  <p><strong>Fixtures:</strong> <em>None</em></p>
  <p><strong>Notes:</strong> Terminology mapping scaffolding only; axis resolution and sizing hooks to be added.</p>
  <p><strong>Spec:</strong> <a href="https://www.w3.org/TR/css-flexbox-1/#box-model">§2 Box Model and Terminology</a></p>
</div>

<details class="valor-spec" data-level="2">
  <summary>Show spec text</summary>

<p>A <dfn class="dfn-paneled" data-dfn-type="dfn" data-export id="flex-container">flex container</dfn> is the box generated by an element with a
	computed <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css-display-3/#propdef-display" id="ref-for-propdef-display①">display</a> of <a class="css" data-link-type="maybe" href="#valdef-display-flex" id="ref-for-valdef-display-flex">flex</a> or <a class="css" data-link-type="maybe" href="#valdef-display-inline-flex" id="ref-for-valdef-display-inline-flex">inline-flex</a>.
	In-flow children of a flex container are called <dfn class="dfn-paneled" data-dfn-type="dfn" data-export data-lt="flex item" id="flex-item">flex items</dfn> and are laid out using the flex layout model.</p>
   <p>Unlike block and inline layout,
	whose layout calculations are biased to the <a href="https://www.w3.org/TR/css3-writing-modes/#abstract-box">block and inline flow directions</a>,
	flex layout is biased to the <dfn data-dfn-type="dfn" data-lt="flex direction" data-noexport id="flex-direction">flex directions<a class="self-link" href="#flex-direction"></a></dfn>.
	To make it easier to talk about flex layout,
	this section defines a set of flex flow–relative terms.
	The <a class="property" data-link-type="propdesc" href="#propdef-flex-flow" id="ref-for-propdef-flex-flow">flex-flow</a> value and the <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#writing-mode" id="ref-for-writing-mode">writing mode</a> determine how these terms map
	to physical directions (top/right/bottom/left),
	axes (vertical/horizontal), and sizes (width/height).</p>
   <figure>
     <img alt height="277" src="images/flex-direction-terms.svg" width="665">
    <figcaption> An illustration of the various directions and sizing terms as applied to a <a class="css" data-link-type="maybe" href="#valdef-flex-direction-row" id="ref-for-valdef-flex-direction-row">row</a> flex container. </figcaption>
   </figure>
   <dl id="main">
    <dt class="axis">main axis
    <dt class="axis">main dimension
    <dd> The <dfn class="dfn-paneled" data-dfn-type="dfn" data-export data-lt="main axis|main-axis" id="main-axis">main axis</dfn> of a flex container is the primary axis along which <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item">flex items</a> are laid out.
			It extends in the <dfn class="dfn-paneled" data-dfn-type="dfn" data-export id="main-dimension">main dimension</dfn>.
    <dt class="side">main-start
    <dt class="side">main-end
    <dd> The <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①">flex items</a> are placed within the container
			starting on the <dfn class="dfn-paneled" data-dfn-type="dfn" data-export id="main-start">main-start</dfn> side
			and going toward the <dfn class="dfn-paneled" data-dfn-type="dfn" data-export id="main-end">main-end</dfn> side.
    <dt class="size">main size
    <dt class="size">main size property
    <dd> The width or height of a <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container">flex container</a> or <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item②">flex item</a>,
			whichever is in the <a data-link-type="dfn" href="#main-dimension" id="ref-for-main-dimension">main dimension</a>,
			is that box’s <dfn class="dfn-paneled" data-dfn-type="dfn" data-export data-lt="main size|main-size" id="main-size">main size</dfn>.
			Its <dfn class="dfn-paneled" data-dfn-type="dfn" data-export id="main-size-property">main size property</dfn> is
			thus either its <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-width" id="ref-for-propdef-width">width</a> or <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-height" id="ref-for-propdef-height">height</a> property,
			whichever is in the <a data-link-type="dfn" href="#main-dimension" id="ref-for-main-dimension①">main dimension</a>.
			Similarly, its <dfn class="dfn-paneled" data-dfn-type="dfn" data-export data-lt="min main size property" id="min-main-size-property">min</dfn> and <dfn class="dfn-paneled" data-dfn-type="dfn" data-export data-lt="max main size property" id="max-main-size-property">max main size properties</dfn> are its <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-min-width" id="ref-for-propdef-min-width">min-width</a>/<a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-max-width" id="ref-for-propdef-max-width">max-width</a> or <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-min-height" id="ref-for-propdef-min-height">min-height</a>/<a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-max-height" id="ref-for-propdef-max-height">max-height</a> properties,
			whichever is in the <a data-link-type="dfn" href="#main-dimension" id="ref-for-main-dimension②">main dimension</a>,
			and determine its <dfn class="dfn-paneled" data-dfn-type="dfn" data-export data-lt="min main size" id="min-main-size">min</dfn>/<dfn class="dfn-paneled" data-dfn-type="dfn" data-export id="max-main-size">max main size</dfn>.
   </dl>
   <dl id="cross">
    <dt class="axis">cross axis
    <dt class="axis">cross dimension
    <dd> The axis perpendicular to the <a data-link-type="dfn" href="#main-axis" id="ref-for-main-axis②">main axis</a> is called the <dfn class="dfn-paneled" data-dfn-type="dfn" data-export data-lt="cross axis|cross-axis" id="cross-axis">cross axis</dfn>.
			It extends in the <dfn class="dfn-paneled" data-dfn-type="dfn" data-export id="cross-dimension">cross dimension</dfn>.
    <dt class="side">cross-start
    <dt class="side">cross-end
    <dd> <a data-link-type="dfn" href="#flex-line" id="ref-for-flex-line">Flex lines</a> are filled with items and placed into the container
			starting on the <dfn class="dfn-paneled" data-dfn-type="dfn" data-export id="cross-start">cross-start</dfn> side of the flex container
			and going toward the <dfn class="dfn-paneled" data-dfn-type="dfn" data-export id="cross-end">cross-end</dfn> side.
    <dt class="size">cross size
    <dt class="size">cross size property
    <dd> The width or height of a <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container①">flex container</a> or <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item③">flex item</a>,
			whichever is in the <a data-link-type="dfn" href="#cross-dimension" id="ref-for-cross-dimension">cross dimension</a>,
			is that box’s <dfn class="dfn-paneled" data-dfn-type="dfn" data-export data-lt="cross size | cross-size" id="cross-size">cross size</dfn>.
			Its <dfn class="dfn-paneled" data-dfn-type="dfn" data-export id="cross-size-property">cross size property</dfn> is
			thus either its <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-width" id="ref-for-propdef-width①">width</a> or <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-height" id="ref-for-propdef-height①">height</a> property,
			whichever is in the <a data-link-type="dfn" href="#cross-dimension" id="ref-for-cross-dimension①">cross dimension</a>.
			Similarly, its <dfn class="dfn-paneled" data-dfn-type="dfn" data-export data-lt="min cross size property" id="min-cross-size-property">min</dfn> and <dfn data-dfn-type="dfn" data-export data-lt="max cross size property" id="max-cross-size-property">max cross size properties<a class="self-link" href="#max-cross-size-property"></a></dfn> are its <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-min-width" id="ref-for-propdef-min-width①">min-width</a>/<a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-max-width" id="ref-for-propdef-max-width①">max-width</a> or <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-min-height" id="ref-for-propdef-min-height①">min-height</a>/<a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-max-height" id="ref-for-propdef-max-height①">max-height</a> properties,
			whichever is in the <a data-link-type="dfn" href="#cross-dimension" id="ref-for-cross-dimension②">cross dimension</a>,
			and determine its <dfn data-dfn-type="dfn" data-export data-lt="min cross size" id="min-cross-size">min<a class="self-link" href="#min-cross-size"></a></dfn>/<dfn data-dfn-type="dfn" data-export id="max-cross-size">max cross size<a class="self-link" href="#max-cross-size"></a></dfn>.
   </dl>
   <p>Additional sizing terminology used in this specification
	is defined in <a href="https://www.w3.org/TR/CSS-SIZING-3/">CSS Intrinsic and Extrinsic Sizing</a>. <a data-link-type="biblio" href="#biblio-css-sizing-3">[CSS-SIZING-3]</a></p>

</details>
<h2 class="heading settled" data-level="3" id="flex-containers"><span class="secno">3. </span><span class="content"> Flex Containers: the <a class="css" data-link-type="maybe" href="#valdef-display-flex" id="ref-for-valdef-display-flex①">flex</a> and <a class="css" data-link-type="maybe" href="#valdef-display-inline-flex" id="ref-for-valdef-display-inline-flex①">inline-flex</a> <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css-display-3/#propdef-display" id="ref-for-propdef-display②">display</a> values</span><a class="self-link" href="#flex-containers"></a></h2>
<div data-valor-status="flex-containers">
  <p><strong>Status:</strong> [Production]</p>
  <p><strong>Code:</strong> <code>css_flexbox::establishes_flex_formatting_context</code>, <code>css_flexbox::FlexDirection</code>, <code>css_flexbox::FlexWrap</code></p>
  <p><strong>Fixtures:</strong> <em>None</em></p>
  <p><strong>Notes:</strong> Detects <code>display:flex</code>/<code>inline-flex</code> and models container properties.</p>
  <p><strong>Spec:</strong> <a href="https://www.w3.org/TR/css-flexbox-1/#flex-containers">§3 Flex Containers</a></p>
</div>

<details class="valor-spec" data-level="2">
  <summary>Show spec text</summary>

<table class="def propdef partial" data-link-for-hint="display">
    <tbody>
     <tr>
      <th>Name:
      <td><a class="css" data-link-type="property" href="https://www.w3.org/TR/css-display-3/#propdef-display" id="ref-for-propdef-display③">display</a>
     <tr class="value">
      <th><a href="https://www.w3.org/TR/css-values/#value-defs">New values:</a>
      <td class="prod">flex <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one">|</a> inline-flex
   </table>
   <dl>
    <dt><dfn class="dfn-paneled css" data-dfn-for="display" data-dfn-type="value" data-export id="valdef-display-flex">flex</dfn>
    <dd> This value causes an element to generate a <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container②">flex container</a> box
			that is <a data-link-type="dfn" href="https://www.w3.org/TR/css-display-3/#block-level" id="ref-for-block-level">block-level</a> when placed in <a data-link-type="dfn" href="https://www.w3.org/TR/css-display-3/#flow-layout" id="ref-for-flow-layout">flow layout</a>.
    <dt><dfn class="dfn-paneled css" data-dfn-for="display" data-dfn-type="value" data-export id="valdef-display-inline-flex">inline-flex</dfn>
    <dd> This value causes an element to generate a <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container③">flex container</a> box
			that is <a data-link-type="dfn" href="https://www.w3.org/TR/css-display-3/#inline-level" id="ref-for-inline-level">inline-level</a> when placed in <a data-link-type="dfn" href="https://www.w3.org/TR/css-display-3/#flow-layout" id="ref-for-flow-layout①">flow layout</a>.
   </dl>
   <p>A <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container④">flex container</a> establishes a new <dfn data-dfn-type="dfn" data-export id="flex-formatting-context">flex formatting context<a class="self-link" href="#flex-formatting-context"></a></dfn> for its contents.
	This is the same as establishing a block formatting context,
	except that flex layout is used instead of block layout.
	For example, floats do not intrude into the flex container,
	and the flex container’s margins do not collapse with the margins of its contents. <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑤">Flex containers</a> form a containing block for their contents <a href="https://www.w3.org/TR/CSS2/visudet.html#containing-block-details">exactly like block containers do</a>. <a data-link-type="biblio" href="#biblio-css21">[CSS21]</a> The <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css-overflow-3/#propdef-overflow" id="ref-for-propdef-overflow">overflow</a> property applies to <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑥">flex containers</a>.</p>
   <p>Flex containers are not block containers,
	and so some properties that were designed with the assumption of block layout don’t apply in the context of flex layout.
	In particular:</p>
   <ul>
    <li data-md>
     <p><a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visuren.html#propdef-float" id="ref-for-propdef-float">float</a> and <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visuren.html#propdef-clear" id="ref-for-propdef-clear">clear</a> do not create floating or clearance of <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item④">flex item</a>,
and do not take it out-of-flow.</p>
    <li data-md>
     <p><a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css-inline-3/#propdef-vertical-align" id="ref-for-propdef-vertical-align">vertical-align</a> has no effect on a flex item.</p>
    <li data-md>
     <p>the <a class="css" data-link-type="maybe" href="https://www.w3.org/TR/css-pseudo-4/#selectordef-first-line" id="ref-for-selectordef-first-line①">::first-line</a> and <a class="css" data-link-type="maybe" href="https://www.w3.org/TR/css-pseudo-4/#selectordef-first-letter" id="ref-for-selectordef-first-letter①">::first-letter</a> pseudo-elements do not apply to <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑦">flex containers</a>,
and <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑧">flex containers</a> do not contribute a <a data-link-type="dfn" href="https://www.w3.org/TR/css-pseudo-4/#first-formatted-line" id="ref-for-first-formatted-line">first formatted line</a> or <a data-link-type="dfn">first letter</a> to their ancestors.</p>
   </ul>
   <p>If an element’s specified <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css-display-3/#propdef-display" id="ref-for-propdef-display④">display</a> is <a class="css" data-link-type="maybe" href="#valdef-display-inline-flex" id="ref-for-valdef-display-inline-flex②">inline-flex</a>,
	then its <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css-display-3/#propdef-display" id="ref-for-propdef-display⑤">display</a> property computes to <a class="css" data-link-type="maybe" href="#valdef-display-flex" id="ref-for-valdef-display-flex②">flex</a> in certain circumstances:
	the table in <a href="https://www.w3.org/TR/CSS2/visuren.html#dis-pos-flo">CSS 2.1 Section 9.7</a> is amended to contain an additional row,
	with <a class="css" data-link-type="maybe" href="#valdef-display-inline-flex" id="ref-for-valdef-display-inline-flex③">inline-flex</a> in the "Specified Value" column
	and <a class="css" data-link-type="maybe" href="#valdef-display-flex" id="ref-for-valdef-display-flex③">flex</a> in the "Computed Value" column.</p>

</details>
<h2 class="heading settled" data-level="4" id="flex-items"><span class="secno">4. </span><span class="content"> Flex Items</span><a class="self-link" href="#flex-items"></a></h2>
<div data-valor-status="flex-items">
  <p><strong>Status:</strong> [Production]</p>
  <p><strong>Code:</strong> <code>css_flexbox::collect_flex_items</code></p>
  <p><strong>Fixtures:</strong> <em>None</em></p>
  <p><strong>Notes:</strong> Filters in-flow children to items; excludes <code>display:none</code> and out-of-flow.</p>
  <p><strong>Spec:</strong> <a href="https://www.w3.org/TR/css-flexbox-1/#flex-items">§4 Flex Items</a></p>
</div>

<details class="valor-spec" data-level="2">
  <summary>Show spec text</summary>

<p>Loosely speaking, the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑤">flex items</a> of a <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑨">flex container</a> are boxes representing its in-flow contents.</p>
   <p>Each in-flow child of a <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container①⓪">flex container</a> becomes a <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑥">flex item</a>,
	and each contiguous sequence of child <a data-link-type="dfn" href="https://www.w3.org/TR/css-display-3/#css-text-run" id="ref-for-css-text-run">text runs</a> is wrapped in an <a data-link-type="dfn" href="https://www.w3.org/TR/css-display-3/#css-anonymous" id="ref-for-css-anonymous">anonymous</a> <a data-link-type="dfn" href="https://www.w3.org/TR/css-display-3/#block-container" id="ref-for-block-container">block container</a> <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑦">flex item</a>.
	However, if the entire sequence of child <a data-link-type="dfn" href="https://www.w3.org/TR/css-display-3/#css-text-run" id="ref-for-css-text-run①">text runs</a> contains only <a href="https://www.w3.org/TR/CSS2/text.html#white-space-prop">white space</a> (i.e. characters that can be affected by the <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css-text-3/#propdef-white-space" id="ref-for-propdef-white-space">white-space</a> property)
	it is instead not rendered (just as if its <a data-link-type="dfn" href="https://www.w3.org/TR/css-display-3/#css-text-node" id="ref-for-css-text-node">text nodes</a> were <span class="css">display:none</span>).</p>
   <div class="example" id="example-cbe28400">
    <a class="self-link" href="#example-cbe28400"></a>
    <p>Examples of flex items: </p>
<pre class="lang-markup highlight"><c- p>&lt;</c-><c- f>div</c-> <c- e>style</c-><c- o>=</c-><c- s>"display:flex"</c-><c- p>></c->

    <c- c>&lt;!-- flex item: block child --></c->
    <c- p>&lt;</c-><c- f>div</c-> <c- e>id</c-><c- o>=</c-><c- s>"item1"</c-><c- p>></c->block<c- p>&lt;/</c-><c- f>div</c-><c- p>></c->

    <c- c>&lt;!-- flex item: floated element; floating is ignored --></c->
    <c- p>&lt;</c-><c- f>div</c-> <c- e>id</c-><c- o>=</c-><c- s>"item2"</c-> <c- e>style</c-><c- o>=</c-><c- s>"float: left;"</c-><c- p>></c->float<c- p>&lt;/</c-><c- f>div</c-><c- p>></c->

    <c- c>&lt;!-- flex item: anonymous block box around inline content --></c->
    anonymous item 3

    <c- c>&lt;!-- flex item: inline child --></c->
    <c- p>&lt;</c-><c- f>span</c-><c- p>></c->
        item 4
        <c- c>&lt;!-- flex items do not </c-><a href="https://www.w3.org/TR/CSS2/visuren.html#anonymous-block-level"><c- c>split</c-></a><c- c> around blocks --></c->
        <c- p>&lt;</c-><c- f>q</c-> <c- e>style</c-><c- o>=</c-><c- s>"display: block"</c-> <c- e>id</c-><c- o>=</c-><c- s>not-an-item</c-><c- p>></c->item 4<c- p>&lt;/</c-><c- f>q</c-><c- p>></c->
        item 4
    <c- p>&lt;/</c-><c- f>span</c-><c- p>></c->
<c- p>&lt;/</c-><c- f>div</c-><c- p>></c->
</pre>
    <figure>
     <figcaption>Flex items determined from above code block</figcaption>
      <a href="examples/flex-item-determination.html"> <object data="images/flex-item-determination.png" type="image/png"> <ol> <li>Flex item containing <samp>block</samp>. </li><li>Flex item containing <samp>float</samp>. </li><li>(Anonymous, unstyleable) flex item containing <samp>anonymous item 3</samp>. </li><li>Flex item containing three blocks in succession: <ul> <li>Anonymous block containing <samp>item 4</samp>. </li><li><code>&lt;q></code> element block containing <samp>item 4</samp>. </li><li>Anonymous block containing <samp>item 4</samp>. </li></ul> </li></ol> </object> </a>
    </figure>
    <p>Note that the inter-element white space disappears:
		it does not become its own flex item,
		even though the inter-element text <em>does</em> get wrapped in an anonymous flex item.</p>
    <p>Note also that the anonymous item’s box is unstyleable,
		since there is no element to assign style rules to.
		Its contents will however inherit styles (such as font settings) from the flex container.</p>
   </div>
   <p>A <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑧">flex item</a> <a data-link-type="dfn" href="https://www.w3.org/TR/css-display-3/#establish-an-independent-formatting-context" id="ref-for-establish-an-independent-formatting-context">establishes an independent formatting context</a> for its contents.
	However, flex items themselves are <dfn data-dfn-type="dfn" data-noexport id="flex-level">flex-level<a class="self-link" href="#flex-level"></a></dfn> boxes, not block-level boxes:
	they participate in their container’s flex formatting context,
	not in a block formatting context.</p>
   <hr>
   <p class="note" role="note"><span>Note:</span> Authors reading this spec may want to <a href="#item-margins">skip past the following box-generation and static position details</a>.</p>
   <p>The <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css-display-3/#propdef-display" id="ref-for-propdef-display⑥">display</a> value of a <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑨">flex item</a> is <a data-link-type="dfn" href="https://www.w3.org/TR/css-display-3/#blockify" id="ref-for-blockify">blockified</a>:
	if the specified <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css-display-3/#propdef-display" id="ref-for-propdef-display⑦">display</a> of an in-flow child of an element generating a <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container①①">flex container</a> is an inline-level value, it computes to its block-level equivalent.
	(See <a href="https://www.w3.org/TR/CSS2/visuren.html#dis-pos-flo">CSS2.1§9.7</a> <a data-link-type="biblio" href="#biblio-css21">[CSS21]</a> and <a href="https://www.w3.org/TR/css-display/#transformations">CSS Display</a> <a data-link-type="biblio" href="#biblio-css3-display">[CSS3-DISPLAY]</a> for details on this type of <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css-display-3/#propdef-display" id="ref-for-propdef-display⑧">display</a> value conversion.)</p>
   <p class="note" role="note"><span>Note:</span> Some values of <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css-display-3/#propdef-display" id="ref-for-propdef-display⑨">display</a> normally trigger the creation of anonymous boxes around the original box.
	If such a box is a <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①⓪">flex item</a>,
	it is blockified first,
	and so anonymous box creation will not happen.
	For example, two contiguous <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①①">flex items</a> with <a class="css" data-link-type="propdesc" href="https://www.w3.org/TR/css-display-3/#propdef-display" id="ref-for-propdef-display①⓪">display: table-cell</a> will become two separate <a class="css" data-link-type="propdesc" href="https://www.w3.org/TR/css-display-3/#propdef-display" id="ref-for-propdef-display①①">display: block</a> <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①②">flex items</a>,
	instead of being wrapped into a single anonymous table.</p>
   <p>In the case of flex items with <a class="css" data-link-type="propdesc" href="https://www.w3.org/TR/css-display-3/#propdef-display" id="ref-for-propdef-display①②">display: table</a>,
	the table wrapper box becomes the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①③">flex item</a>,
	and the <a class="property" data-link-type="propdesc" href="#propdef-order" id="ref-for-propdef-order①">order</a> and <a class="property" data-link-type="propdesc" href="#propdef-align-self" id="ref-for-propdef-align-self④">align-self</a> properties apply to it.
	The contents of any caption boxes contribute to the calculation of
	the table wrapper box’s min-content and max-content sizes.
	However, like <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-width" id="ref-for-propdef-width②">width</a> and <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-height" id="ref-for-propdef-height②">height</a>, the <a class="property" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex">flex</a> longhands apply to the table box as follows:
	the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①④">flex item</a>’s final size is calculated
	by performing layout as if the distance between
	the table wrapper box’s edges and the table box’s content edges
	were all part of the table box’s border+padding area,
	and the table box were the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①⑤">flex item</a>.</p>

</details>
<h3 class="heading settled" data-level="4.1" id="abspos-items"><span class="secno">4.1. </span><span class="content"> Absolutely-Positioned Flex Children</span><a class="self-link" href="#abspos-items"></a></h3>
<!--__VALOR_STATUS:abspos-items__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<p>As it is out-of-flow,
	an absolutely-positioned child of a <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container①②">flex container</a> does not participate in flex layout.</p>
   <p>The <a href="https://www.w3.org/TR/CSS2/visudet.html#abs-non-replaced-width">static position</a> of an absolutely-positioned child of a <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container①③">flex container</a> is determined such that the child is positioned
	as if it were the sole <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①⑥">flex item</a> in the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container①④">flex container</a>,
	assuming both the child and the flex container
	were fixed-size boxes of their used size.
	For this purpose, <span class="css">auto</span> margins are treated as zero.</p>
   <div class="note" role="note">
     In other words,
		the <a data-link-type="dfn" href="#static-position-rectangle" id="ref-for-static-position-rectangle">static-position rectangle</a> of an absolutely-positioned child of a <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container①⑤">flex container</a> is the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container①⑥">flex container’s</a> content box,
		where the <dfn class="dfn-paneled" data-dfn-type="dfn" data-noexport id="static-position-rectangle">static-position rectangle</dfn> is the <a data-link-type="dfn" href="https://www.w3.org/TR/css3-align/#alignment-container" id="ref-for-alignment-container">alignment container</a> used to determine the static-position offsets of an absolutely-positioned box.
    <p>(In block layout the static position rectangle corresponds to the position of the “hypothetical box”
		described in <a href="https://www.w3.org/TR/CSS2/visudet.html#abs-non-replaced-width">CSS2.1§10.3.7</a>.
		Since it has no alignment properties,
		CSS2.1 always uses a <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#block-start" id="ref-for-block-start">block-start</a> <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#inline-start" id="ref-for-inline-start">inline-start</a> alignment
		of the absolutely-positioned box within the <a data-link-type="dfn" href="#static-position-rectangle" id="ref-for-static-position-rectangle①">static-position rectangle</a>.
		Note that this definition will eventually move to the CSS Positioning module.)</p>
   </div>
   <div class="example" id="example-30aea1d4">
    <a class="self-link" href="#example-30aea1d4"></a> The effect of this is that if you set, for example, <a class="css" data-link-type="propdesc" href="#propdef-align-self" id="ref-for-propdef-align-self⑤">align-self: center;</a> on an absolutely-positioned child of a <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container①⑦">flex container</a>,
		auto offsets on the child will center it in the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container①⑧">flex container’s</a> <a data-link-type="dfn" href="#cross-axis" id="ref-for-cross-axis②">cross axis</a>.
    <p>However, since the absolutely-positioned box is considered to be “fixed-size”,
		a value of <a class="css" data-link-type="maybe" href="#valdef-align-items-stretch" id="ref-for-valdef-align-items-stretch">stretch</a> is treated the same as <a class="css" data-link-type="maybe" href="#valdef-align-items-flex-start" id="ref-for-valdef-align-items-flex-start">flex-start</a>.</p>
   </div>

</details>
<h3 class="heading settled" data-level="4.2" id="item-margins"><span class="secno">4.2. </span><span class="content"> Flex Item Margins and Paddings</span><a class="self-link" href="#item-margins"></a></h3>
<!--__VALOR_STATUS:item-margins__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<p>The margins of adjacent <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①⑦">flex items</a> do not <a href="https://www.w3.org/TR/CSS2/box.html#collapsing-margins">collapse</a>.</p>
   <p>Percentage margins and paddings on <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①⑧">flex items</a>,
	like those on <a data-link-type="dfn" href="https://www.w3.org/TR/css-display-3/#block-box" id="ref-for-block-box">block boxes</a>,
	are resolved against the <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#inline-size" id="ref-for-inline-size">inline size</a> of their <a data-link-type="dfn" href="https://www.w3.org/TR/css-display-3/#containing-block" id="ref-for-containing-block">containing block</a>,
	e.g. left/right/top/bottom percentages
	all resolve against their <a data-link-type="dfn" href="https://www.w3.org/TR/css-display-3/#containing-block" id="ref-for-containing-block①">containing block</a>’s <em>width</em> in horizontal <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#writing-mode" id="ref-for-writing-mode①">writing modes</a>.</p>
   <p>Auto margins expand to absorb extra space in the corresponding dimension.
	They can be used for alignment,
	or to push adjacent flex items apart.
	See <a href="#auto-margins">Aligning with <span class="css">auto</span> margins</a>.</p>

</details>
<h3 class="heading settled" data-level="4.3" id="painting"><span class="secno">4.3. </span><span class="content"> Flex Item Z-Ordering</span><a class="self-link" href="#painting"></a></h3>
<!--__VALOR_STATUS:painting__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<p><a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①⑨">Flex items</a> paint exactly the same as inline blocks <a data-link-type="biblio" href="#biblio-css21">[CSS21]</a>,
	except that <a class="property" data-link-type="propdesc" href="#propdef-order" id="ref-for-propdef-order②">order</a>-modified document order is used in place of raw document order,
	and <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css3-positioning/#propdef-z-index" id="ref-for-propdef-z-index">z-index</a> values other than <a class="css" data-link-type="maybe" href="https://www.w3.org/TR/css3-positioning/#valdef-z-index-auto" id="ref-for-valdef-z-index-auto">auto</a> create a stacking context
	even if <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css3-positioning/#propdef-position" id="ref-for-propdef-position">position</a> is <a class="css" data-link-type="maybe" href="https://www.w3.org/TR/css3-positioning/#valdef-position-static" id="ref-for-valdef-position-static">static</a> (behaving exactly as if <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css3-positioning/#propdef-position" id="ref-for-propdef-position①">position</a> were <a class="css" data-link-type="maybe" href="https://www.w3.org/TR/css3-positioning/#valdef-position-relative" id="ref-for-valdef-position-relative">relative</a>).</p>
   <p class="note" role="note"><span>Note:</span> Descendants that are positioned outside a flex item still participate
	in any stacking context established by the flex item.</p>

</details>
<h3 class="heading settled" data-level="4.4" id="visibility-collapse"><span class="secno">4.4. </span><span class="content"> Collapsed Items</span><a class="self-link" href="#visibility-collapse"></a></h3>
<!--__VALOR_STATUS:visibility-collapse__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<p>Specifying <span class="css">visibility:collapse</span> on a flex item
	causes it to become a <dfn data-dfn-type="dfn" data-local-lt="collapsed" data-noexport id="collapsed-flex-item">collapsed flex item<a class="self-link" href="#collapsed-flex-item"></a></dfn>,
	producing an effect similar to <span class="css">visibility:collapse</span> on a table-row or table-column:
	the collapsed flex item is removed from rendering entirely,
	but leaves behind a "strut" that keeps the flex line’s cross-size stable.
	Thus, if a flex container has only one flex line,
	dynamically collapsing or uncollapsing items
	may change the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container①⑨">flex container</a>’s <a data-link-type="dfn" href="#main-size" id="ref-for-main-size">main size</a>, but
	is guaranteed to have no effect on its <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size①">cross size</a> and won’t cause the rest of the page’s layout to "wobble".
	Flex line wrapping <em>is</em> re-done after collapsing, however,
	so the cross-size of a flex container with multiple lines might or might not change.</p>
   <p>Though collapsed flex items aren’t rendered,
	they do appear in the <a href="https://www.w3.org/TR/CSS2/intro.html#formatting-structure">formatting structure</a>.
	Therefore, unlike on <span class="css">display:none</span> items <a data-link-type="biblio" href="#biblio-css21">[CSS21]</a>,
	effects that depend on a box appearing in the formatting structure
	(like incrementing counters or running animations and transitions)
	still operate on collapsed items.</p>
   <div class="example" id="example-a95220bf">
    <a class="self-link" href="#example-a95220bf"></a> In the following example,
		a sidebar is sized to fit its content. <a class="css" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visufx.html#propdef-visibility" id="ref-for-propdef-visibility">visibility: collapse</a> is used to dynamically hide parts of a navigation sidebar
		without affecting its width, even though the widest item (“Architecture”)
		is in a collapsed section.
    <figure>
     <figcaption>Sample live rendering for example code below</figcaption>
     <div id="visibility-collapse-example">
      <nav>
       <ul>
        <li id="nav-about">
         <a href="#nav-about">About</a>
         <ul>
          <li><a href="#">History</a>
          <li><a href="#">Mission</a>
          <li><a href="#">People</a>
         </ul>
        <li id="nav-projects">
         <a href="#nav-projects">Projects</a>
         <ul>
          <li><a href="#">Art</a>
          <li><a href="#">Architecture</a>
          <li><a href="#">Music</a>
         </ul>
        <li id="nav-interact">
         <a href="#nav-interact">Interact</a>
         <ul>
          <li><a href="#">Blog</a>
          <li><a href="#">Forums</a>
         </ul>
       </ul>
      </nav>
      <article> Hover over the menu to the left:
			    each section expands to show its sub-items.
			    In order to keep the sidebar width (and this main area width) stable, <a class="css" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visufx.html#propdef-visibility" id="ref-for-propdef-visibility①">visibility: collapse</a> is used instead of <a class="css" data-link-type="propdesc" href="https://www.w3.org/TR/css-display-3/#propdef-display" id="ref-for-propdef-display①③">display: none</a>.
			    This results in a sidebar that is always wide enough for the word “Architecture”,
			    even though it is not always visible. </article>
     </div>
    </figure>
<pre class="lang-css highlight"><c- n>@media</c-> <c- p>(</c->min-width<c- f>: 60em) </c-><c- p>{</c->
  <c- c>/* </c-><a href="https://www.w3.org/TR/css3-mediaqueries/#width"><c- c>two column layout only when enough room</c-></a><c- c> (relative to default text size) */</c->
  div { display: flex; }
  #main {
    flex: 1;         /* <a href="#flexibility">Main takes up all remaining space</a> */
    order: 1;        /* <a href="#order-property">Place it after (to the right of) the navigation</a> */
    min-width: 12em; /* Optimize main content area sizing */
  }
}
/* menu items use flex layout so that visibility:collapse will work */
nav > ul > li {
  display: flex;
  flex-flow: column;
}
/* dynamically collapse submenus when not targetted */
nav > ul > li:not(:target):not(:hover) > ul {
  visibility: collapse;
}
</pre>
<pre class="lang-markup highlight"><c- p>&lt;</c-><c- f>div</c-><c- p>></c->
  <c- p>&lt;</c-><c- f>article</c-> <c- e>id</c-><c- o>=</c-><c- s>"main"</c-><c- p>></c->
    Interesting Stuff to Read
  <c- p>&lt;/</c-><c- f>article</c-><c- p>></c->
  <c- p>&lt;</c-><c- f>nav</c-><c- p>></c->
    <c- p>&lt;</c-><c- f>ul</c-><c- p>></c->
      <c- p>&lt;</c-><c- f>li</c-> <c- e>id</c-><c- o>=</c-><c- s>"nav-about"</c-><c- p>>&lt;</c-><c- f>a</c-> <c- e>href</c-><c- o>=</c-><c- s>"#nav-about"</c-><c- p>></c->About<c- p>&lt;/</c-><c- f>a</c-><c- p>></c->
        …
      <c- p>&lt;</c-><c- f>li</c-> <c- e>id</c-><c- o>=</c-><c- s>"nav-projects"</c-><c- p>>&lt;</c-><c- f>a</c-> <c- e>href</c-><c- o>=</c-><c- s>"#nav-projects"</c-><c- p>></c->Projects<c- p>&lt;/</c-><c- f>a</c-><c- p>></c->
        <c- p>&lt;</c-><c- f>ul</c-><c- p>></c->
          <c- p>&lt;</c-><c- f>li</c-><c- p>>&lt;</c-><c- f>a</c-> <c- e>href</c-><c- o>=</c-><c- s>"…"</c-><c- p>></c->Art<c- p>&lt;/</c-><c- f>a</c-><c- p>></c->
          <c- p>&lt;</c-><c- f>li</c-><c- p>>&lt;</c-><c- f>a</c-> <c- e>href</c-><c- o>=</c-><c- s>"…"</c-><c- p>></c->Architecture<c- p>&lt;/</c-><c- f>a</c-><c- p>></c->
          <c- p>&lt;</c-><c- f>li</c-><c- p>>&lt;</c-><c- f>a</c-> <c- e>href</c-><c- o>=</c-><c- s>"…"</c-><c- p>></c->Music<c- p>&lt;/</c-><c- f>a</c-><c- p>></c->
        <c- p>&lt;/</c-><c- f>ul</c-><c- p>></c->
      <c- p>&lt;</c-><c- f>li</c-> <c- e>id</c-><c- o>=</c-><c- s>"nav-interact"</c-><c- p>>&lt;</c-><c- f>a</c-> <c- e>href</c-><c- o>=</c-><c- s>"#nav-interact"</c-><c- p>></c->Interact<c- p>&lt;/</c-><c- f>a</c-><c- p>></c->
        …
    <c- p>&lt;/</c-><c- f>ul</c-><c- p>></c->
  <c- p>&lt;/</c-><c- f>nav</c-><c- p>></c->
<c- p>&lt;/</c-><c- f>div</c-><c- p>></c->
<c- p>&lt;</c-><c- f>footer</c-><c- p>></c->
…
</pre>
   </div>
   <p>To compute the size of the strut, flex layout is first performed with all items uncollapsed,
	and then re-run with each collapsed item replaced by a strut that maintains
	the original cross-size of the item’s original line.
	See the <a href="#layout-algorithm">Flex Layout Algorithm</a> for the normative definition of how <span class="css">visibility:collapse</span> interacts with flex layout.</p>
   <p class="note" role="note"><span>Note:</span> Using <span class="css">visibility:collapse</span> on any flex items
	will cause the flex layout algorithm to repeat partway through,
	re-running the most expensive steps.
	It’s recommended that authors continue to use <span class="css">display:none</span> to hide items
	if the items will not be dynamically collapsed and uncollapsed,
	as that is more efficient for the layout engine.
	(Since only part of the steps need to be repeated when <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visufx.html#propdef-visibility" id="ref-for-propdef-visibility②">visibility</a> is changed,
	however, 'visibility: collapse' is still recommended for dynamic cases.)</p>

</details>
<h3 class="heading settled" data-level="4.5" id="min-size-auto"><span class="secno">4.5. </span><span class="content"> Automatic Minimum Size of Flex Items</span><a class="self-link" href="#min-size-auto"></a></h3>
<!--__VALOR_STATUS:min-size-auto__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<p class="note" role="note"><span>Note:</span> The <a class="css" data-link-type="maybe" href="https://www.w3.org/TR/css-sizing-3/#valdef-width-auto" id="ref-for-valdef-width-auto">auto</a> keyword,
	representing an <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#automatic-minimum-size" id="ref-for-automatic-minimum-size">automatic minimum size</a>,
	is the new initial value of the <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-min-width" id="ref-for-propdef-min-width②">min-width</a> and <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-min-height" id="ref-for-propdef-min-height②">min-height</a> properties.
	The keyword was previously defined in this specification,
	but is now defined in the <a data-link-type="biblio" href="#biblio-css-sizing-3">CSS Sizing</a> module.</p>
   <p>To provide a more reasonable default <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#min-width" id="ref-for-min-width">minimum size</a> for <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item②⓪">flex items</a>,
	the used value of a <a data-link-type="dfn" href="#main-axis" id="ref-for-main-axis③">main axis</a> <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#automatic-minimum-size" id="ref-for-automatic-minimum-size①">automatic minimum size</a> on a <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item②①">flex item</a> that is not a <a data-link-type="dfn" href="https://www.w3.org/TR/css-overflow-3/#scroll-container" id="ref-for-scroll-container">scroll container</a> is a <dfn class="dfn-paneled" data-dfn-type="dfn" data-noexport id="content-based-minimum-size">content-based minimum size</dfn>;
	for <a data-link-type="dfn" href="https://www.w3.org/TR/css-overflow-3/#scroll-container" id="ref-for-scroll-container①">scroll containers</a> the <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#automatic-minimum-size" id="ref-for-automatic-minimum-size②">automatic minimum size</a> is zero, as usual.</p>
   <p>In general, the <a data-link-type="dfn" href="#content-based-minimum-size" id="ref-for-content-based-minimum-size">content-based minimum size</a> of a <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item②②">flex item</a> is the smaller of its <a data-link-type="dfn" href="#content-size-suggestion" id="ref-for-content-size-suggestion">content size suggestion</a> and its <a data-link-type="dfn" href="#specified-size-suggestion" id="ref-for-specified-size-suggestion">specified size suggestion</a>.
	However, if the box has an aspect ratio and no <a data-link-type="dfn" href="https://www.w3.org/TR/css3-images/#specified-size" id="ref-for-specified-size">specified size</a>,
	its <a data-link-type="dfn" href="#content-based-minimum-size" id="ref-for-content-based-minimum-size①">content-based minimum size</a> is the smaller of its <a data-link-type="dfn" href="#content-size-suggestion" id="ref-for-content-size-suggestion①">content size suggestion</a> and its <a data-link-type="dfn" href="#transferred-size-suggestion" id="ref-for-transferred-size-suggestion">transferred size suggestion</a>.
	If the box has neither a <a data-link-type="dfn" href="#specified-size-suggestion" id="ref-for-specified-size-suggestion①">specified size suggestion</a> nor an aspect ratio,
	its <a data-link-type="dfn" href="#content-based-minimum-size" id="ref-for-content-based-minimum-size②">content-based minimum size</a> is the <a data-link-type="dfn" href="#content-size-suggestion" id="ref-for-content-size-suggestion②">content size suggestion</a>.</p>
   <p>The <a data-link-type="dfn" href="#content-size-suggestion" id="ref-for-content-size-suggestion③">content size suggestion</a>, <a data-link-type="dfn" href="#specified-size-suggestion" id="ref-for-specified-size-suggestion②">specified size suggestion</a>, and <a data-link-type="dfn" href="#transferred-size-suggestion" id="ref-for-transferred-size-suggestion①">transferred size suggestion</a> used in this calculation account for the relevant min/max/preferred size properties
	so that the <a data-link-type="dfn" href="#content-based-minimum-size" id="ref-for-content-based-minimum-size③">content-based minimum size</a> does not interfere with any author-provided constraints,
	and are defined below:</p>
   <dl>
    <dt><dfn class="dfn-paneled" data-dfn-type="dfn" data-noexport id="specified-size-suggestion">specified size suggestion</dfn>
    <dd> If the item’s computed <a data-link-type="dfn" href="#main-size-property" id="ref-for-main-size-property">main size property</a> is <a data-link-type="dfn" href="#definite" id="ref-for-definite">definite</a>,
			then the <a data-link-type="dfn" href="#specified-size-suggestion" id="ref-for-specified-size-suggestion③">specified size suggestion</a> is that size
			(clamped by its <a data-link-type="dfn" href="#max-main-size-property" id="ref-for-max-main-size-property">max main size property</a> if it’s <a data-link-type="dfn" href="#definite" id="ref-for-definite①">definite</a>).
			It is otherwise undefined.
    <dt><dfn class="dfn-paneled" data-dfn-type="dfn" data-noexport id="transferred-size-suggestion">transferred size suggestion</dfn>
    <dd> If the item has an intrinsic aspect ratio
			and its computed <a data-link-type="dfn" href="#cross-size-property" id="ref-for-cross-size-property">cross size property</a> is <a data-link-type="dfn" href="#definite" id="ref-for-definite②">definite</a>,
			then the <a data-link-type="dfn" href="#transferred-size-suggestion" id="ref-for-transferred-size-suggestion②">transferred size suggestion</a> is that size
			(clamped by its <a data-link-type="dfn" href="#min-cross-size-property" id="ref-for-min-cross-size-property">min and max cross size properties</a> if they are <a data-link-type="dfn" href="#definite" id="ref-for-definite③">definite</a>),
			converted through the aspect ratio.
			It is otherwise undefined.
    <dt><dfn class="dfn-paneled" data-dfn-type="dfn" data-noexport id="content-size-suggestion">content size suggestion</dfn>
    <dd> The <a data-link-type="dfn" href="#content-size-suggestion" id="ref-for-content-size-suggestion④">content size suggestion</a> is the <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#min-content" id="ref-for-min-content">min-content size</a> in the <a data-link-type="dfn" href="#main-axis" id="ref-for-main-axis④">main axis</a>,
			clamped, if it has an aspect ratio, by any <a data-link-type="dfn" href="#definite" id="ref-for-definite④">definite</a> <a data-link-type="dfn" href="#min-cross-size-property" id="ref-for-min-cross-size-property①">min and max cross size properties</a> converted through the aspect ratio,
			and then further clamped by the <a data-link-type="dfn" href="#max-main-size-property" id="ref-for-max-main-size-property①">max main size property</a> if that is <a data-link-type="dfn" href="#definite" id="ref-for-definite⑤">definite</a>.
   </dl>
   <p>For the purpose of calculating an intrinsic size of the box
	(e.g. the box’s <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#min-content" id="ref-for-min-content①">min-content size</a>),
	a <a data-link-type="dfn" href="#content-based-minimum-size" id="ref-for-content-based-minimum-size④">content-based minimum size</a> causes the box’s size in that axis to become indefinite
	(even if e.g. its <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-width" id="ref-for-propdef-width③">width</a> property specifies a <a data-link-type="dfn" href="#definite" id="ref-for-definite⑥">definite</a> size).
	Note this means that percentages calculated against this size
	will <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#behave-as-auto" id="ref-for-behave-as-auto">behave as auto</a>.</p>
   <p>Nonetheless, although this may require an additional layout pass to re-resolve percentages in some cases,
	this value
	(like the <a class="css" data-link-type="maybe" href="https://www.w3.org/TR/css-sizing-3/#valdef-width-min-content" id="ref-for-valdef-width-min-content">min-content</a>, <a class="css" data-link-type="maybe" href="https://www.w3.org/TR/css-sizing-3/#valdef-width-max-content" id="ref-for-valdef-width-max-content">max-content</a>, and <span class="css">fit-content</span> values defined in <a data-link-type="biblio" href="#biblio-css-sizing-3">[CSS-SIZING-3]</a>)
	does not prevent the resolution of percentage sizes within the item.</p>
   <div class="note" id="min-size-opt" role="note">
    <a class="self-link" href="#min-size-opt"></a> Note that while a content-based minimum size is often appropriate,
		and helps prevent content from overlapping or spilling outside its container,
		in some cases it is not:
    <p>In particular, if flex sizing is being used for a major content area of a document,
		it is better to set an explicit font-relative minimum width such as <a class="css" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-min-width" id="ref-for-propdef-min-width③">min-width: 12em</a>.
		A content-based minimum width could result in a large table or large image
		stretching the size of the entire content area into an overflow zone,
		and thereby making lines of text gratuitously long and hard to read.</p>
    <p>Note also, when content-based sizing is used on an item with large amounts of content,
		the layout engine must traverse all of this content before finding its minimum size,
		whereas if the author sets an explicit minimum, this is not necessary.
		(For items with small amounts of content, however,
		this traversal is trivial and therefore not a performance concern.)</p>
   </div>

</details>
<h2 class="heading settled" data-level="5" id="flow-order"><span class="secno">5. </span><span class="content"> Ordering and Orientation</span><a class="self-link" href="#flow-order"></a></h2>
<div data-valor-status="flow-order">
  <p><strong>Status:</strong> [Production] (helpers only)</p>
  <p><strong>Code:</strong> <code>css_flexbox::FlexDirection</code>, <code>css_flexbox::resolve_axes</code>, <code>css_flexbox::WritingMode</code></p>
  <p><strong>Fixtures:</strong> <em>Doc-only</em> (no geometry assertions yet)</p>
  <p><strong>Notes:</strong> Resolves main/cross axes and reverse based on <code>flex-direction</code> and writing mode.</p>
  <p><strong>Spec:</strong> <a href="https://www.w3.org/TR/css-flexbox-1/#flow-order">§5 Ordering and Orientation</a></p>
</div>

<details class="valor-spec" data-level="2">
  <summary>Show spec text</summary>

<p>The contents of a flex container can be laid out in any direction and in any order.
	This allows an author to trivially achieve effects that would previously have required complex or fragile methods,
	such as hacks using the <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visuren.html#propdef-float" id="ref-for-propdef-float①">float</a> and <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visuren.html#propdef-clear" id="ref-for-propdef-clear①">clear</a> properties.
	This functionality is exposed through the <a class="property" data-link-type="propdesc" href="#propdef-flex-direction" id="ref-for-propdef-flex-direction">flex-direction</a>, <a class="property" data-link-type="propdesc" href="#propdef-flex-wrap" id="ref-for-propdef-flex-wrap">flex-wrap</a>, and <a class="property" data-link-type="propdesc" href="#propdef-order" id="ref-for-propdef-order③">order</a> properties.</p>
   <p class="note" role="note"><span>Note:</span> The reordering capabilities of flex layout intentionally affect <em>only the visual rendering</em>,
	leaving speech order and navigation based on the source order.
	This allows authors to manipulate the visual presentation
	while leaving the source order intact for non-CSS UAs and for
	linear models such as speech and sequential navigation.
	See <a href="#order-accessibility">Reordering and Accessibility</a> and the <a href="#overview">Flex Layout Overview</a> for examples
	that use this dichotomy to improve accessibility.</p>
   <p><strong class="advisement"> Authors <em>must not</em> use <a class="property" data-link-type="propdesc" href="#propdef-order" id="ref-for-propdef-order④">order</a> or the <span class="css">*-reverse</span> values of <a class="property" data-link-type="propdesc" href="#propdef-flex-flow" id="ref-for-propdef-flex-flow①">flex-flow</a>/<a class="property" data-link-type="propdesc" href="#propdef-flex-direction" id="ref-for-propdef-flex-direction①">flex-direction</a> as a substitute for correct source ordering,
	as that can ruin the accessibility of the document.</strong></p>

</details>
<h3 class="heading settled" data-level="5.1" id="flex-direction-property"><span class="secno">5.1. </span><span class="content"> Flex Flow Direction: the <a class="property" data-link-type="propdesc" href="#propdef-flex-direction" id="ref-for-propdef-flex-direction②">flex-direction</a> property</span><a class="self-link" href="#flex-direction-property"></a></h3>
<!--__VALOR_STATUS:flex-direction-property__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<table class="def propdef" data-link-for-hint="flex-direction">
    <tbody>
     <tr>
      <th>Name:
      <td><dfn class="dfn-paneled css" data-dfn-type="property" data-export id="propdef-flex-direction">flex-direction</dfn>
     <tr class="value">
      <th><a href="https://www.w3.org/TR/css-values/#value-defs">Value:</a>
      <td class="prod">row <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one①">|</a> row-reverse <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one②">|</a> column <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one③">|</a> column-reverse
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#initial-values">Initial:</a>
      <td>row
     <tr>
      <th>Applies to:
      <td><a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container②⓪">flex containers</a>
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#inherited-property">Inherited:</a>
      <td>no
     <tr>
      <th><a href="https://www.w3.org/TR/css-values/#percentages">Percentages:</a>
      <td>n/a
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#computed">Computed value:</a>
      <td>specified keyword
     <tr>
      <th>Canonical order:
      <td>per grammar
     <tr>
      <th><a href="https://www.w3.org/TR/web-animations-1/#animation-type">Animation type:</a>
      <td>discrete
   </table>
   <p>The <a class="property" data-link-type="propdesc" href="#propdef-flex-direction" id="ref-for-propdef-flex-direction③">flex-direction</a> property specifies how <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item②③">flex items</a> are placed in the flex container,
	by setting the direction of the flex container’s <a data-link-type="dfn" href="#main-axis" id="ref-for-main-axis⑤">main axis</a>.
	This determines the direction in which flex items are laid out.</p>
   <dl>
    <dt><dfn class="dfn-paneled css" data-dfn-for="flex-direction" data-dfn-type="value" data-export id="valdef-flex-direction-row">row</dfn>
    <dd> The flex container’s <a data-link-type="dfn" href="#main-axis" id="ref-for-main-axis⑥">main axis</a> has the same orientation as the <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#inline-axis" id="ref-for-inline-axis">inline axis</a> of the current <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#writing-mode" id="ref-for-writing-mode②">writing mode</a>.
			The <a data-link-type="dfn" href="#main-start" id="ref-for-main-start">main-start</a> and <a data-link-type="dfn" href="#main-end" id="ref-for-main-end">main-end</a> directions are equivalent to the <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#inline-start" id="ref-for-inline-start①">inline-start</a> and <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#inline-end" id="ref-for-inline-end">inline-end</a> directions, respectively,
			of the current <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#writing-mode" id="ref-for-writing-mode③">writing mode</a>.
    <dt><dfn class="dfn-paneled css" data-dfn-for="flex-direction" data-dfn-type="value" data-export id="valdef-flex-direction-row-reverse">row-reverse</dfn>
    <dd> Same as <a class="css" data-link-type="maybe" href="#valdef-flex-direction-row" id="ref-for-valdef-flex-direction-row①">row</a>,
			except the <a data-link-type="dfn" href="#main-start" id="ref-for-main-start①">main-start</a> and <a data-link-type="dfn" href="#main-end" id="ref-for-main-end①">main-end</a> directions are swapped.
    <dt><dfn class="dfn-paneled css" data-dfn-for="flex-direction" data-dfn-type="value" data-export id="valdef-flex-direction-column">column</dfn>
    <dd> The flex container’s <a data-link-type="dfn" href="#main-axis" id="ref-for-main-axis⑦">main axis</a> has the same orientation as the <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#block-axis" id="ref-for-block-axis">block axis</a> of the current <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#writing-mode" id="ref-for-writing-mode④">writing mode</a>.
			The <a data-link-type="dfn" href="#main-start" id="ref-for-main-start②">main-start</a> and <a data-link-type="dfn" href="#main-end" id="ref-for-main-end②">main-end</a> directions are equivalent to the <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#block-start" id="ref-for-block-start①">block-start</a> and <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#block-end" id="ref-for-block-end">block-end</a> directions, respectively,
			of the current <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#writing-mode" id="ref-for-writing-mode⑤">writing mode</a>.
    <dt><dfn class="dfn-paneled css" data-dfn-for="flex-direction" data-dfn-type="value" data-export id="valdef-flex-direction-column-reverse">column-reverse</dfn>
    <dd> Same as <a class="css" data-link-type="maybe" href="#valdef-flex-direction-column" id="ref-for-valdef-flex-direction-column">column</a>,
			except the <a data-link-type="dfn" href="#main-start" id="ref-for-main-start③">main-start</a> and <a data-link-type="dfn" href="#main-end" id="ref-for-main-end③">main-end</a> directions are swapped.
   </dl>
   <p class="note" role="note"><span>Note:</span> The reverse values do not reverse box ordering:
	like <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css-writing-modes-4/#propdef-writing-mode" id="ref-for-propdef-writing-mode">writing-mode</a> and <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css-writing-modes-3/#propdef-direction" id="ref-for-propdef-direction">direction</a> <a data-link-type="biblio" href="#biblio-css3-writing-modes">[CSS3-WRITING-MODES]</a>,
	they only change the direction of flow.
	Painting order, speech order, and sequential navigation orders
	are not affected.</p>

</details>
<h3 class="heading settled" data-level="5.2" id="flex-wrap-property"><span class="secno">5.2. </span><span class="content"> Flex Line Wrapping: the <a class="property" data-link-type="propdesc" href="#propdef-flex-wrap" id="ref-for-propdef-flex-wrap①">flex-wrap</a> property</span><a class="self-link" href="#flex-wrap-property"></a></h3>
<!--__VALOR_STATUS:flex-wrap-property__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<table class="def propdef" data-link-for-hint="flex-wrap">
    <tbody>
     <tr>
      <th>Name:
      <td><dfn class="dfn-paneled css" data-dfn-type="property" data-export id="propdef-flex-wrap">flex-wrap</dfn>
     <tr class="value">
      <th><a href="https://www.w3.org/TR/css-values/#value-defs">Value:</a>
      <td class="prod">nowrap <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one④">|</a> wrap <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one⑤">|</a> wrap-reverse
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#initial-values">Initial:</a>
      <td>nowrap
     <tr>
      <th>Applies to:
      <td><a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container②①">flex containers</a>
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#inherited-property">Inherited:</a>
      <td>no
     <tr>
      <th><a href="https://www.w3.org/TR/css-values/#percentages">Percentages:</a>
      <td>n/a
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#computed">Computed value:</a>
      <td>specified keyword
     <tr>
      <th>Canonical order:
      <td>per grammar
     <tr>
      <th><a href="https://www.w3.org/TR/web-animations-1/#animation-type">Animation type:</a>
      <td>discrete
   </table>
   <p>The <a class="property" data-link-type="propdesc" href="#propdef-flex-wrap" id="ref-for-propdef-flex-wrap②">flex-wrap</a> property controls whether the flex container is <a data-link-type="dfn" href="#single-line-flex-container" id="ref-for-single-line-flex-container">single-line</a> or <a data-link-type="dfn" href="#multi-line-flex-container" id="ref-for-multi-line-flex-container">multi-line</a>,
	and the direction of the <a data-link-type="dfn" href="#cross-axis" id="ref-for-cross-axis③">cross-axis</a>,
	which determines the direction new lines are stacked in.</p>
   <dl>
    <dt><dfn class="dfn-paneled css" data-dfn-for="flex-wrap" data-dfn-type="value" data-export id="valdef-flex-wrap-nowrap">nowrap</dfn>
    <dd> The flex container is <a data-link-type="dfn" href="#single-line-flex-container" id="ref-for-single-line-flex-container①">single-line</a>.
    <dt><dfn class="dfn-paneled css" data-dfn-for="flex-wrap" data-dfn-type="value" data-export id="valdef-flex-wrap-wrap">wrap</dfn>
    <dd> The flex container is <a data-link-type="dfn" href="#multi-line-flex-container" id="ref-for-multi-line-flex-container①">multi-line</a>.
    <dt><dfn class="dfn-paneled css" data-dfn-for="flex-wrap" data-dfn-type="value" data-export id="valdef-flex-wrap-wrap-reverse">wrap-reverse</dfn>
    <dd> Same as <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap" id="ref-for-valdef-flex-wrap-wrap">wrap</a>.
   </dl>
   <p>For the values that are not <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap-reverse" id="ref-for-valdef-flex-wrap-wrap-reverse">wrap-reverse</a>,
	the <a data-link-type="dfn" href="#cross-start" id="ref-for-cross-start">cross-start</a> direction is equivalent to either
	the <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#inline-start" id="ref-for-inline-start②">inline-start</a> or <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#block-start" id="ref-for-block-start②">block-start</a> direction of the current <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#writing-mode" id="ref-for-writing-mode⑥">writing mode</a> (whichever is in the <a data-link-type="dfn" href="#cross-axis" id="ref-for-cross-axis④">cross axis</a>)
	and the <a data-link-type="dfn" href="#cross-end" id="ref-for-cross-end">cross-end</a> direction is the opposite direction of <a data-link-type="dfn" href="#cross-start" id="ref-for-cross-start①">cross-start</a>.
	When <a class="property" data-link-type="propdesc" href="#propdef-flex-wrap" id="ref-for-propdef-flex-wrap③">flex-wrap</a> is <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap-reverse" id="ref-for-valdef-flex-wrap-wrap-reverse①">wrap-reverse</a>,
	the <a data-link-type="dfn" href="#cross-start" id="ref-for-cross-start②">cross-start</a> and <a data-link-type="dfn" href="#cross-end" id="ref-for-cross-end①">cross-end</a> directions
	are swapped.</p>

</details>
<h3 class="heading settled" data-level="5.3" id="flex-flow-property"><span class="secno">5.3. </span><span class="content"> Flex Direction and Wrap: the <a class="property" data-link-type="propdesc" href="#propdef-flex-flow" id="ref-for-propdef-flex-flow②">flex-flow</a> shorthand</span><a class="self-link" href="#flex-flow-property"></a></h3>
<!--__VALOR_STATUS:flex-flow-property__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<table class="def propdef" data-link-for-hint="flex-flow">
    <tbody>
     <tr>
      <th>Name:
      <td><dfn class="dfn-paneled css" data-dfn-type="property" data-export id="propdef-flex-flow">flex-flow</dfn>
     <tr class="value">
      <th><a href="https://www.w3.org/TR/css-values/#value-defs">Value:</a>
      <td class="prod"><a class="production" data-link-type="propdesc" href="#propdef-flex-direction" id="ref-for-propdef-flex-direction④">&lt;‘flex-direction’></a> <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-any" id="ref-for-comb-any">||</a> <a class="production" data-link-type="propdesc" href="#propdef-flex-wrap" id="ref-for-propdef-flex-wrap④">&lt;‘flex-wrap’></a>
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#initial-values">Initial:</a>
      <td>see individual properties
     <tr>
      <th>Applies to:
      <td>see individual properties
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#inherited-property">Inherited:</a>
      <td>see individual properties
     <tr>
      <th><a href="https://www.w3.org/TR/css-values/#percentages">Percentages:</a>
      <td>see individual properties
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#computed">Computed value:</a>
      <td>see individual properties
     <tr>
      <th><a href="https://www.w3.org/TR/web-animations-1/#animation-type">Animation type:</a>
      <td>see individual properties
     <tr>
      <th>Canonical order:
      <td>per grammar
   </table>
   <p>The <a class="property" data-link-type="propdesc" href="#propdef-flex-flow" id="ref-for-propdef-flex-flow③">flex-flow</a> property is a shorthand for setting the <a class="property" data-link-type="propdesc" href="#propdef-flex-direction" id="ref-for-propdef-flex-direction⑤">flex-direction</a> and <a class="property" data-link-type="propdesc" href="#propdef-flex-wrap" id="ref-for-propdef-flex-wrap⑤">flex-wrap</a> properties,
	which together define the flex container’s main and cross axes.</p>
   <div class="example" id="example-c226b82c">
    <a class="self-link" href="#example-c226b82c"></a> Some examples of valid flows in an English (left-to-right, horizontal writing mode) document:
    <table style="margin: 0 auto; vertical-align: middle; border-spacing: 2em 1em;">
     <tbody>
      <tr>
       <td>
<pre class="lang-css highlight"><c- f>div </c-><c- p>{</c-> <c- k>flex-flow</c-><c- p>:</c-> row<c- p>;</c-> <c- p>}</c->
<c- c>/* Initial value. Main-axis is inline, no wrapping.</c->
<c- c>   (Items will either shrink to fit or overflow.) */</c->
</pre>
       <td><img alt height="46" src="images/flex-flow1.svg" width="205">
      <tr>
       <td>
<pre class="lang-css highlight"><c- f>div </c-><c- p>{</c-> <c- k>flex-flow</c-><c- p>:</c-> column wrap<c- p>;</c-> <c- p>}</c->
<c- c>/* Main-axis is block-direction (top to bottom)</c->
<c- c>   and lines wrap in the inline direction (rightwards). */</c->
</pre>
       <td><img alt height="160" src="images/flex-flow2.svg" width="89">
      <tr>
       <td>
<pre class="lang-css highlight"><c- f>div </c-><c- p>{</c-> <c- k>flex-flow</c-><c- p>:</c-> row-reverse wrap-reverse<c- p>;</c-> <c- p>}</c->
<c- c>/* Main-axis is the opposite of inline direction</c->
<c- c>   (right to left). New lines wrap upwards. */</c->
</pre>
       <td><img alt height="89" src="images/flex-flow3.svg" width="160">
    </table>
   </div>
   <div class="note" role="note">
     Note that the <a class="property" data-link-type="propdesc" href="#propdef-flex-flow" id="ref-for-propdef-flex-flow④">flex-flow</a> directions are <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#writing-mode" id="ref-for-writing-mode⑦">writing mode</a> sensitive.
		In vertical Japanese, for example,
		a <a class="css" data-link-type="maybe" href="#valdef-flex-direction-row" id="ref-for-valdef-flex-direction-row②">row</a> flex container lays out its contents from top to bottom,
		as seen in this example:
    <table style="margin: 1em auto; text-align: center;">
     <thead>
      <tr>
       <th>English
       <th>Japanese
     <tbody>
      <tr>
       <td>
<pre class="lang-css highlight">flex-flow: row wrap;        <br>writing-mode: horizontal-tb;</pre>
       <td>
<pre class="lang-css highlight">flex-flow: row wrap;        <br>writing-mode: vertical-rl;</pre>
      <tr>
       <td><img alt src="images/flex-flow-english.svg">
       <td><img alt src="images/flex-flow-japanese.svg">
    </table>
   </div>

</details>
<h3 class="heading settled" data-level="5.4" id="order-property"><span class="secno">5.4. </span><span class="content"> Display Order: the <a class="property" data-link-type="propdesc" href="#propdef-order" id="ref-for-propdef-order⑤">order</a> property</span><a class="self-link" href="#order-property"></a></h3>
<div data-valor-status="order-property">
  <p><strong>Status:</strong> [Production]</p>
  <p><strong>Code:</strong> <code>css_flexbox::order_key</code>, <code>css_flexbox::sort_items_by_order_stable</code></p>
  <p><strong>Fixtures:</strong> <em>Doc-only</em> (visual order effects will assert once layout lands)</p>
  <p><strong>Notes:</strong> Stable sort by <code>order</code> with DOM-order tie-breaking.</p>
  <p><strong>Spec:</strong> <a href="https://www.w3.org/TR/css-flexbox-1/#order-property">§5.4 order</a></p>
</div>

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<p><a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item②④">Flex items</a> are, by default, displayed and laid out in the same order as they appear in the source document.
	The <a class="property" data-link-type="propdesc" href="#propdef-order" id="ref-for-propdef-order⑥">order</a> property can be used to change this ordering.</p>
   <table class="def propdef" data-link-for-hint="order">
    <tbody>
     <tr>
      <th>Name:
      <td><dfn class="dfn-paneled css" data-dfn-type="property" data-export id="propdef-order">order</dfn>
     <tr class="value">
      <th><a href="https://www.w3.org/TR/css-values/#value-defs">Value:</a>
      <td class="prod"><a class="production css" data-link-type="type" href="https://www.w3.org/TR/css3-values/#integer-value" id="ref-for-integer-value">&lt;integer></a>
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#initial-values">Initial:</a>
      <td>0
     <tr>
      <th>Applies to:
      <td><a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item②⑤">flex items</a>
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#inherited-property">Inherited:</a>
      <td>no
     <tr>
      <th><a href="https://www.w3.org/TR/css-values/#percentages">Percentages:</a>
      <td>n/a
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#computed">Computed value:</a>
      <td>specified integer
     <tr>
      <th>Canonical order:
      <td>per grammar
     <tr>
      <th><a href="https://www.w3.org/TR/web-animations-1/#animation-type">Animation type:</a>
      <td>by computed value type
   </table>
   <p>The <a class="property" data-link-type="propdesc" href="#propdef-order" id="ref-for-propdef-order⑦">order</a> property controls the order in which
	flex items appear within the flex container,
	by assigning them to ordinal groups.
	It takes a single <dfn class="css" data-dfn-for="order" data-dfn-type="value" data-export id="valdef-order-integer"><a class="production css" data-link-type="type" href="https://www.w3.org/TR/css3-values/#integer-value" id="ref-for-integer-value①">&lt;integer></a><a class="self-link" href="#valdef-order-integer"></a></dfn> value,
	which specifies which ordinal group the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item②⑥">flex item</a> belongs to.</p>
   <p>A flex container lays out its content in <dfn class="dfn-paneled" data-dfn-type="dfn" data-export id="order-modified-document-order">order-modified document order</dfn>,
	starting from the lowest numbered ordinal group and going up.
	Items with the same ordinal group are laid out in the order they appear in the source document.
	This also affects the <a href="https://www.w3.org/TR/CSS2/zindex.html">painting order</a> <a data-link-type="biblio" href="#biblio-css21">[CSS21]</a>,
	exactly as if the flex items were reordered in the source document.
	Absolutely-positioned children of a <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container②②">flex container</a> are treated as having <a class="css" data-link-type="propdesc" href="#propdef-order" id="ref-for-propdef-order⑧">order: 0</a> for the purpose of determining their painting order relative to <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item②⑦">flex items</a>.</p>
   <div class="example" id="example-390e48aa">
    <a class="self-link" href="#example-390e48aa"></a> The following figure shows a simple tabbed interface, where the tab for the active pane is always first:
    <figure><img alt src="images/flex-order-example.png"></figure>
    <p>This could be implemented with the following CSS (showing only the relevant code):</p>
<pre class="lang-css highlight"><c- f>.tabs </c-><c- p>{</c->
  <c- k>display</c-><c- p>:</c-> flex<c- p>;</c->
<c- p>}</c->
<c- f>.tabs > .current </c-><c- p>{</c->
  <c- k>order</c-><c- p>:</c-> <c- m>-1</c-><c- p>;</c-> <c- c>/* Lower than the default of 0 */</c->
<c- p>}</c->
</pre>
   </div>
   <p>Unless otherwise specified by a future specification,
	this property has no effect on boxes that are not <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item②⑧">flex items</a>.</p>
   <h4 class="heading settled" data-level="5.4.1" id="order-accessibility"><span class="secno">5.4.1. </span><span class="content"> Reordering and Accessibility</span><a class="self-link" href="#order-accessibility"></a></h4>
   <p>The <a class="property" data-link-type="propdesc" href="#propdef-order" id="ref-for-propdef-order⑨">order</a> property <em>does not</em> affect ordering in non-visual media
	(such as <a href="https://www.w3.org/TR/css3-speech/">speech</a>).
	Likewise, <a class="property" data-link-type="propdesc" href="#propdef-order" id="ref-for-propdef-order①⓪">order</a> does not affect
	the default traversal order of sequential navigation modes
	(such as cycling through links, see e.g. <a href="https://html.spec.whatwg.org/multipage/interaction.html#attr-tabindex"><code>tabindex</code></a> <a data-link-type="biblio" href="#biblio-html">[HTML]</a>).</p>
   <p><strong class="advisement"> Authors <em>must</em> use <a class="property" data-link-type="propdesc" href="#propdef-order" id="ref-for-propdef-order①①">order</a> only for visual, not logical, reordering of content.
	Style sheets that use <a class="property" data-link-type="propdesc" href="#propdef-order" id="ref-for-propdef-order①②">order</a> to perform logical reordering are non-conforming.</strong></p>
   <p class="note" role="note"><span>Note:</span> This is so that non-visual media and non-CSS UAs,
	which typically present content linearly,
	can rely on a logical source order,
	while <a class="property" data-link-type="propdesc" href="#propdef-order" id="ref-for-propdef-order①③">order</a> is used to tailor the visual order.
	(Since visual perception is two-dimensional and non-linear,
	the desired visual order is not always logical.)</p>
   <div class="example" id="example-d528284a">
    <a class="self-link" href="#example-d528284a"></a> Many web pages have a similar shape in the markup,
		with a header on top,
		a footer on bottom,
		and then a content area and one or two additional columns in the middle.
		Generally,
		it’s desirable that the content come first in the page’s source code,
		before the additional columns.
		However, this makes many common designs,
		such as simply having the additional columns on the left and the content area on the right,
		difficult to achieve.
		This has been addressed in many ways over the years,
		often going by the name "Holy Grail Layout" when there are two additional columns. <a class="property" data-link-type="propdesc" href="#propdef-order" id="ref-for-propdef-order①④">order</a> makes this trivial.
		For example, take the following sketch of a page’s code and desired layout:
    <div class="code-and-figure">
     <div>
<pre class="lang-markup highlight"><c- cp>&lt;!DOCTYPE html></c->
<c- p>&lt;</c-><c- f>header</c-><c- p>></c->...<c- p>&lt;/</c-><c- f>header</c-><c- p>></c->
<c- p>&lt;</c-><c- f>main</c-><c- p>></c->
   <c- p>&lt;</c-><c- f>article</c-><c- p>></c->...<c- p>&lt;/</c-><c- f>article</c-><c- p>></c->
   <c- p>&lt;</c-><c- f>nav</c-><c- p>></c->...<c- p>&lt;/</c-><c- f>nav</c-><c- p>></c->
   <c- p>&lt;</c-><c- f>aside</c-><c- p>></c->...<c- p>&lt;/</c-><c- f>aside</c-><c- p>></c->
<c- p>&lt;/</c-><c- f>main</c-><c- p>></c->
<c- p>&lt;</c-><c- f>footer</c-><c- p>></c->...<c- p>&lt;/</c-><c- f>footer</c-><c- p>></c->
</pre>
     </div>
     <div><img alt="In this page the header is at the top and the footer at the bottom, but the article is in the center, flanked by the nav on the right and the aside on the left." height="360" src="images/flex-order-page.svg" width="400"></div>
    </div>
    <p>This layout can be easily achieved with flex layout:</p>
<pre class="lang-css highlight"><c- f>main </c-><c- p>{</c-> <c- k>display</c-><c- p>:</c-> flex<c- p>;</c-> <c- p>}</c->
<c- f>main > article </c-><c- p>{</c-> <c- k>order</c-><c- p>:</c-> <c- m>2</c-><c- p>;</c-> <c- k>min-width</c-><c- p>:</c-> <c- m>12</c-><c- l>em</c-><c- p>;</c-> <c- k>flex</c-><c- p>:</c-><c- m>1</c-><c- p>;</c-> <c- p>}</c->
<c- f>main > nav     </c-><c- p>{</c-> <c- k>order</c-><c- p>:</c-> <c- m>1</c-><c- p>;</c-> <c- k>width</c-><c- p>:</c-> <c- m>200</c-><c- l>px</c-><c- p>;</c-> <c- p>}</c->
<c- f>main > aside   </c-><c- p>{</c-> <c- k>order</c-><c- p>:</c-> <c- m>3</c-><c- p>;</c-> <c- k>width</c-><c- p>:</c-> <c- m>200</c-><c- l>px</c-><c- p>;</c-> <c- p>}</c->
</pre>
    <p>As an added bonus,
		the columns will all be <a class="css" data-link-type="value" href="#valdef-align-items-stretch" id="ref-for-valdef-align-items-stretch①">equal-height</a> by default,
		and the main content will be as wide as necessary to fill the screen.
		Additionally,
		this can then be combined with media queries to switch to an all-vertical layout on narrow screens:</p>
<pre class="lang-css highlight"><c- n>@media</c-> all and <c- p>(</c->max-width<c- f>: 600px) </c-><c- p>{</c->
  <c- c>/* Too narrow to support three columns */</c->
  main { flex-flow: column; }
  main > article, main > nav, main > aside {
    /* Return them to document order */
    order: 0; width: auto;
  }
}
</pre>
    <p><small>(Further use of multi-line flex containers to achieve even more intelligent wrapping left as an exercise for the reader.)</small></p>
   </div>
   <p>In order to preserve the author’s intended ordering in all presentation modes,
	authoring tools—including WYSIWYG editors as well as Web-based authoring aids—<wbr>must reorder the underlying document source
	and not use <a class="property" data-link-type="propdesc" href="#propdef-order" id="ref-for-propdef-order①⑤">order</a> to perform reordering
	unless the author has explicitly indicated that the underlying
	document order (which determines speech and navigation order) should be <em>out-of-sync</em> with the visual order.</p>
   <div class="example" id="example-cc8d5acf">
    <a class="self-link" href="#example-cc8d5acf"></a> For example, a tool might offer both drag-and-drop reordering of flex items
		as well as handling of media queries for alternate layouts per screen size range.
    <p>Since most of the time, reordering should affect all screen ranges
		as well as navigation and speech order,
		the tool would perform drag-and-drop reordering at the DOM layer.
		In some cases, however, the author may want different visual orderings per screen size.
		The tool could offer this functionality by using <a class="property" data-link-type="propdesc" href="#propdef-order" id="ref-for-propdef-order①⑥">order</a> together with media queries,
		but also tie the smallest screen size’s ordering to the underlying DOM order
		(since this is most likely to be a logical linear presentation order)
		while using <a class="property" data-link-type="propdesc" href="#propdef-order" id="ref-for-propdef-order①⑦">order</a> to determine the visual presentation order in other size ranges.</p>
    <p>This tool would be conformant, whereas a tool that only ever used <a class="property" data-link-type="propdesc" href="#propdef-order" id="ref-for-propdef-order①⑧">order</a> to handle drag-and-drop reordering
		(however convenient it might be to implement it that way)
		would be non-conformant.</p>
   </div>
   <p class="note" role="note"><span>Note:</span> User agents, including browsers, accessible technology, and extensions,
	may offer spatial navigation features.
	This section does not preclude respecting the <a class="property" data-link-type="propdesc" href="#propdef-order" id="ref-for-propdef-order①⑨">order</a> property
	when determining element ordering in such spatial navigation modes;
	indeed it would need to be considered for such a feature to work.
	But <a class="property" data-link-type="propdesc" href="#propdef-order" id="ref-for-propdef-order②⓪">order</a> is not the only (or even the primary) CSS property
	that would need to be considered for such a spatial navigation feature.
	A well-implemented spatial navigation feature would need to consider
	all the layout features of CSS that modify spatial relationships.</p>

</details>
<h2 class="heading settled" data-level="6" id="flex-lines"><span class="secno">6. </span><span class="content"> Flex Lines</span><a class="self-link" href="#flex-lines"></a></h2>
<!--__VALOR_STATUS:flex-lines__-->

<details class="valor-spec" data-level="2">
  <summary>Show spec text</summary>

<p><a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item②⑨">Flex items</a> in a <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container②③">flex container</a> are laid out and aligned
	within <dfn class="dfn-paneled" data-dfn-type="dfn" data-export data-lt="flex line" id="flex-line">flex lines</dfn>,
	hypothetical containers used for grouping and alignment by the layout algorithm.
	A flex container can be either <a data-link-type="dfn" href="#single-line-flex-container" id="ref-for-single-line-flex-container②">single-line</a> or <a data-link-type="dfn" href="#multi-line-flex-container" id="ref-for-multi-line-flex-container②">multi-line</a>,
	depending on the <a class="property" data-link-type="propdesc" href="#propdef-flex-wrap" id="ref-for-propdef-flex-wrap⑥">flex-wrap</a> property:</p>
   <ul>
    <li data-md>
     <p>A <dfn class="dfn-paneled" data-dfn-type="dfn" data-export data-local-lt="single-line" id="single-line-flex-container">single-line flex container</dfn> (i.e. one with <a class="css" data-link-type="propdesc" href="#propdef-flex-wrap" id="ref-for-propdef-flex-wrap⑦">flex-wrap: nowrap</a>)
lays out all of its children in a single line,
even if that would cause its contents to overflow.</p>
    <li data-md>
     <p>A <dfn class="dfn-paneled" data-dfn-type="dfn" data-export data-local-lt="multi-line" id="multi-line-flex-container">multi-line flex container</dfn> (i.e. one with <a class="css" data-link-type="propdesc" href="#propdef-flex-wrap" id="ref-for-propdef-flex-wrap⑧">flex-wrap: wrap</a> or <a class="css" data-link-type="propdesc" href="#propdef-flex-wrap" id="ref-for-propdef-flex-wrap⑨">flex-wrap: wrap-reverse</a>)
breaks its <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item③⓪">flex items</a> across multiple lines,
similar to how text is broken onto a new line when it gets too wide to fit on the existing line.
When additional lines are created,
they are stacked in the flex container along the <a data-link-type="dfn" href="#cross-axis" id="ref-for-cross-axis⑤">cross axis</a> according to the <a class="property" data-link-type="propdesc" href="#propdef-flex-wrap" id="ref-for-propdef-flex-wrap①⓪">flex-wrap</a> property.
Every line contains at least one <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item③①">flex item</a>,
unless the flex container itself is completely empty.</p>
   </ul>
   <div class="example" id="example-b7468756">
    <a class="self-link" href="#example-b7468756"></a> This example shows four buttons that do not fit side-by-side horizontally,
		and therefore will wrap into multiple lines.
<pre class="lang-css highlight"><c- f>#flex </c-><c- p>{</c->
  <c- k>display</c-><c- p>:</c-> flex<c- p>;</c->
  <c- k>flex-flow</c-><c- p>:</c-> row wrap<c- p>;</c->
  <c- k>width</c-><c- p>:</c-> <c- m>300</c-><c- l>px</c-><c- p>;</c->
<c- p>}</c->
<c- f>.item </c-><c- p>{</c->
  <c- k>width</c-><c- p>:</c-> <c- m>80</c-><c- l>px</c-><c- p>;</c->
<c- p>}</c->
</pre>
<pre class="lang-markup highlight"><c- p>&lt;</c-><c- f>div</c-> <c- e>id</c-><c- o>=</c-><c- s>"flex"</c-><c- p>></c->
  <c- p>&lt;</c-><c- f>div</c-> <c- e>class</c-><c- o>=</c-><c- s>"item"</c-><c- p>></c->1<c- p>&lt;/</c-><c- f>div</c-><c- p>></c->
  <c- p>&lt;</c-><c- f>div</c-> <c- e>class</c-><c- o>=</c-><c- s>"item"</c-><c- p>></c->2<c- p>&lt;/</c-><c- f>div</c-><c- p>></c->
  <c- p>&lt;</c-><c- f>div</c-> <c- e>class</c-><c- o>=</c-><c- s>"item"</c-><c- p>></c->3<c- p>&lt;/</c-><c- f>div</c-><c- p>></c->
  <c- p>&lt;</c-><c- f>div</c-> <c- e>class</c-><c- o>=</c-><c- s>"item"</c-><c- p>></c->4<c- p>&lt;/</c-><c- f>div</c-><c- p>></c->
<c- p>&lt;/</c-><c- f>div</c-><c- p>></c->
</pre>
    <p>Since the container is 300px wide, only three of the items fit onto a single line.
		They take up 240px, with 60px left over of remaining space.
		Because the <a class="property" data-link-type="propdesc" href="#propdef-flex-flow" id="ref-for-propdef-flex-flow⑤">flex-flow</a> property specifies a <a data-link-type="dfn" href="#multi-line-flex-container" id="ref-for-multi-line-flex-container③">multi-line</a> flex container
		(due to the <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap" id="ref-for-valdef-flex-wrap-wrap①">wrap</a> keyword appearing in its value),
		the flex container will create an additional line to contain the last item.</p>
    <figure>
      <img src="images/multiline-no-flex.svg">
     <figcaption>An example rendering of the multi-line flex container.</figcaption>
    </figure>
   </div>
   <p>Once content is broken into lines,
	each line is laid out independently;
	flexible lengths and the <a class="property" data-link-type="propdesc" href="#propdef-justify-content" id="ref-for-propdef-justify-content①">justify-content</a> and <a class="property" data-link-type="propdesc" href="#propdef-align-self" id="ref-for-propdef-align-self⑥">align-self</a> properties only consider the items on a single line at a time.</p>
   <p>In a <a data-link-type="dfn" href="#multi-line-flex-container" id="ref-for-multi-line-flex-container④">multi-line</a> <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container②④">flex container</a> (even one with only a single line),
	the <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size②">cross size</a> of each line
	is the minimum size necessary to contain the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item③②">flex items</a> on the line
	(after alignment due to <a class="property" data-link-type="propdesc" href="#propdef-align-self" id="ref-for-propdef-align-self⑦">align-self</a>),
	and the lines are aligned within the flex container with the <a class="property" data-link-type="propdesc" href="#propdef-align-content" id="ref-for-propdef-align-content①">align-content</a> property.
	In a <a data-link-type="dfn" href="#single-line-flex-container" id="ref-for-single-line-flex-container③">single-line</a> <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container②⑤">flex container</a>,
	the <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size③">cross size</a> of the line is the <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size④">cross size</a> of the flex container,
	and <a class="property" data-link-type="propdesc" href="#propdef-align-content" id="ref-for-propdef-align-content②">align-content</a> has no effect.
	The <a data-link-type="dfn" href="#main-size" id="ref-for-main-size①">main size</a> of a line is always the same as the <a data-link-type="dfn" href="#main-size" id="ref-for-main-size②">main size</a> of the flex container’s content box.</p>
   <div class="example" id="example-5601e71c">
    <a class="self-link" href="#example-5601e71c"></a> Here’s the same example as the previous,
		except that the flex items have all been given <a class="css" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex①">flex: auto</a>.
		The first line has 60px of remaining space,
		and all of the items have the same flexibility,
		so each of the three items on that line will receive 20px of extra width,
		each ending up 100px wide.
		The remaining item is on a line of its own
		and will stretch to the entire width of the line, i.e. 300px.
    <figure>
      <img src="images/multiline-flex.svg">
     <figcaption> A rendering of the same as above,
				but with the items all given <a class="css" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex②">flex: auto</a>. </figcaption>
    </figure>
   </div>

</details>
<h2 class="heading settled" data-level="7" id="flexibility"><span class="secno">7. </span><span class="content"> Flexibility</span><a class="self-link" href="#flexibility"></a></h2>
<!--__VALOR_STATUS:flexibility__-->

<details class="valor-spec" data-level="2">
  <summary>Show spec text</summary>

<p>The defining aspect of flex layout is the ability to make the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item③③">flex items</a> “flex”,
	altering their width/height to fill the available space in the <a data-link-type="dfn" href="#main-dimension" id="ref-for-main-dimension③">main dimension</a>.
	This is done with the <a class="property" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex③">flex</a> property.
	A flex container distributes free space to its items (proportional to their <a data-link-type="dfn" href="#flex-flex-grow-factor" id="ref-for-flex-flex-grow-factor">flex grow factor</a>) to fill the container,
	or shrinks them (proportional to their <a data-link-type="dfn" href="#flex-flex-shrink-factor" id="ref-for-flex-flex-shrink-factor">flex shrink factor</a>) to prevent overflow.</p>
   <p>A <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item③④">flex item</a> is <dfn class="dfn-paneled" data-dfn-type="dfn" data-noexport id="fully-inflexible">fully inflexible</dfn> if both its <a class="property" data-link-type="propdesc" href="#propdef-flex-grow" id="ref-for-propdef-flex-grow">flex-grow</a> and <a class="property" data-link-type="propdesc" href="#propdef-flex-shrink" id="ref-for-propdef-flex-shrink">flex-shrink</a> values are zero,
	and <dfn class="dfn-paneled" data-dfn-type="dfn" data-noexport id="flexible">flexible</dfn> otherwise.</p>

</details>
<h3 class="heading settled" data-level="7.1" id="flex-property"><span class="secno">7.1. </span><span class="content"> The <a class="property" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex④">flex</a> Shorthand</span><a class="self-link" href="#flex-property"></a></h3>
<!--__VALOR_STATUS:flex-property__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<table class="def propdef" data-link-for-hint="flex">
    <tbody>
     <tr>
      <th>Name:
      <td><dfn class="dfn-paneled css" data-dfn-type="property" data-export id="propdef-flex">flex</dfn>
     <tr class="value">
      <th><a href="https://www.w3.org/TR/css-values/#value-defs">Value:</a>
      <td class="prod">none <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one⑥">|</a> [ <a class="production" data-link-type="propdesc" href="#propdef-flex-grow" id="ref-for-propdef-flex-grow①">&lt;‘flex-grow’></a> <a class="production" data-link-type="propdesc" href="#propdef-flex-shrink" id="ref-for-propdef-flex-shrink①">&lt;‘flex-shrink’></a><a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#mult-opt" id="ref-for-mult-opt">?</a> <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-any" id="ref-for-comb-any①">||</a> <a class="production" data-link-type="propdesc" href="#propdef-flex-basis" id="ref-for-propdef-flex-basis">&lt;‘flex-basis’></a> ]
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#initial-values">Initial:</a>
      <td>0 1 auto
     <tr>
      <th>Applies to:
      <td><a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item③⑤">flex items</a>
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#inherited-property">Inherited:</a>
      <td>no
     <tr>
      <th><a href="https://www.w3.org/TR/css-values/#percentages">Percentages:</a>
      <td>see individual properties
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#computed">Computed value:</a>
      <td>see individual properties
     <tr>
      <th><a href="https://www.w3.org/TR/web-animations-1/#animation-type">Animation type:</a>
      <td>by computed value type
     <tr>
      <th>Canonical order:
      <td>per grammar
   </table>
   <p>The <a class="property" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex⑤">flex</a> property specifies the components of a <dfn data-dfn-type="dfn" data-noexport id="flexible-length">flexible length<a class="self-link" href="#flexible-length"></a></dfn>:
	the <dfn class="dfn-paneled" data-dfn-type="dfn" data-lt="flex factor" data-noexport id="flex-factor">flex factors</dfn> (<a data-link-type="dfn" href="#flex-flex-grow-factor" id="ref-for-flex-flex-grow-factor①">grow</a> and <a data-link-type="dfn" href="#flex-flex-shrink-factor" id="ref-for-flex-flex-shrink-factor①">shrink</a>)
	and the <a data-link-type="dfn" href="#flex-flex-basis" id="ref-for-flex-flex-basis">flex basis</a>.
	When a box is a <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item③⑥">flex item</a>, <a class="property" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex⑥">flex</a> is consulted <em>instead of</em> the <a data-link-type="dfn" href="#main-size-property" id="ref-for-main-size-property①">main size property</a> to determine the <a data-link-type="dfn" href="#main-size" id="ref-for-main-size③">main size</a> of the box.
	If a box is not a <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item③⑦">flex item</a>, <a class="property" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex⑦">flex</a> has no effect.</p>
   <dl>
    <dt><dfn class="css" data-dfn-for="flex" data-dfn-type="value" data-export id="valdef-flex-flex-grow"><a class="production" data-link-type="propdesc" href="#propdef-flex-grow" id="ref-for-propdef-flex-grow②">&lt;‘flex-grow’></a><a class="self-link" href="#valdef-flex-flex-grow"></a></dfn>
    <dd>
      This <a class="production css" data-link-type="type" href="https://www.w3.org/TR/css3-values/#number-value" id="ref-for-number-value">&lt;number></a> component sets <a class="property" data-link-type="propdesc" href="#propdef-flex-grow" id="ref-for-propdef-flex-grow③">flex-grow</a> <a href="#flex-components">longhand</a> and specifies the <dfn class="dfn-paneled" data-dfn-for="flex" data-dfn-type="dfn" data-noexport id="flex-flex-grow-factor">flex grow factor</dfn>,
			which determines how much the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item③⑧">flex item</a> will grow
			relative to the rest of the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item③⑨">flex items</a> in the flex container
			when positive free space is distributed.
			When omitted, it is set to <span class="css">1</span>.
     <details class="note">
      <summary>Flex values between 0 and 1 have a somewhat special behavior:
				when the sum of the flex values on the line is less than 1,
				they will take up less than 100% of the free space.</summary>
      <p>An item’s <a class="property" data-link-type="propdesc" href="#propdef-flex-grow" id="ref-for-propdef-flex-grow④">flex-grow</a> value
				is effectively a request for some proportion of the free space,
				with <span class="css">1</span> meaning “100% of the free space”;
				then if the items on the line are requesting more than 100% in total,
				the requests are rebalanced to keep the same ratio but use up exactly 100% of it.
				However, if the items request <em>less</em> than the full amount
				(such as three items that are each <a class="css" data-link-type="propdesc" href="#propdef-flex-grow" id="ref-for-propdef-flex-grow⑤">flex-grow: .25</a>)
				then they’ll each get exactly what they request
				(25% of the free space to each,
				with the final 25% left unfilled).
				See <a href="#resolve-flexible-lengths">§9.7 Resolving Flexible Lengths</a> for the exact details
				of how free space is distributed.</p>
      <p>This pattern is required for continuous behavior as <a class="property" data-link-type="propdesc" href="#propdef-flex-grow" id="ref-for-propdef-flex-grow⑥">flex-grow</a> approaches zero
				(which means the item wants <em>none</em> of the free space).
				Without this, a <a class="css" data-link-type="propdesc" href="#propdef-flex-grow" id="ref-for-propdef-flex-grow⑦">flex-grow: 1</a> item would take all of the free space;
				but so would a <a class="css" data-link-type="propdesc" href="#propdef-flex-grow" id="ref-for-propdef-flex-grow⑧">flex-grow: 0.1</a> item,
				and a <a class="css" data-link-type="propdesc" href="#propdef-flex-grow" id="ref-for-propdef-flex-grow⑨">flex-grow: 0.01</a> item,
				etc.,
				until finally the value is small enough to underflow to zero
				and the item suddenly takes up none of the free space.
				With this behavior,
				the item instead gradually takes less of the free space
				as <a class="property" data-link-type="propdesc" href="#propdef-flex-grow" id="ref-for-propdef-flex-grow①⓪">flex-grow</a> shrinks below <span class="css">1</span>,
				smoothly transitioning to taking none of the free space at zero.</p>
      <p>Unless this “partial fill” behavior is <em>specifically</em> what’s desired,
				authors should stick to values ≥ 1;
				for example, using <span class="css">1</span> and <span class="css">2</span> is usually better
				than using <span class="css">.33</span> and <span class="css">.67</span>,
				as they’re more likely to behave as intended
				if items are added, removed, or line-wrapped.</p>
     </details>
    <dt><dfn class="css" data-dfn-for="flex" data-dfn-type="value" data-export id="valdef-flex-flex-shrink"><a class="production" data-link-type="propdesc" href="#propdef-flex-shrink" id="ref-for-propdef-flex-shrink②">&lt;‘flex-shrink’></a><a class="self-link" href="#valdef-flex-flex-shrink"></a></dfn>
    <dd>
      This <a class="production css" data-link-type="type" href="https://www.w3.org/TR/css3-values/#number-value" id="ref-for-number-value①">&lt;number></a> component sets <a class="property" data-link-type="propdesc" href="#propdef-flex-shrink" id="ref-for-propdef-flex-shrink③">flex-shrink</a> <a href="#flex-components">longhand</a> and specifies the <dfn class="dfn-paneled" data-dfn-for="flex" data-dfn-type="dfn" data-noexport id="flex-flex-shrink-factor">flex shrink factor</dfn>,
			which determines how much the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item④⓪">flex item</a> will shrink
			relative to the rest of the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item④①">flex items</a> in the flex container
			when negative free space is distributed.
			When omitted, it is set to <span class="css">1</span>.
     <p class="note" role="note"><span>Note:</span> The <a data-link-type="dfn" href="#flex-flex-shrink-factor" id="ref-for-flex-flex-shrink-factor②">flex shrink factor</a> is multiplied by the <a data-link-type="dfn" href="#flex-base-size" id="ref-for-flex-base-size">flex base size</a> when distributing negative space.
			This distributes negative space in proportion to how much the item is able to shrink,
			so that e.g. a small item won’t shrink to zero before a larger item has been noticeably reduced.</p>
    <dt><dfn class="css" data-dfn-for="flex" data-dfn-type="value" data-export id="valdef-flex-flex-basis"><a class="production" data-link-type="propdesc" href="#propdef-flex-basis" id="ref-for-propdef-flex-basis①">&lt;‘flex-basis’></a><a class="self-link" href="#valdef-flex-flex-basis"></a></dfn>
    <dd>
      This component sets the <a class="property" data-link-type="propdesc" href="#propdef-flex-basis" id="ref-for-propdef-flex-basis②">flex-basis</a> <a href="#flex-components">longhand</a>,
			which specifies the <dfn class="dfn-paneled" data-dfn-for="flex" data-dfn-type="dfn" data-noexport id="flex-flex-basis">flex basis</dfn>:
			the initial <a data-link-type="dfn" href="#main-size" id="ref-for-main-size④">main size</a> of the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item④②">flex item</a>,
			before free space is distributed according to the flex factors.
     <p><a class="production" data-link-type="propdesc" href="#propdef-flex-basis" id="ref-for-propdef-flex-basis③">&lt;‘flex-basis’></a> accepts the same values as the <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-width" id="ref-for-propdef-width④">width</a> and <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-height" id="ref-for-propdef-height③">height</a> properties
			(except that <a class="css" data-link-type="maybe" href="#valdef-flex-basis-auto" id="ref-for-valdef-flex-basis-auto">auto</a> is treated differently)
			plus the <a class="css" data-link-type="maybe" href="#valdef-flex-basis-content" id="ref-for-valdef-flex-basis-content">content</a> keyword:</p>
     <dl>
      <dt><dfn class="dfn-paneled css" data-dfn-for="flex-basis" data-dfn-type="value" data-export id="valdef-flex-basis-auto">auto</dfn>
      <dd> When specified on a <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item④③">flex item</a>, the <a class="css" data-link-type="maybe" href="#valdef-flex-basis-auto" id="ref-for-valdef-flex-basis-auto①">auto</a> keyword
					retrieves the value of the <a data-link-type="dfn" href="#main-size-property" id="ref-for-main-size-property②">main size property</a> as the used <a class="property" data-link-type="propdesc" href="#propdef-flex-basis" id="ref-for-propdef-flex-basis④">flex-basis</a>.
					If that value is itself <a class="css" data-link-type="maybe" href="#valdef-flex-basis-auto" id="ref-for-valdef-flex-basis-auto②">auto</a>, then the used value is <a class="css" data-link-type="maybe" href="#valdef-flex-basis-content" id="ref-for-valdef-flex-basis-content①">content</a>.
      <dt><dfn class="dfn-paneled css" data-dfn-for="flex-basis" data-dfn-type="value" data-export id="valdef-flex-basis-content">content</dfn>
      <dd>
        Indicates an <a href="#algo-main-item">automatic size</a> based on the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item④④">flex item</a>’s content.
					(It is typically equivalent to the <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#max-content" id="ref-for-max-content">max-content size</a>,
					but with adjustments to handle aspect ratios,
					intrinsic sizing constraints,
					and orthogonal flows;
					see <a href="#algo-main-item">details</a> in <a href="#layout-algorithm">§9 Flex Layout Algorithm</a>.)
       <p class="note" role="note"><span>Note:</span> This value was not present in the initial release of Flexible Box Layout,
					and thus some older implementations will not support it.
					The equivalent effect can be achieved by using <span class="css">auto</span> together with a main size (<a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-width" id="ref-for-propdef-width⑤">width</a> or <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-height" id="ref-for-propdef-height④">height</a>) of <a class="css" data-link-type="maybe" href="https://www.w3.org/TR/css-sizing-3/#valdef-width-auto">auto</a>.</p>
      <dt><a class="production" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-width" id="ref-for-propdef-width⑥">&lt;‘width’></a>
      <dd> For all other values, <a class="property" data-link-type="propdesc" href="#propdef-flex-basis" id="ref-for-propdef-flex-basis⑤">flex-basis</a> is resolved the same way as for <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-width" id="ref-for-propdef-width⑦">width</a> and <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-height" id="ref-for-propdef-height⑤">height</a>.
     </dl>
     <p>When omitted from the <a class="property" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex⑧">flex</a> shorthand, its specified value is <span class="css">0</span>.</p>
    <dt><dfn class="dfn-paneled css" data-dfn-for="flex" data-dfn-type="value" data-export id="valdef-flex-none">none</dfn>
    <dd> The keyword <a class="css" data-link-type="maybe" href="#valdef-flex-none" id="ref-for-valdef-flex-none">none</a> expands to <span class="css">0 0 auto</span>.
   </dl>
   <figure>
     <img height="240" src="images/rel-vs-abs-flex.svg" width="504">
    <figcaption> A diagram showing the difference between "absolute" flex
			(starting from a basis of zero)
			and "relative" flex
			(starting from a basis of the item’s content size).
			The three items have flex factors of <span class="css">1</span>, <span class="css">1</span>, and <span class="css">2</span>, respectively:
			notice that the item with a flex factor of <span class="css">2</span> grows twice as fast as the others. </figcaption>
   </figure>
   <p>The initial values of the <a class="property" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex⑨">flex</a> components are equivalent to <a class="css-code" href="#flex-initial">flex: 0 1 auto</a>.</p>
   <p class="note" role="note"><span>Note:</span> The initial values of <a class="property" data-link-type="propdesc" href="#propdef-flex-grow" id="ref-for-propdef-flex-grow①①">flex-grow</a> and <a class="property" data-link-type="propdesc" href="#propdef-flex-basis" id="ref-for-propdef-flex-basis⑥">flex-basis</a> are different from their defaults when omitted in the <a class="property" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex①⓪">flex</a> shorthand.
	This is so that the <a class="property" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex①①">flex</a> shorthand can better accommodate the most <a href="#flex-common">common cases</a>.</p>
   <p>A unitless zero that is not already preceded by two flex factors
	must be interpreted as a flex factor.
	To avoid misinterpretation or invalid declarations,
	authors must specify a zero <a class="production" data-link-type="propdesc" href="#propdef-flex-basis" id="ref-for-propdef-flex-basis⑦">&lt;‘flex-basis’></a> component
	with a unit or precede it by two flex factors.</p>
   <h4 class="heading settled" data-level="7.1.1" id="flex-common"><span class="secno">7.1.1. </span><span class="content"> Basic Values of <a class="property" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex①②">flex</a></span><a class="self-link" href="#flex-common"></a></h4>
   <p><em>This section is informative.</em></p>
   <p>The list below summarizes the effects of the four <a class="property" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex①③">flex</a> values
	that represent most commonly-desired effects:</p>
   <dl>
    <dt id="flex-initial"><a class="self-link" href="#flex-initial"></a><a class="css" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex①④">flex: initial</a>
    <dd> Equivalent to <a class="css" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex①⑤">flex: 0 1 auto</a>. (This is the initial value.)
			Sizes the item based on the <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-width" id="ref-for-propdef-width⑧">width</a>/<a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-height" id="ref-for-propdef-height⑥">height</a> properties.
			(If the item’s <a data-link-type="dfn" href="#main-size-property" id="ref-for-main-size-property③">main size property</a> computes to <a class="css" data-link-type="value" href="https://www.w3.org/TR/css-sizing-3/#valdef-width-auto" id="ref-for-valdef-width-auto①">auto</a>,
			this will size the flex item based on its contents.)
			Makes the flex item inflexible when there is positive free space,
			but allows it to shrink to its minimum size when there is insufficient space.
			The <a href="#alignment">alignment abilities</a> or <a href="#auto-margins"><span class="css">auto</span> margins</a> can be used to align flex items along the <a data-link-type="dfn" href="#main-axis" id="ref-for-main-axis⑧">main axis</a>.
    <dt><a class="css" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex①⑥">flex: auto</a>
    <dd> Equivalent to <a class="css" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex①⑦">flex: 1 1 auto</a>.
			Sizes the item based on the <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-width" id="ref-for-propdef-width⑨">width</a>/<a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-height" id="ref-for-propdef-height⑦">height</a> properties,
			but makes them fully flexible, so that they absorb any free space along the <a data-link-type="dfn" href="#main-axis" id="ref-for-main-axis⑨">main axis</a>.
			If all items are either <a class="css" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex①⑧">flex: auto</a>, <a class="css" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex①⑨">flex: initial</a>, or <a class="css" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex②⓪">flex: none</a>,
			any positive free space after the items have been sized will be distributed evenly to the items with <a class="css" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex②①">flex: auto</a>.
    <dt><a class="css" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex②②">flex: none</a>
    <dd> Equivalent to <a class="css" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex②③">flex: 0 0 auto</a>.
			This value sizes the item according to the <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-width" id="ref-for-propdef-width①⓪">width</a>/<a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-height" id="ref-for-propdef-height⑧">height</a> properties,
			but makes the flex item <a data-link-type="dfn" href="#fully-inflexible" id="ref-for-fully-inflexible">fully inflexible</a>.
			This is similar to <a class="css" data-link-type="maybe" href="https://www.w3.org/TR/css-cascade-4/#valdef-all-initial" id="ref-for-valdef-all-initial">initial</a>,
			except that flex items are not allowed to shrink,
			even in overflow situations.
    <dt><a class="css" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex②④">flex: &lt;positive-number></a>
    <dd> Equivalent to <a class="css" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex②⑤">flex: &lt;positive-number> 1 0</a>.
			Makes the flex item flexible and sets the <a data-link-type="dfn" href="#flex-flex-basis" id="ref-for-flex-flex-basis①">flex basis</a> to zero,
			resulting in an item that receives the specified proportion of the free space in the flex container.
			If all items in the flex container use this pattern,
			their sizes will be proportional to the specified flex factor.
   </dl>
   <p>By default, flex items won’t shrink below their minimum content size
	(the length of the longest word or fixed-size element).
	To change this, set the <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-min-width" id="ref-for-propdef-min-width④">min-width</a> or <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-min-height" id="ref-for-propdef-min-height③">min-height</a> property.
	(See <a href="#min-size-auto">§4.5 Automatic Minimum Size of Flex Items</a>.)</p>

</details>
<h3 class="heading settled" data-level="7.2" id="flex-components"><span class="secno">7.2. </span><span class="content"> Components of Flexibility</span><a class="self-link" href="#flex-components"></a></h3>
<!--__VALOR_STATUS:flex-components__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<p>Individual components of flexibility can be controlled by independent longhand properties.</p>
   <p><strong class="advisement"> Authors are encouraged to control flexibility using the <a class="property" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex②⑥">flex</a> shorthand
	rather than with its longhand properties directly,
	as the shorthand correctly resets any unspecified components
	to accommodate <a href="#flex-common">common uses</a>.</strong></p>
   <h4 class="heading settled" data-level="7.2.1" id="flex-grow-property"><span class="secno">7.2.1. </span><span class="content"> The <a class="property" data-link-type="propdesc" href="#propdef-flex-grow" id="ref-for-propdef-flex-grow①②">flex-grow</a> property</span><a class="self-link" href="#flex-grow-property"></a></h4>
   <table class="def propdef" data-link-for-hint="flex-grow">
    <tbody>
     <tr>
      <th>Name:
      <td><dfn class="dfn-paneled css" data-dfn-type="property" data-export id="propdef-flex-grow">flex-grow</dfn>
     <tr class="value">
      <th><a href="https://www.w3.org/TR/css-values/#value-defs">Value:</a>
      <td class="prod"><a class="production css" data-link-type="type" href="https://www.w3.org/TR/css3-values/#number-value" id="ref-for-number-value②">&lt;number></a>
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#initial-values">Initial:</a>
      <td>0
     <tr>
      <th>Applies to:
      <td><a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item④⑤">flex items</a>
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#inherited-property">Inherited:</a>
      <td>no
     <tr>
      <th><a href="https://www.w3.org/TR/css-values/#percentages">Percentages:</a>
      <td>n/a
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#computed">Computed value:</a>
      <td>specified number
     <tr>
      <th>Canonical order:
      <td>per grammar
     <tr>
      <th><a href="https://www.w3.org/TR/web-animations-1/#animation-type">Animation type:</a>
      <td>by computed value type
   </table>
   <p><strong class="advisement"> Authors are encouraged to control flexibility using the <a class="property" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex②⑦">flex</a> shorthand
	rather than with <a class="property" data-link-type="propdesc" href="#propdef-flex-grow" id="ref-for-propdef-flex-grow①③">flex-grow</a> directly,
	as the shorthand correctly resets any unspecified components
	to accommodate <a href="#flex-common">common uses</a>.</strong></p>
   <p>The <a class="property" data-link-type="propdesc" href="#propdef-flex-grow" id="ref-for-propdef-flex-grow①④">flex-grow</a> property sets the <a data-link-type="dfn" href="#flex-flex-grow-factor" id="ref-for-flex-flex-grow-factor②">flex grow factor</a> to the provided <dfn class="css" data-dfn-for="flex-grow" data-dfn-type="value" data-export id="valdef-flex-grow-number"><a class="production css" data-link-type="type" href="https://www.w3.org/TR/css3-values/#number-value" id="ref-for-number-value③">&lt;number></a><a class="self-link" href="#valdef-flex-grow-number"></a></dfn>.
	Negative numbers are invalid.</p>
   <h4 class="heading settled" data-level="7.2.2" id="flex-shrink-property"><span class="secno">7.2.2. </span><span class="content"> The <a class="property" data-link-type="propdesc" href="#propdef-flex-shrink" id="ref-for-propdef-flex-shrink④">flex-shrink</a> property</span><a class="self-link" href="#flex-shrink-property"></a></h4>
   <table class="def propdef" data-link-for-hint="flex-shrink">
    <tbody>
     <tr>
      <th>Name:
      <td><dfn class="dfn-paneled css" data-dfn-type="property" data-export id="propdef-flex-shrink">flex-shrink</dfn>
     <tr class="value">
      <th><a href="https://www.w3.org/TR/css-values/#value-defs">Value:</a>
      <td class="prod"><a class="production css" data-link-type="type" href="https://www.w3.org/TR/css3-values/#number-value" id="ref-for-number-value④">&lt;number></a>
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#initial-values">Initial:</a>
      <td>1
     <tr>
      <th>Applies to:
      <td><a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item④⑥">flex items</a>
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#inherited-property">Inherited:</a>
      <td>no
     <tr>
      <th><a href="https://www.w3.org/TR/css-values/#percentages">Percentages:</a>
      <td>n/a
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#computed">Computed value:</a>
      <td>specified value
     <tr>
      <th>Canonical order:
      <td>per grammar
     <tr>
      <th><a href="https://www.w3.org/TR/web-animations-1/#animation-type">Animation type:</a>
      <td>number
   </table>
   <p><strong class="advisement"> Authors are encouraged to control flexibility using the <a class="property" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex②⑧">flex</a> shorthand
	rather than with <a class="property" data-link-type="propdesc" href="#propdef-flex-shrink" id="ref-for-propdef-flex-shrink⑤">flex-shrink</a> directly,
	as the shorthand correctly resets any unspecified components
	to accommodate <a href="#flex-common">common uses</a>.</strong></p>
   <p>The <a class="property" data-link-type="propdesc" href="#propdef-flex-shrink" id="ref-for-propdef-flex-shrink⑥">flex-shrink</a> property sets the <a data-link-type="dfn" href="#flex-flex-shrink-factor" id="ref-for-flex-flex-shrink-factor③">flex shrink factor</a> to the provided <dfn class="css" data-dfn-for="flex-shrink" data-dfn-type="value" data-export id="valdef-flex-shrink-number"><a class="production css" data-link-type="type" href="https://www.w3.org/TR/css3-values/#number-value" id="ref-for-number-value⑤">&lt;number></a><a class="self-link" href="#valdef-flex-shrink-number"></a></dfn>.
	Negative numbers are invalid.</p>
   <h4 class="heading settled" data-level="7.2.3" id="flex-basis-property"><span class="secno">7.2.3. </span><span class="content"> The <a class="property" data-link-type="propdesc" href="#propdef-flex-basis" id="ref-for-propdef-flex-basis⑧">flex-basis</a> property</span><a class="self-link" href="#flex-basis-property"></a></h4>
   <table class="def propdef" data-link-for-hint="flex-basis">
    <tbody>
     <tr>
      <th>Name:
      <td><dfn class="dfn-paneled css" data-dfn-type="property" data-export id="propdef-flex-basis">flex-basis</dfn>
     <tr class="value">
      <th><a href="https://www.w3.org/TR/css-values/#value-defs">Value:</a>
      <td class="prod">content <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one⑦">|</a> <a class="production" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-width" id="ref-for-propdef-width①①">&lt;‘width’></a>
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#initial-values">Initial:</a>
      <td>auto
     <tr>
      <th>Applies to:
      <td><a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item④⑦">flex items</a>
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#inherited-property">Inherited:</a>
      <td>no
     <tr>
      <th><a href="https://www.w3.org/TR/css-values/#percentages">Percentages:</a>
      <td>relative to the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container②⑥">flex container’s</a> inner <a data-link-type="dfn" href="#main-size" id="ref-for-main-size⑤">main size</a>
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#computed">Computed value:</a>
      <td>specified keyword or a computed <a class="production css" data-link-type="type" href="https://www.w3.org/TR/css-values-4/#typedef-length-percentage" id="ref-for-typedef-length-percentage">&lt;length-percentage></a> value
     <tr>
      <th>Canonical order:
      <td>per grammar
     <tr>
      <th><a href="https://www.w3.org/TR/web-animations-1/#animation-type">Animation type:</a>
      <td>by computed value type
   </table>
   <p><strong class="advisement"> Authors are encouraged to control flexibility using the <a class="property" data-link-type="propdesc" href="#propdef-flex" id="ref-for-propdef-flex②⑨">flex</a> shorthand
	rather than with <a class="property" data-link-type="propdesc" href="#propdef-flex-basis" id="ref-for-propdef-flex-basis⑨">flex-basis</a> directly,
	as the shorthand correctly resets any unspecified components
	to accommodate <a href="#flex-common">common uses</a>.</strong></p>
   <p>The <a class="property" data-link-type="propdesc" href="#propdef-flex-basis" id="ref-for-propdef-flex-basis①⓪">flex-basis</a> property sets the <a data-link-type="dfn" href="#flex-flex-basis" id="ref-for-flex-flex-basis②">flex basis</a>.
	It accepts the same values as the <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-width" id="ref-for-propdef-width①②">width</a> and <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-height" id="ref-for-propdef-height⑨">height</a> property, plus <a class="css" data-link-type="maybe" href="#valdef-flex-basis-content" id="ref-for-valdef-flex-basis-content②">content</a>.</p>
   <p>For all values other than <a class="css" data-link-type="maybe" href="#valdef-flex-basis-auto" id="ref-for-valdef-flex-basis-auto③">auto</a> and <a class="css" data-link-type="maybe" href="#valdef-flex-basis-content" id="ref-for-valdef-flex-basis-content③">content</a> (defined above), <a class="property" data-link-type="propdesc" href="#propdef-flex-basis" id="ref-for-propdef-flex-basis①①">flex-basis</a> is resolved the same way as <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-width" id="ref-for-propdef-width①③">width</a> in horizontal writing modes <a data-link-type="biblio" href="#biblio-css21">[CSS21]</a>,
	except that if a value would resolve to <a class="css" data-link-type="maybe" href="https://www.w3.org/TR/css-sizing-3/#valdef-width-auto" id="ref-for-valdef-width-auto②">auto</a> for <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-width" id="ref-for-propdef-width①④">width</a>,
	it instead resolves to <a class="css" data-link-type="maybe" href="#valdef-flex-basis-content" id="ref-for-valdef-flex-basis-content④">content</a> for <a class="property" data-link-type="propdesc" href="#propdef-flex-basis" id="ref-for-propdef-flex-basis①②">flex-basis</a>.
	For example, percentage values of <a class="property" data-link-type="propdesc" href="#propdef-flex-basis" id="ref-for-propdef-flex-basis①③">flex-basis</a> are resolved against
	the flex item’s containing block (i.e. its <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container②⑦">flex container</a>);
	and if that containing block’s size is <a data-link-type="dfn" href="#definite" id="ref-for-definite⑦">indefinite</a>,
	the used value for <a class="property" data-link-type="propdesc" href="#propdef-flex-basis" id="ref-for-propdef-flex-basis①④">flex-basis</a> is <a class="css" data-link-type="maybe" href="#valdef-flex-basis-content" id="ref-for-valdef-flex-basis-content⑤">content</a>.
	As another corollary, <a class="property" data-link-type="propdesc" href="#propdef-flex-basis" id="ref-for-propdef-flex-basis①⑤">flex-basis</a> determines the size of the content box,
	unless otherwise specified
	such as by <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css-sizing-3/#propdef-box-sizing" id="ref-for-propdef-box-sizing">box-sizing</a> <a data-link-type="biblio" href="#biblio-css3ui">[CSS3UI]</a>.</p>

</details>
<h2 class="heading settled" data-level="8" id="alignment"><span class="secno">8. </span><span class="content"> Alignment</span><a class="self-link" href="#alignment"></a></h2>
<div data-valor-status="alignment">
  <p><strong>Status:</strong> [MVP]</p>
  <p><strong>Code:</strong> <code>css_flexbox::align_single_line_cross</code>, <code>css_flexbox::align_cross_for_items</code>, <code>css_flexbox::layout_single_line_with_cross</code>, <code>css_flexbox::layout_single_line</code> (justify)</p>
  <p><strong>Fixtures:</strong> <code>layout/flex/11_align_items_center.html</code>, <code>layout/flex/12_justify_content_center.html</code>, <code>layout/flex/14_justify_space_between.html</code>, <code>layout/flex/gap.html</code></p>
  <p><strong>Notes:</strong> Single-line only; baseline and multi-line alignment not yet implemented. <span class="css">gap</span> asserted via geometry; fonts neutralized to avoid flakiness.</p>
  <p><strong>Spec:</strong> <a href="https://www.w3.org/TR/css-flexbox-1/#alignment">§8 Alignment</a></p>
</div>

<details class="valor-spec" data-level="2">
  <summary>Show spec text</summary>

<p>After a flex container’s contents have finished their flexing
	and the dimensions of all flex items are finalized,
	they can then be aligned within the flex container.</p>
   <p>The <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/box.html#propdef-margin" id="ref-for-propdef-margin">margin</a> properties can be used to align items in a manner similar to, but more powerful than, what margins can do in block layout. <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item④⑧">Flex items</a> also respect the alignment properties from <a href="https://www.w3.org/TR/css3-align/">CSS Box Alignment</a>,
	which allow easy keyword-based alignment of items in both the <a data-link-type="dfn" href="#main-axis" id="ref-for-main-axis①⓪">main axis</a> and <a data-link-type="dfn" href="#cross-axis" id="ref-for-cross-axis⑥">cross axis</a>.
	These properties make many common types of alignment trivial,
	including some things that were very difficult in CSS 2.1,
	like horizontal and vertical centering.</p>
   <p class="note" role="note"><span>Note:</span> While the alignment properties are defined in <a href="https://www.w3.org/TR/css3-align/">CSS Box Alignment</a> <a data-link-type="biblio" href="#biblio-css-align-3">[CSS-ALIGN-3]</a>,
	Flexible Box Layout reproduces the definitions of the relevant ones here
	so as to not create a normative dependency that may slow down advancement of the spec.
	These properties apply only to flex layout
	until <a href="https://www.w3.org/TR/css3-align/">CSS Box Alignment Level 3</a> is finished
	and defines their effect for other layout modes.
	Additionally, any new values defined in the Box Alignment module
	will apply to Flexible Box Layout;
	in otherwords, the Box Alignment module, once completed,
	will supercede the definitions here.</p>

</details>
<h3 class="heading settled" data-level="8.1" id="auto-margins"><span class="secno">8.1. </span><span class="content"> Aligning with <a class="css" data-link-type="value">auto</a> margins</span><a class="self-link" href="#auto-margins"></a></h3>
<!--__VALOR_STATUS:auto-margins__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<p><em>This section is non-normative.
		The normative definition of how margins affect flex items is in the <a href="#layout-algorithm">Flex Layout Algorithm</a> section.</em></p>
   <p>Auto margins on flex items have an effect very similar to auto margins in block flow:</p>
   <ul>
    <li data-md>
     <p>During calculations of flex bases and flexible lengths, auto margins are treated as <span class="css">0</span>.</p>
    <li data-md>
     <p>Prior to alignment via <a class="property" data-link-type="propdesc" href="#propdef-justify-content" id="ref-for-propdef-justify-content②">justify-content</a> and <a class="property" data-link-type="propdesc" href="#propdef-align-self" id="ref-for-propdef-align-self⑧">align-self</a>,
any positive free space is distributed to auto margins in that dimension.</p>
    <li data-md>
     <p>Overflowing boxes ignore their auto margins and overflow in the <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-3/#end" id="ref-for-end">end</a> direction.</p>
   </ul>
   <p class="note" role="note"><span>Note:</span> If free space is distributed to auto margins,
	the alignment properties will have no effect in that dimension
	because the margins will have stolen all the free space
	left over after flexing.</p>
   <div class="example" id="example-243a1d6d">
    <a class="self-link" href="#example-243a1d6d"></a> One use of <span class="css">auto</span> margins in the main axis is to separate flex items into distinct "groups".
		The following example shows how to use this to reproduce a common UI pattern -
		a single bar of actions with some aligned on the left and others aligned on the right.
    <figure>
     <figcaption> Sample rendering of the code below. </figcaption>
     <ul id="auto-bar">
      <li><a href="#">About</a>
      <li><a href="#">Projects</a>
      <li><a href="#">Interact</a>
      <li id="login"><a href="#">Login</a>
     </ul>
    </figure>
<pre class="lang-css highlight"><c- f>nav > ul </c-><c- p>{</c->
  <c- k>display</c-><c- p>:</c-> flex<c- p>;</c->
<c- p>}</c->
<c- f>nav > ul > #login </c-><c- p>{</c->
  <c- k>margin-left</c-><c- p>:</c-> auto<c- p>;</c->
<c- p>}</c->
</pre>
<pre class="lang-markup highlight"><c- p>&lt;</c-><c- f>nav</c-><c- p>></c->
  <c- p>&lt;</c-><c- f>ul</c-><c- p>></c->
    <c- p>&lt;</c-><c- f>li</c-><c- p>>&lt;</c-><c- f>a</c-> <c- e>href</c-><c- o>=</c-><c- s>/about</c-><c- p>></c->About<c- p>&lt;/</c-><c- f>a</c-><c- p>></c->
    <c- p>&lt;</c-><c- f>li</c-><c- p>>&lt;</c-><c- f>a</c-> <c- e>href</c-><c- o>=</c-><c- s>/projects</c-><c- p>></c->Projects<c- p>&lt;/</c-><c- f>a</c-><c- p>></c->
    <c- p>&lt;</c-><c- f>li</c-><c- p>>&lt;</c-><c- f>a</c-> <c- e>href</c-><c- o>=</c-><c- s>/interact</c-><c- p>></c->Interact<c- p>&lt;/</c-><c- f>a</c-><c- p>></c->
    <c- p>&lt;</c-><c- f>li</c-> <c- e>id</c-><c- o>=</c-><c- s>"login"</c-><c- p>>&lt;</c-><c- f>a</c-> <c- e>href</c-><c- o>=</c-><c- s>/login</c-><c- p>></c->Login<c- p>&lt;/</c-><c- f>a</c-><c- p>></c->
  <c- p>&lt;/</c-><c- f>ul</c-><c- p>></c->
<c- p>&lt;/</c-><c- f>nav</c-><c- p>></c->
</pre>
   </div>
   <div class="example" id="example-543514d9">
    <a class="self-link" href="#example-543514d9"></a> The figure below illustrates the difference in cross-axis alignment in overflow situations between
		using <a href="#auto-margins"><span class="css">auto</span> margins</a> and using the <a class="css" data-link-type="property" href="#propdef-align-items" id="ref-for-propdef-align-items①">alignment properties</a>.
    <figure>
     <div style="display:table; margin: 0 auto 1em;">
      <div class="cross-auto-figure" style="display:table-cell; padding-right: 50px;">
       <div>
        <div>About</div>
        <div>Authoritarianism</div>
        <div>Blog</div>
       </div>
      </div>
      <div class="cross-auto-figure" style="display:table-cell; padding-left: 50px;">
       <div>
        <div>About</div>
        <div style="margin-left: -31px;">Authoritarianism</div>
        <div>Blog</div>
       </div>
      </div>
     </div>
     <figcaption> The items in the figure on the left are centered with margins,
				while those in the figure on the right are centered with <a class="property" data-link-type="propdesc" href="#propdef-align-self" id="ref-for-propdef-align-self⑨">align-self</a>.
				If this column flex container was placed against the left edge of the page,
				the margin behavior would be more desirable,
				as the long item would be fully readable.
				In other circumstances,
				the true centering behavior might be better. </figcaption>
    </figure>
   </div>

</details>
<h3 class="heading settled" data-level="8.2" id="justify-content-property"><span class="secno">8.2. </span><span class="content"> Axis Alignment: the <a class="property" data-link-type="propdesc" href="#propdef-justify-content" id="ref-for-propdef-justify-content③">justify-content</a> property</span><a class="self-link" href="#justify-content-property"></a></h3>
<!--__VALOR_STATUS:justify-content-property__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<table class="def propdef" data-link-for-hint="justify-content">
    <tbody>
     <tr>
      <th>Name:
      <td><dfn class="dfn-paneled css" data-dfn-type="property" data-export id="propdef-justify-content">justify-content</dfn>
     <tr class="value">
      <th><a href="https://www.w3.org/TR/css-values/#value-defs">Value:</a>
      <td class="prod">flex-start <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one⑧">|</a> flex-end <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one⑨">|</a> center <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one①⓪">|</a> space-between <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one①①">|</a> space-around
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#initial-values">Initial:</a>
      <td>flex-start
     <tr>
      <th>Applies to:
      <td><a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container②⑧">flex containers</a>
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#inherited-property">Inherited:</a>
      <td>no
     <tr>
      <th><a href="https://www.w3.org/TR/css-values/#percentages">Percentages:</a>
      <td>n/a
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#computed">Computed value:</a>
      <td>specified keyword
     <tr>
      <th>Canonical order:
      <td>per grammar
     <tr>
      <th><a href="https://www.w3.org/TR/web-animations-1/#animation-type">Animation type:</a>
      <td>discrete
   </table>
   <p>The <a class="property" data-link-type="propdesc" href="#propdef-justify-content" id="ref-for-propdef-justify-content④">justify-content</a> property aligns <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item④⑨">flex items</a> along the <a data-link-type="dfn" href="#main-axis" id="ref-for-main-axis①①">main axis</a> of the current line of the flex container.
	This is done <em>after</em> any flexible lengths and any <a href="#auto-margins">auto margins</a> have been resolved.
	Typically it helps distribute extra free space leftover when either
	all the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑤⓪">flex items</a> on a line are inflexible,
	or are flexible but have reached their maximum size.
	It also exerts some control over the alignment of items when they overflow the line.</p>
   <dl>
    <dt><dfn class="dfn-paneled css" data-dfn-for="justify-content" data-dfn-type="value" data-export id="valdef-justify-content-flex-start">flex-start</dfn>
    <dd> <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑤①">Flex items</a> are packed toward the start of the line.
			The <a data-link-type="dfn" href="#main-start" id="ref-for-main-start④">main-start</a> margin edge of the first <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑤②">flex item</a> on the line
			is placed flush with the <a data-link-type="dfn" href="#main-start" id="ref-for-main-start⑤">main-start</a> edge of the line,
			and each subsequent <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑤③">flex item</a> is placed flush with the preceding item.
    <dt><dfn class="css" data-dfn-for="justify-content" data-dfn-type="value" data-export id="valdef-justify-content-flex-end">flex-end<a class="self-link" href="#valdef-justify-content-flex-end"></a></dfn>
    <dd> <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑤④">Flex items</a> are packed toward the end of the line.
			The <a data-link-type="dfn" href="#main-end" id="ref-for-main-end④">main-end</a> margin edge of the last <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑤⑤">flex item</a> is placed flush with the <a data-link-type="dfn" href="#main-end" id="ref-for-main-end⑤">main-end</a> edge of the line,
			and each preceding <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑤⑥">flex item</a> is placed flush with the subsequent item.
    <dt><dfn class="dfn-paneled css" data-dfn-for="justify-content" data-dfn-type="value" data-export id="valdef-justify-content-center">center</dfn>
    <dd> <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑤⑦">Flex items</a> are packed toward the center of the line.
			The <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑤⑧">flex items</a> on the line are placed flush with each other
			and aligned in the center of the line,
			with equal amounts of space between the <a data-link-type="dfn" href="#main-start" id="ref-for-main-start⑥">main-start</a> edge of the line and the first item on the line
			and between the <a data-link-type="dfn" href="#main-end" id="ref-for-main-end⑥">main-end</a> edge of the line and the last item on the line.
			(If the leftover free-space is negative,
			the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑤⑨">flex items</a> will overflow equally in both directions.)
    <dt><dfn class="css" data-dfn-for="justify-content" data-dfn-type="value" data-export id="valdef-justify-content-space-between">space-between<a class="self-link" href="#valdef-justify-content-space-between"></a></dfn>
    <dd> <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑥⓪">Flex items</a> are evenly distributed in the line.
			If the leftover free-space is negative
			or there is only a single <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑥①">flex item</a> on the line,
			this value is identical to <a class="css" data-link-type="value" href="#valdef-justify-content-flex-start" id="ref-for-valdef-justify-content-flex-start">flex-start</a>.
			Otherwise,
			the <a data-link-type="dfn" href="#main-start" id="ref-for-main-start⑦">main-start</a> margin edge of the first <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑥②">flex item</a> on the line
			is placed flush with the <a data-link-type="dfn" href="#main-start" id="ref-for-main-start⑧">main-start</a> edge of the line,
			the <a data-link-type="dfn" href="#main-end" id="ref-for-main-end⑦">main-end</a> margin edge of the last <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑥③">flex item</a> on the line
			is placed flush with the <a data-link-type="dfn" href="#main-end" id="ref-for-main-end⑧">main-end</a> edge of the line,
			and the remaining <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑥④">flex items</a> on the line are distributed
			so that the spacing between any two adjacent items is the same.
    <dt><dfn class="css" data-dfn-for="justify-content" data-dfn-type="value" data-export id="valdef-justify-content-space-around">space-around<a class="self-link" href="#valdef-justify-content-space-around"></a></dfn>
    <dd> <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑥⑤">Flex items</a> are evenly distributed in the line,
			with half-size spaces on either end.
			If the leftover free-space is negative or
			there is only a single <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑥⑥">flex item</a> on the line,
			this value is identical to <a class="css" data-link-type="value" href="#valdef-justify-content-center" id="ref-for-valdef-justify-content-center">center</a>.
			Otherwise, the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑥⑦">flex items</a> on the line are distributed
			such that the spacing between any two adjacent <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑥⑧">flex items</a> on the line is the same,
			and the spacing between the first/last <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑥⑨">flex items</a> and the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container②⑨">flex container</a> edges
			is half the size of the spacing between <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑦⓪">flex items</a>.
   </dl>
   <div class="figure">
     <img alt height="270" src="images/flex-pack.svg" width="504">
    <p class="caption">An illustration of the five <a class="property" data-link-type="propdesc" href="#propdef-justify-content" id="ref-for-propdef-justify-content⑤">justify-content</a> keywords and their effects on a flex container with three colored items. </p>
   </div>

</details>
<h3 class="heading settled" data-level="8.3" id="align-items-property"><span class="secno">8.3. </span><span class="content"> Cross-axis Alignment: the <a class="property" data-link-type="propdesc" href="#propdef-align-items" id="ref-for-propdef-align-items②">align-items</a> and <a class="property" data-link-type="propdesc" href="#propdef-align-self" id="ref-for-propdef-align-self①⓪">align-self</a> properties</span><a class="self-link" href="#align-items-property"></a></h3>
<!--__VALOR_STATUS:align-items-property__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<table class="def propdef" data-link-for-hint="align-items">
    <tbody>
     <tr>
      <th>Name:
      <td><dfn class="dfn-paneled css" data-dfn-type="property" data-export id="propdef-align-items">align-items</dfn>
     <tr class="value">
      <th><a href="https://www.w3.org/TR/css-values/#value-defs">Value:</a>
      <td class="prod">flex-start <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one①②">|</a> flex-end <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one①③">|</a> center <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one①④">|</a> baseline <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one①⑤">|</a> stretch
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#initial-values">Initial:</a>
      <td>stretch
     <tr>
      <th>Applies to:
      <td><a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container③⓪">flex containers</a>
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#inherited-property">Inherited:</a>
      <td>no
     <tr>
      <th><a href="https://www.w3.org/TR/css-values/#percentages">Percentages:</a>
      <td>n/a
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#computed">Computed value:</a>
      <td>specified keyword
     <tr>
      <th>Canonical order:
      <td>per grammar
     <tr>
      <th><a href="https://www.w3.org/TR/web-animations-1/#animation-type">Animation type:</a>
      <td>discrete
   </table>
   <table class="def propdef" data-link-for-hint="align-self">
    <tbody>
     <tr>
      <th>Name:
      <td><dfn class="dfn-paneled css" data-dfn-type="property" data-export id="propdef-align-self">align-self</dfn>
     <tr class="value">
      <th><a href="https://www.w3.org/TR/css-values/#value-defs">Value:</a>
      <td class="prod">auto <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one①⑥">|</a> flex-start <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one①⑦">|</a> flex-end <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one①⑧">|</a> center <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one①⑨">|</a> baseline <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one②⓪">|</a> stretch
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#initial-values">Initial:</a>
      <td>auto
     <tr>
      <th>Applies to:
      <td><a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑦①">flex items</a>
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#inherited-property">Inherited:</a>
      <td>no
     <tr>
      <th><a href="https://www.w3.org/TR/css-values/#percentages">Percentages:</a>
      <td>n/a
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#computed">Computed value:</a>
      <td>specified keyword
     <tr>
      <th>Canonical order:
      <td>per grammar
     <tr>
      <th><a href="https://www.w3.org/TR/web-animations-1/#animation-type">Animation type:</a>
      <td>discrete
   </table>
   <p><a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑦②">Flex items</a> can be aligned in the <a data-link-type="dfn" href="#cross-axis" id="ref-for-cross-axis⑦">cross axis</a> of the current line of the flex container,
	similar to <a class="property" data-link-type="propdesc" href="#propdef-justify-content" id="ref-for-propdef-justify-content⑥">justify-content</a> but in the perpendicular direction. <a class="property" data-link-type="propdesc" href="#propdef-align-items" id="ref-for-propdef-align-items③">align-items</a> sets the default alignment for all of the flex container’s <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑦③">items</a>,
	including anonymous <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑦④">flex items</a>. <a class="property" data-link-type="propdesc" href="#propdef-align-self" id="ref-for-propdef-align-self①①">align-self</a> allows this default alignment to be overridden for individual <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑦⑤">flex items</a>.
	(For anonymous flex items, <a class="property" data-link-type="propdesc" href="#propdef-align-self" id="ref-for-propdef-align-self①②">align-self</a> always matches the value of <a class="property" data-link-type="propdesc" href="#propdef-align-items" id="ref-for-propdef-align-items④">align-items</a> on their associated flex container.)</p>
   <p>If either of the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑦⑥">flex item’s</a> cross-axis margins are <a class="css" data-link-type="value">auto</a>, <a class="property" data-link-type="propdesc" href="#propdef-align-self" id="ref-for-propdef-align-self①③">align-self</a> has no effect.</p>
   <p>Values have the following meanings:</p>
   <dl>
    <dt><dfn class="dfn-paneled css" data-dfn-for="align-items, align-self" data-dfn-type="value" data-export id="valdef-align-items-auto">auto</dfn>
    <dd> Defers <a data-link-type="dfn" href="#cross-axis" id="ref-for-cross-axis⑧">cross-axis</a> alignment control
			to the value of <a class="property" data-link-type="propdesc" href="#propdef-align-items" id="ref-for-propdef-align-items⑤">align-items</a> on the parent box.
			(This is the initial value of <a class="property" data-link-type="propdesc" href="#propdef-align-self" id="ref-for-propdef-align-self①④">align-self</a>.)
    <dt><dfn class="dfn-paneled css" data-dfn-for="align-items, align-self" data-dfn-type="value" data-export id="valdef-align-items-flex-start">flex-start</dfn>
    <dd> The <a data-link-type="dfn" href="#cross-start" id="ref-for-cross-start③">cross-start</a> margin edge of the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑦⑦">flex item</a> is placed flush with the <a data-link-type="dfn" href="#cross-start" id="ref-for-cross-start④">cross-start</a> edge of the line.
    <dt><dfn class="css" data-dfn-for="align-items, align-self" data-dfn-type="value" data-export id="valdef-align-items-flex-end">flex-end<a class="self-link" href="#valdef-align-items-flex-end"></a></dfn>
    <dd> The <a data-link-type="dfn" href="#cross-end" id="ref-for-cross-end②">cross-end</a> margin edge of the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑦⑧">flex item</a> is placed flush with the <a data-link-type="dfn" href="#cross-end" id="ref-for-cross-end③">cross-end</a> edge of the line.
    <dt><dfn class="css" data-dfn-for="align-items, align-self" data-dfn-type="value" data-export id="valdef-align-items-center">center<a class="self-link" href="#valdef-align-items-center"></a></dfn>
    <dd> The <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑦⑨">flex item</a>’s margin box is centered in the <a data-link-type="dfn" href="#cross-axis" id="ref-for-cross-axis⑨">cross axis</a> within the line.
			(If the <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size⑤">cross size</a> of the flex line is less than that of the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑧⓪">flex item</a>,
			it will overflow equally in both directions.)
    <dt><dfn class="dfn-paneled css" data-dfn-for="align-items, align-self" data-dfn-type="value" data-export id="valdef-align-items-baseline">baseline</dfn>
    <dd> The <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑧①">flex item</a> <dfn class="dfn-paneled" data-dfn-for="align-items, align-self" data-dfn-type="dfn" data-noexport id="baseline-participation">participates in baseline alignment</dfn>:
			all participating <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑧②">flex items</a> on the line
			are aligned such that their baselines align,
			and the item with the largest distance between its baseline and its <a data-link-type="dfn" href="#cross-start" id="ref-for-cross-start⑤">cross-start</a> margin edge
			is placed flush against the <a data-link-type="dfn" href="#cross-start" id="ref-for-cross-start⑥">cross-start</a> edge of the line.
			If the item does not have a baseline in the necessary axis,
			then one is <a data-link-type="dfn" href="https://www.w3.org/TR/css3-align/#synthesize-baseline" id="ref-for-synthesize-baseline">synthesized</a> from the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑧③">flex item</a>’s border box.
    <dt><dfn class="dfn-paneled css" data-dfn-for="align-items, align-self" data-dfn-type="value" data-export id="valdef-align-items-stretch">stretch</dfn>
    <dd>
      If the <a data-link-type="dfn" href="#cross-size-property" id="ref-for-cross-size-property①">cross size property</a> of the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑧④">flex item</a> computes to <a class="css" data-link-type="maybe" href="https://www.w3.org/TR/css-sizing-3/#valdef-width-auto" id="ref-for-valdef-width-auto③">auto</a>,
			and neither of the <a data-link-type="dfn" href="#cross-axis" id="ref-for-cross-axis①⓪">cross-axis</a> margins are <span class="css">auto</span>,
			the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑧⑤">flex item</a> is <dfn class="dfn-paneled" data-dfn-for data-dfn-type="dfn" data-noexport id="stretched">stretched</dfn>.
			Its used value is the length necessary to make the <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size⑥">cross size</a> of the item’s margin box as close to the same size as the line as possible,
			while still respecting the constraints imposed by <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-min-height" id="ref-for-propdef-min-height④">min-height</a>/<a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-min-width" id="ref-for-propdef-min-width⑤">min-width</a>/<a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-max-height" id="ref-for-propdef-max-height②">max-height</a>/<a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-max-width" id="ref-for-propdef-max-width②">max-width</a>.
     <p class="note" role="note"><span>Note:</span> If the flex container’s height is constrained
			this value may cause the contents of the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑧⑥">flex item</a> to overflow the item.</p>
     <p>The <a data-link-type="dfn" href="#cross-start" id="ref-for-cross-start⑦">cross-start</a> margin edge of the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑧⑦">flex item</a> is placed flush with the <a data-link-type="dfn" href="#cross-start" id="ref-for-cross-start⑧">cross-start</a> edge of the line.</p>
   </dl>
   <div class="figure">
     <img alt height="377" src="images/flex-align.svg" width="508">
    <p class="caption">An illustration of the five <a class="property" data-link-type="propdesc" href="#propdef-align-items" id="ref-for-propdef-align-items⑥">align-items</a> keywords and their effects on a flex container with four colored items. </p>
   </div>

</details>
<h3 class="heading settled" data-level="8.4" id="align-content-property"><span class="secno">8.4. </span><span class="content"> Packing Flex Lines: the <a class="property" data-link-type="propdesc" href="#propdef-align-content" id="ref-for-propdef-align-content③">align-content</a> property</span><a class="self-link" href="#align-content-property"></a></h3>
<!--__VALOR_STATUS:align-content-property__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<table class="def propdef" data-link-for-hint="align-content">
    <tbody>
     <tr>
      <th>Name:
      <td><dfn class="dfn-paneled css" data-dfn-type="property" data-export id="propdef-align-content">align-content</dfn>
     <tr class="value">
      <th><a href="https://www.w3.org/TR/css-values/#value-defs">Value:</a>
      <td class="prod">flex-start <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one②①">|</a> flex-end <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one②②">|</a> center <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one②③">|</a> space-between <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one②④">|</a> space-around <a data-link-type="grammar" href="https://www.w3.org/TR/css-values-4/#comb-one" id="ref-for-comb-one②⑤">|</a> stretch
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#initial-values">Initial:</a>
      <td>stretch
     <tr>
      <th>Applies to:
      <td><a data-link-type="dfn" href="#multi-line-flex-container" id="ref-for-multi-line-flex-container⑤">multi-line</a> <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container③①">flex containers</a>
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#inherited-property">Inherited:</a>
      <td>no
     <tr>
      <th><a href="https://www.w3.org/TR/css-values/#percentages">Percentages:</a>
      <td>n/a
     <tr>
      <th><a href="https://www.w3.org/TR/css-cascade/#computed">Computed value:</a>
      <td>specified keyword
     <tr>
      <th>Canonical order:
      <td>per grammar
     <tr>
      <th><a href="https://www.w3.org/TR/web-animations-1/#animation-type">Animation type:</a>
      <td>discrete
   </table>
   <p>The <a class="property" data-link-type="propdesc" href="#propdef-align-content" id="ref-for-propdef-align-content④">align-content</a> property aligns a flex container’s lines within the flex container
	when there is extra space in the <a data-link-type="dfn" href="#cross-axis" id="ref-for-cross-axis①①">cross-axis</a>,
	similar to how <a class="property" data-link-type="propdesc" href="#propdef-justify-content" id="ref-for-propdef-justify-content⑦">justify-content</a> aligns individual items within the <a data-link-type="dfn" href="#main-axis" id="ref-for-main-axis①②">main-axis</a>.
	Note, this property has no effect on a <a data-link-type="dfn" href="#single-line-flex-container" id="ref-for-single-line-flex-container④">single-line</a> <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container③②">flex container</a>.
	Values have the following meanings:</p>
   <dl>
    <dt><dfn class="dfn-paneled css" data-dfn-for="align-content" data-dfn-type="value" data-export id="valdef-align-content-flex-start">flex-start</dfn>
    <dd> Lines are packed toward the start of the flex container.
			The <a data-link-type="dfn" href="#cross-start" id="ref-for-cross-start⑨">cross-start</a> edge of the first line in the flex container
			is placed flush with the <a data-link-type="dfn" href="#cross-start" id="ref-for-cross-start①⓪">cross-start</a> edge of the flex container,
			and each subsequent line is placed flush with the preceding line.
    <dt><dfn class="css" data-dfn-for="align-content" data-dfn-type="value" data-export id="valdef-align-content-flex-end">flex-end<a class="self-link" href="#valdef-align-content-flex-end"></a></dfn>
    <dd> Lines are packed toward the end of the flex container.
			The <a data-link-type="dfn" href="#cross-end" id="ref-for-cross-end④">cross-end</a> edge of the last line
			is placed flush with the <a data-link-type="dfn" href="#cross-end" id="ref-for-cross-end⑤">cross-end</a> edge of the flex container,
			and each preceding line is placed flush with the subsequent line.
    <dt><dfn class="dfn-paneled css" data-dfn-for="align-content" data-dfn-type="value" data-export id="valdef-align-content-center">center</dfn>
    <dd> Lines are packed toward the center of the flex container.
			The lines in the flex container are placed flush with each other
			and aligned in the center of the flex container,
			with equal amounts of space
			between the <a data-link-type="dfn" href="#cross-start" id="ref-for-cross-start①①">cross-start</a> content edge of the flex container
			and the first line in the flex container,
			and between the <a data-link-type="dfn" href="#cross-end" id="ref-for-cross-end⑥">cross-end</a> content edge of the flex container
			and the last line in the flex container.
			(If the leftover free-space is negative,
			the lines will overflow equally in both directions.)
    <dt><dfn class="css" data-dfn-for="align-content" data-dfn-type="value" data-export id="valdef-align-content-space-between">space-between<a class="self-link" href="#valdef-align-content-space-between"></a></dfn>
    <dd> Lines are evenly distributed in the flex container.
			If the leftover free-space is negative
			or there is only a single <a data-link-type="dfn" href="#flex-line" id="ref-for-flex-line①">flex line</a> in the flex container,
			this value is identical to <a class="css" data-link-type="value" href="#valdef-align-content-flex-start" id="ref-for-valdef-align-content-flex-start">flex-start</a>.
			Otherwise,
			the <a data-link-type="dfn" href="#cross-start" id="ref-for-cross-start①②">cross-start</a> edge of the first line in the flex container
			is placed flush with the <a data-link-type="dfn" href="#cross-start" id="ref-for-cross-start①③">cross-start</a> content edge of the flex container,
			the <a data-link-type="dfn" href="#cross-end" id="ref-for-cross-end⑦">cross-end</a> edge of the last line in the flex container
			is placed flush with the <a data-link-type="dfn" href="#cross-end" id="ref-for-cross-end⑧">cross-end</a> content edge of the flex container,
			and the remaining lines in the flex container are distributed
			so that the spacing between any two adjacent lines is the same.
    <dt><dfn class="css" data-dfn-for="align-content" data-dfn-type="value" data-export id="valdef-align-content-space-around">space-around<a class="self-link" href="#valdef-align-content-space-around"></a></dfn>
    <dd> Lines are evenly distributed in the flex container,
			with half-size spaces on either end.
			If the leftover free-space is negative
			this value is identical to <a class="css" data-link-type="value" href="#valdef-align-content-center" id="ref-for-valdef-align-content-center">center</a>.
			Otherwise, the lines in the flex container are distributed
			such that the spacing between any two adjacent lines is the same,
			and the spacing between the first/last lines and the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container③③">flex container</a> edges
			is half the size of the spacing between <a data-link-type="dfn" href="#flex-line" id="ref-for-flex-line②">flex lines</a>.
    <dt><dfn class="dfn-paneled css" data-dfn-for="align-content" data-dfn-type="value" data-export id="valdef-align-content-stretch">stretch</dfn>
    <dd> Lines stretch to take up the remaining space.
			If the leftover free-space is negative,
			this value is identical to <a class="css" data-link-type="value" href="#valdef-align-content-flex-start" id="ref-for-valdef-align-content-flex-start①">flex-start</a>.
			Otherwise,
			the free-space is split equally between all of the lines,
			increasing their cross size.
   </dl>
   <p class="note" role="note"><span>Note:</span> Only <a data-link-type="dfn" href="#multi-line-flex-container" id="ref-for-multi-line-flex-container⑥">multi-line</a> <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container③④">flex containers</a> ever have free space in the <a data-link-type="dfn" href="#cross-axis" id="ref-for-cross-axis①②">cross-axis</a> for lines to be aligned in,
	because in a <a data-link-type="dfn" href="#single-line-flex-container" id="ref-for-single-line-flex-container⑤">single-line</a> flex container
	the sole line automatically stretches to fill the space.</p>
   <div class="figure">
     <img alt height="508" src="images/align-content-example.svg" width="612">
    <p class="caption"> An illustration of the <a class="property" data-link-type="propdesc" href="#propdef-align-content" id="ref-for-propdef-align-content⑤">align-content</a> keywords and their effects on a <a data-link-type="dfn" href="#multi-line-flex-container" id="ref-for-multi-line-flex-container⑦">multi-line</a> flex container. </p>
   </div>

</details>
<h3 class="heading settled" data-level="8.5" id="flex-baselines"><span class="secno">8.5. </span><span class="content"> Flex Container Baselines</span><a class="self-link" href="#flex-baselines"></a></h3>
<!--__VALOR_STATUS:flex-baselines__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<p>In order for a <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container③⑤">flex container</a> to itself <a href="#baseline-participation" id="ref-for-baseline-participation">participate in baseline alignment</a> (e.g. when the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container③⑥">flex container</a> is itself a <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑧⑧">flex item</a> in an outer <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container③⑦">flex container</a>),
	it needs to submit the position of the baselines that will best represent its contents.
	To this end,
	the baselines of a flex container are determined as follows
	(after reordering with <a class="property" data-link-type="propdesc" href="#propdef-order" id="ref-for-propdef-order②①">order</a>,
	and taking <a class="property" data-link-type="propdesc" href="#propdef-flex-direction" id="ref-for-propdef-flex-direction⑥">flex-direction</a> into account):</p>
   <dl>
    <dt>first/last <dfn class="dfn-paneled" data-dfn-type="dfn" data-export data-lt="main-axis baseline set|first main-axis baseline set|last main-axis baseline set" id="main-axis-baseline"> main-axis baseline set</dfn>
    <dd>
      When the <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#inline-axis" id="ref-for-inline-axis①">inline axis</a> of the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container③⑧">flex container</a> matches its <a data-link-type="dfn" href="#main-axis" id="ref-for-main-axis①③">main axis</a>,
			its baselines are determined as follows:
     <ol>
      <li data-md>
       <p>If any of the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑧⑨">flex items</a> on the flex container’s startmost/endmost <a data-link-type="dfn" href="#flex-line" id="ref-for-flex-line③">flex line</a> <a href="#baseline-participation" id="ref-for-baseline-participation①">participate in baseline alignment</a>,
the flex container’s first/last <a data-link-type="dfn" href="#main-axis-baseline" id="ref-for-main-axis-baseline">main-axis baseline set</a> is <a data-link-type="dfn" href="https://www.w3.org/TR/css3-align/#generate-baselines" id="ref-for-generate-baselines">generated</a> from
the shared <a data-link-type="dfn" href="https://www.w3.org/TR/css3-align/#alignment-baseline" id="ref-for-alignment-baseline">alignment baseline</a> of those <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑨⓪">flex items</a>.</p>
      <li data-md>
       <p>Otherwise, if the flex container has at least one <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑨①">flex item</a>,
the flex container’s first/last <a data-link-type="dfn" href="#main-axis-baseline" id="ref-for-main-axis-baseline①">main-axis baseline set</a> is <a data-link-type="dfn" href="https://www.w3.org/TR/css3-align/#generate-baselines" id="ref-for-generate-baselines①">generated</a> from
the <a data-link-type="dfn" href="https://www.w3.org/TR/css3-align/#alignment-baseline" id="ref-for-alignment-baseline①">alignment baseline</a> of the startmost/endmost <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑨②">flex item</a>.
(If that item has no <a data-link-type="dfn" href="https://www.w3.org/TR/css3-align/#alignment-baseline" id="ref-for-alignment-baseline②">alignment baseline</a> parallel to the flex container’s <a data-link-type="dfn" href="#main-axis" id="ref-for-main-axis①④">main axis</a>,
then one is first <a data-link-type="dfn" href="https://www.w3.org/TR/css3-align/#synthesize-baseline" id="ref-for-synthesize-baseline①">synthesized</a> from its border edges.)</p>
      <li data-md>
       <p>Otherwise, the flex container has no first/last main-axis <a data-link-type="dfn" href="https://www.w3.org/TR/css3-align/#baseline-set" id="ref-for-baseline-set">baseline set</a>,
and one is <a data-link-type="dfn" href="https://www.w3.org/TR/css3-align/#synthesize-baseline" id="ref-for-synthesize-baseline②">synthesized</a> if needed
according to the rules of its <a data-link-type="dfn" href="https://www.w3.org/TR/css3-align/#shared-alignment-context" id="ref-for-shared-alignment-context">alignment context</a>.</p>
     </ol>
    <dt>first/last <dfn class="dfn-paneled" data-dfn-type="dfn" data-export data-lt="cross-axis baseline set|first cross-axis baseline set|last cross-axis baseline set" id="cross-axis-baseline"> cross-axis baseline set</dfn>
    <dd>
      When the <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#inline-axis" id="ref-for-inline-axis②">inline axis</a> of the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container③⑨">flex container</a> matches its <a data-link-type="dfn" href="#cross-axis" id="ref-for-cross-axis①③">cross axis</a>,
			its baselines are determined as follows:
     <ol>
      <li data-md>
       <p>If the flex container has at least one <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑨③">flex item</a>,
the flex container’s first/last <a data-link-type="dfn" href="#cross-axis-baseline" id="ref-for-cross-axis-baseline">cross-axis baseline set</a> is <a data-link-type="dfn" href="https://www.w3.org/TR/css3-align/#generate-baselines" id="ref-for-generate-baselines②">generated</a> from
the <a data-link-type="dfn" href="https://www.w3.org/TR/css3-align/#alignment-baseline" id="ref-for-alignment-baseline③">alignment baseline</a> of the startmost/endmost <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑨④">flex item</a>.
(If that item has no <a data-link-type="dfn" href="https://www.w3.org/TR/css3-align/#alignment-baseline" id="ref-for-alignment-baseline④">alignment baseline</a> parallel to the flex container’s <a data-link-type="dfn" href="#cross-axis" id="ref-for-cross-axis①④">cross axis</a>,
then one is first <a data-link-type="dfn" href="https://www.w3.org/TR/css3-align/#synthesize-baseline" id="ref-for-synthesize-baseline③">synthesized</a> from its border edges.)</p>
      <li data-md>
       <p>Otherwise, the flex container has no first/last cross-axis <a data-link-type="dfn" href="https://www.w3.org/TR/css3-align/#baseline-set" id="ref-for-baseline-set①">baseline set</a>,
and one is <a data-link-type="dfn" href="https://www.w3.org/TR/css3-align/#synthesize-baseline" id="ref-for-synthesize-baseline④">synthesized</a> if needed
according to the rules of its <a data-link-type="dfn" href="https://www.w3.org/TR/css3-align/#shared-alignment-context" id="ref-for-shared-alignment-context①">alignment context</a>.</p>
     </ol>
   </dl>
   <p>When calculating the baseline according to the above rules,
	if the box contributing a baseline has an <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css-overflow-3/#propdef-overflow" id="ref-for-propdef-overflow①">overflow</a> value that allows scrolling,
	the box must be treated as being in its initial scroll position
	for the purpose of determining its baseline.</p>
   <p>When <a href="https://www.w3.org/TR/CSS2/tables.html#height-layout">determining the baseline of a table cell</a>,
	a flex container provides a baseline just as a line box or table-row does. <a data-link-type="biblio" href="#biblio-css21">[CSS21]</a></p>
   <p>See <a href="https://www.w3.org/TR/css-writing-modes-3/#intro-baselines">CSS Writing Modes 3 §4.1 Introduction to Baselines</a> and <a href="https://www.w3.org/TR/css3-align/#baseline-rules">CSS Box Alignment 3 §9 Baseline Alignment Details</a> for more information on baselines.</p>

</details>
<h2 class="heading settled" data-level="9" id="layout-algorithm"><span class="secno">9. </span><span class="content"> Flex Layout Algorithm</span><a class="self-link" href="#layout-algorithm"></a></h2>
<!--__VALOR_STATUS:layout-algorithm__-->

<details class="valor-spec" data-level="2">
  <summary>Show spec text</summary>

<p>This section contains normative algorithms
	detailing the exact layout behavior of a flex container and its contents.
	The algorithms here are written to optimize readability and theoretical simplicity,
	and may not necessarily be the most efficient.
	Implementations may use whatever actual algorithms they wish,
	but must produce the same results as the algorithms described here.</p>
   <p class="note" role="note"><span>Note:</span> This section is mainly intended for implementors.
	Authors writing web pages should generally be served well by the individual property descriptions,
	and do not need to read this section unless they have a deep-seated urge to understand arcane details of CSS layout.</p>
   <p>The following sections define the algorithm for laying out a flex container and its contents.</p>
   <p class="note" role="note"><span>Note:</span> Flex layout works with the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑨⑤">flex items</a> in <a data-link-type="dfn" href="#order-modified-document-order" id="ref-for-order-modified-document-order">order-modified document order</a>,
	not their original document order.</p>

</details>
<h3 class="heading settled" data-level="9.1" id="box-manip"><span class="secno">9.1. </span><span class="content"> Initial Setup</span><a class="self-link" href="#box-manip"></a></h3>
<!--__VALOR_STATUS:box-manip__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<ol class="layout-start" start="1">
    <li id="algo-anon-box"><a class="self-link" href="#algo-anon-box"></a> <strong>Generate anonymous flex items</strong> as described in <a href="#flex-items">§4 Flex Items</a>.
   </ol>

</details>
<h3 class="heading settled" data-level="9.2" id="line-sizing"><span class="secno">9.2. </span><span class="content"> Line Length Determination</span><a class="self-link" href="#line-sizing"></a></h3>
<!--__VALOR_STATUS:line-sizing__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<ol class="continue">
    <li id="algo-available">
     <a class="self-link" href="#algo-available"></a> <strong>Determine the available main and cross space for the flex items.</strong> For each dimension,
			if that dimension of the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container④⓪">flex container</a>’s content box is a <a data-link-type="dfn" href="#definite" id="ref-for-definite⑧">definite size</a>, use that;
			if that dimension of the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container④①">flex container</a> is being sized under a <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#min-content-constraint" id="ref-for-min-content-constraint">min</a> or <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#max-content-constraint" id="ref-for-max-content-constraint">max-content constraint</a>,
			the available space in that dimension is that constraint;
			otherwise, subtract the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container④②">flex container</a>’s margin, border, and padding
			from the space available to the flex container in that dimension
			and use that value. <span class="note">This might result in an infinite value.</span>
     <div class="example" id="example-34f93ad9">
      <a class="self-link" href="#example-34f93ad9"></a>
      <p>For example, the <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#available" id="ref-for-available">available space</a> to a flex item in a <a href="https://www.w3.org/TR/CSS2/visuren.html#floats">floated</a> <a class="css" data-link-type="value" href="https://www.w3.org/TR/css-sizing-3/#valdef-width-auto" id="ref-for-valdef-width-auto④">auto</a>-sized <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container④③">flex container</a> is: </p>
      <ul>
       <li>the width of the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container④④">flex container</a>’s containing block minus the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container④⑤">flex container</a>’s margin, border, and padding in the horizontal dimension
       <li>infinite in the vertical dimension
      </ul>
     </div>
    <li id="algo-main-item">
     <a class="self-link" href="#algo-main-item"></a> <strong>Determine the <dfn class="dfn-paneled" data-dfn-type="dfn" data-noexport id="flex-base-size">flex base size</dfn> and <dfn class="dfn-paneled" data-dfn-type="dfn" data-noexport id="hypothetical-main-size">hypothetical main size</dfn> of each item:</strong>
     <ol type="A">
      <li> If the item has a <a data-link-type="dfn" href="#definite" id="ref-for-definite⑨">definite</a> used <a data-link-type="dfn" href="#flex-flex-basis" id="ref-for-flex-flex-basis③">flex basis</a>,
					that’s the <a data-link-type="dfn" href="#flex-base-size" id="ref-for-flex-base-size①">flex base size</a>.
      <li>
        If the flex item has ...
       <ul>
        <li>an intrinsic aspect ratio,
        <li>a used <a data-link-type="dfn" href="#flex-flex-basis" id="ref-for-flex-flex-basis④">flex basis</a> of <a class="css" data-link-type="maybe" href="#valdef-flex-basis-content" id="ref-for-valdef-flex-basis-content⑥">content</a>, and
        <li>a <a data-link-type="dfn" href="#definite" id="ref-for-definite①⓪">definite</a> <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size⑦">cross size</a>,
       </ul>
        then the <a data-link-type="dfn" href="#flex-base-size" id="ref-for-flex-base-size②">flex base size</a> is calculated from its inner <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size⑧">cross size</a> and the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑨⑥">flex item</a>’s intrinsic aspect ratio.
      <li> If the used <a data-link-type="dfn" href="#flex-flex-basis" id="ref-for-flex-flex-basis⑤">flex basis</a> is <a class="css" data-link-type="maybe" href="#valdef-flex-basis-content" id="ref-for-valdef-flex-basis-content⑦">content</a> or depends on its <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#available" id="ref-for-available①">available space</a>,
					and the flex container is being sized under a min-content or max-content constraint
					(e.g. when performing <a href="https://www.w3.org/TR/CSS2/tables.html#auto-table-layout">automatic table layout</a> <a data-link-type="biblio" href="#biblio-css21">[CSS21]</a>),
					size the item under that constraint.
					The <a data-link-type="dfn" href="#flex-base-size" id="ref-for-flex-base-size③">flex base size</a> is the item’s resulting <a data-link-type="dfn" href="#main-size" id="ref-for-main-size⑥">main size</a>.
      <li>
        Otherwise,
					if the used <a data-link-type="dfn" href="#flex-flex-basis" id="ref-for-flex-flex-basis⑥">flex basis</a> is <a class="css" data-link-type="maybe" href="#valdef-flex-basis-content" id="ref-for-valdef-flex-basis-content⑧">content</a> or depends on its <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#available" id="ref-for-available②">available space</a>,
					the available main size is infinite,
					and the flex item’s inline axis is parallel to the main axis,
					lay the item out using <a href="https://www.w3.org/TR/css3-writing-modes/#orthogonal-flows">the rules for a box in an orthogonal flow</a> <a data-link-type="biblio" href="#biblio-css3-writing-modes">[CSS3-WRITING-MODES]</a>.
					The <a data-link-type="dfn" href="#flex-base-size" id="ref-for-flex-base-size④">flex base size</a> is the item’s max-content <a data-link-type="dfn" href="#main-size" id="ref-for-main-size⑦">main size</a>.
       <p class="note" role="note"><span>Note:</span> This case occurs, for example,
					in an English document (horizontal <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#writing-mode" id="ref-for-writing-mode⑧">writing mode</a>)
					containing a column flex container
					containing a vertical Japanese (vertical <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#writing-mode" id="ref-for-writing-mode⑨">writing mode</a>) <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑨⑦">flex item</a>.</p>
      <li> Otherwise,
					size the item into the <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#available" id="ref-for-available③">available space</a> using its used <a data-link-type="dfn" href="#flex-flex-basis" id="ref-for-flex-flex-basis⑦">flex basis</a> in place of its <a data-link-type="dfn" href="#main-size" id="ref-for-main-size⑧">main size</a>,
					treating a value of <a class="css" data-link-type="maybe" href="#valdef-flex-basis-content" id="ref-for-valdef-flex-basis-content⑨">content</a> as <a class="css" data-link-type="maybe" href="https://www.w3.org/TR/css-sizing-3/#valdef-width-max-content" id="ref-for-valdef-width-max-content①">max-content</a>.
					If a <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size⑨">cross size</a> is needed to determine the <a data-link-type="dfn" href="#main-size" id="ref-for-main-size⑨">main size</a> (e.g. when the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑨⑧">flex item</a>’s <a data-link-type="dfn" href="#main-size" id="ref-for-main-size①⓪">main size</a> is in its block axis)
					and the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item⑨⑨">flex item</a>’s cross size is <a class="css" data-link-type="maybe" href="#valdef-flex-basis-auto" id="ref-for-valdef-flex-basis-auto④">auto</a> and not <a data-link-type="dfn" href="#definite" id="ref-for-definite①①">definite</a>,
					in this calculation use <span class="css">fit-content</span> as the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①⓪⓪">flex item</a>’s <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size①⓪">cross size</a>.
					The <a data-link-type="dfn" href="#flex-base-size" id="ref-for-flex-base-size⑤">flex base size</a> is the item’s resulting <a data-link-type="dfn" href="#main-size" id="ref-for-main-size①①">main size</a>.
     </ol>
     <p>When determining the <a data-link-type="dfn" href="#flex-base-size" id="ref-for-flex-base-size⑥">flex base size</a>,
			the item’s min and max <a data-link-type="dfn" href="#main-size" id="ref-for-main-size①②">main sizes</a> are ignored
			(no clamping occurs).
			Furthermore, the sizing calculations that floor the content box size at zero
			when applying <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css-sizing-3/#propdef-box-sizing" id="ref-for-propdef-box-sizing①">box-sizing</a> are also ignored.
			(For example, an item with a specified size of zero,
			positive padding, and <a class="css" data-link-type="propdesc" href="https://www.w3.org/TR/css-sizing-3/#propdef-box-sizing" id="ref-for-propdef-box-sizing②">box-sizing: border-box</a> will have an outer <a data-link-type="dfn" href="#flex-base-size" id="ref-for-flex-base-size⑦">flex base size</a> of zero—<wbr>and hence a negative inner <a data-link-type="dfn" href="#flex-base-size" id="ref-for-flex-base-size⑧">flex base size</a>.)</p>
     <p>The <a data-link-type="dfn" href="#hypothetical-main-size" id="ref-for-hypothetical-main-size">hypothetical main size</a> is the item’s <a data-link-type="dfn" href="#flex-base-size" id="ref-for-flex-base-size⑨">flex base size</a> clamped according to its <a data-link-type="dfn" href="https://www.w3.org/TR/css-cascade-4/#used-value" id="ref-for-used-value">used</a> min and max <a data-link-type="dfn" href="#main-size" id="ref-for-main-size①③">main sizes</a> (and flooring the content box size at zero).</p>
    <li id="algo-main-container"><a class="self-link" href="#algo-main-container"></a> <strong>Determine the <a data-link-type="dfn" href="#main-size" id="ref-for-main-size①④">main size</a> of the flex container</strong> using the rules of the formatting context in which it participates.
			For this computation, <a class="css" data-link-type="value">auto</a> margins on flex items are treated as <span class="css">0</span>.
   </ol>

</details>
<h3 class="heading settled" data-level="9.3" id="main-sizing"><span class="secno">9.3. </span><span class="content"> Main Size Determination</span><a class="self-link" href="#main-sizing"></a></h3>
<!--__VALOR_STATUS:main-sizing__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<ol class="continue">
    <li id="algo-line-break">
     <a class="self-link" href="#algo-line-break"></a> <strong>Collect flex items into flex lines:</strong>
     <ul>
      <li> If the flex container is <a data-link-type="dfn" href="#single-line-flex-container" id="ref-for-single-line-flex-container⑥">single-line</a>,
					collect all the flex items into a single flex line.
      <li>
        Otherwise,
					starting from the first uncollected item,
					collect consecutive items one by one
					until the first time that the <em>next</em> collected item
					would not fit into the flex container’s <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#inner-size" id="ref-for-inner-size">inner</a> main size
					(or until a forced break is encountered,
					see <a href="#pagination">§10 Fragmenting Flex Layout</a>).
					If the very first uncollected item wouldn’t fit,
					collect just it into the line.
       <p> For this step,
						the size of a flex item is its <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#outer-size" id="ref-for-outer-size">outer</a> <a data-link-type="dfn" href="#hypothetical-main-size" id="ref-for-hypothetical-main-size①">hypothetical main size</a>. <span class="note">(Note: This can be negative.)</span> </p>
       <p> Repeat until all flex items have been collected into flex lines. </p>
       <p class="note" role="note"> Note that the "collect as many" line will collect zero-sized flex items
						onto the end of the previous line
						even if the last non-zero item exactly "filled up" the line. </p>
     </ul>
    <li id="algo-flex"><a class="self-link" href="#algo-flex"></a> <strong><a href="#resolve-flexible-lengths">Resolve the flexible lengths</a></strong> of all the flex items
			to find their used <a data-link-type="dfn" href="#main-size" id="ref-for-main-size①⑤">main size</a>. See <a href="#resolve-flexible-lengths">§9.7 Resolving Flexible Lengths</a>.
   </ol>

</details>
<h3 class="heading settled" data-level="9.4" id="cross-sizing"><span class="secno">9.4. </span><span class="content"> Cross Size Determination</span><a class="self-link" href="#cross-sizing"></a></h3>
<!--__VALOR_STATUS:cross-sizing__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<ol class="continue">
    <li id="algo-cross-item"><a class="self-link" href="#algo-cross-item"></a> <strong>Determine the <dfn class="dfn-paneled" data-dfn-type="dfn" data-noexport id="hypothetical-cross-size">hypothetical cross size</dfn> of each item</strong> by performing layout with the used <a data-link-type="dfn" href="#main-size" id="ref-for-main-size①⑥">main size</a> and the available space,
			treating <a class="css" data-link-type="value" href="https://www.w3.org/TR/css-sizing-3/#valdef-width-auto" id="ref-for-valdef-width-auto⑤">auto</a> as <span class="css">fit-content</span>.
    <li id="algo-cross-line">
     <a class="self-link" href="#algo-cross-line"></a> <strong>Calculate the cross size of each flex line.</strong>
     <p> If the flex container is <a data-link-type="dfn" href="#single-line-flex-container" id="ref-for-single-line-flex-container⑦">single-line</a> and has a <a data-link-type="dfn" href="#definite" id="ref-for-definite①②">definite</a> <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size①①">cross size</a>,
				the <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size①②">cross size</a> of the <a data-link-type="dfn" href="#flex-line" id="ref-for-flex-line④">flex line</a> is the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container④⑥">flex container</a>’s inner <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size①③">cross size</a>. </p>
     <p> Otherwise,
				for each flex line: </p>
     <ol>
      <li> Collect all the flex items whose inline-axis is parallel to the main-axis,
					whose <a class="property" data-link-type="propdesc" href="#propdef-align-self" id="ref-for-propdef-align-self①⑤">align-self</a> is <a class="css" data-link-type="maybe" href="#valdef-align-items-baseline" id="ref-for-valdef-align-items-baseline">baseline</a>,
					and whose cross-axis margins are both non-<a class="css" data-link-type="value">auto</a>.
					Find the largest of the distances between each item’s baseline and its hypothetical outer cross-start edge,
					and the largest of the distances between each item’s baseline and its hypothetical outer cross-end edge,
					and sum these two values.
      <li> Among all the items not collected by the previous step,
					find the largest outer <a data-link-type="dfn" href="#hypothetical-cross-size" id="ref-for-hypothetical-cross-size">hypothetical cross size</a>.
      <li>
        The used cross-size of the <a data-link-type="dfn" href="#flex-line" id="ref-for-flex-line⑤">flex line</a> is the largest of the numbers found in the previous two steps and zero.
       <p>If the flex container is <a data-link-type="dfn" href="#single-line-flex-container" id="ref-for-single-line-flex-container⑧">single-line</a>,
					then clamp the line’s cross-size to be within
					the container’s computed min and max <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size①④">cross sizes</a>. <span class="note">Note that if CSS 2.1’s definition of min/max-width/height applied more generally,
					this behavior would fall out automatically.</span></p>
     </ol>
    <li id="algo-line-stretch"><a class="self-link" href="#algo-line-stretch"></a> <strong>Handle 'align-content: stretch'.</strong> If the flex container has a <a data-link-type="dfn" href="#definite" id="ref-for-definite①③">definite</a> cross size, <a class="property" data-link-type="propdesc" href="#propdef-align-content" id="ref-for-propdef-align-content⑥">align-content</a> is <a class="css" data-link-type="value" href="#valdef-align-content-stretch" id="ref-for-valdef-align-content-stretch">stretch</a>,
			and the sum of the flex lines' cross sizes is less than the flex container’s inner cross size,
			increase the cross size of each flex line by equal amounts
			such that the sum of their cross sizes exactly equals the flex container’s inner cross size.
    <li id="algo-visibility">
     <a class="self-link" href="#algo-visibility"></a> <strong>Collapse <span class="css">visibility:collapse</span> items.</strong> If any flex items have <a class="css" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visufx.html#propdef-visibility" id="ref-for-propdef-visibility③">visibility: collapse</a>,
			note the cross size of the line they’re in as the item’s <dfn data-dfn-type="dfn" data-noexport id="strut-size">strut size<a class="self-link" href="#strut-size"></a></dfn>,
			and restart layout from the beginning.
     <p> In this second layout round,
				when <a href="#algo-line-break">collecting items into lines</a>,
				treat the collapsed items as having zero <a data-link-type="dfn" href="#main-size" id="ref-for-main-size①⑦">main size</a>.
				For the rest of the algorithm following that step,
				ignore the collapsed items entirely
				(as if they were <span class="css">display:none</span>)
				except that after <a href="#algo-cross-line">calculating the cross size of the lines</a>,
				if any line’s cross size is less than
				the largest <var>strut size</var> among all the collapsed items in the line,
				set its cross size to that <var>strut size</var>. </p>
     <p> Skip this step in the second layout round. </p>
    <li id="algo-stretch">
     <a class="self-link" href="#algo-stretch"></a> <strong>Determine the used cross size of each flex item.</strong> If a flex item has <a class="css" data-link-type="propdesc" href="#propdef-align-self" id="ref-for-propdef-align-self①⑥">align-self: stretch</a>,
			its computed cross size property is <a class="css" data-link-type="value" href="https://www.w3.org/TR/css-sizing-3/#valdef-width-auto" id="ref-for-valdef-width-auto⑥">auto</a>,
			and neither of its cross-axis margins are <a class="css" data-link-type="value">auto</a>,
			the used outer cross size is the used cross size of its flex line,
			clamped according to the item’s used min and max <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size①⑤">cross sizes</a>.
			Otherwise,
			the used cross size is the item’s <a data-link-type="dfn" href="#hypothetical-cross-size" id="ref-for-hypothetical-cross-size①">hypothetical cross size</a>.
     <p>If the flex item has <a class="css" data-link-type="propdesc" href="#propdef-align-self" id="ref-for-propdef-align-self①⑦">align-self: stretch</a>,
			redo layout for its contents,
			treating this used size as its definite cross size
			so that percentage-sized children can be resolved.</p>
     <p class="note" role="note"> Note that this step does not affect the <a data-link-type="dfn" href="#main-size" id="ref-for-main-size①⑧">main size</a> of the flex item,
				even if it has an intrinsic aspect ratio. </p>
   </ol>

</details>
<h3 class="heading settled" data-level="9.5" id="main-alignment"><span class="secno">9.5. </span><span class="content"> Main-Axis Alignment</span><a class="self-link" href="#main-alignment"></a></h3>
<!--__VALOR_STATUS:main-alignment__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<ol class="continue">
    <li id="algo-main-align">
     <a class="self-link" href="#algo-main-align"></a> <strong>Distribute any remaining free space.</strong> For each flex line:
     <ol>
      <li> If the remaining free space is positive
					and at least one main-axis margin on this line is <a class="css" data-link-type="value">auto</a>,
					distribute the free space equally among these margins.
					Otherwise, set all <a class="css" data-link-type="value">auto</a> margins to zero.
      <li> Align the items along the main-axis per <a class="property" data-link-type="propdesc" href="#propdef-justify-content" id="ref-for-propdef-justify-content⑧">justify-content</a>.
     </ol>
   </ol>

</details>
<h3 class="heading settled" data-level="9.6" id="cross-alignment"><span class="secno">9.6. </span><span class="content"> Cross-Axis Alignment</span><a class="self-link" href="#cross-alignment"></a></h3>
<!--__VALOR_STATUS:cross-alignment__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<ol class="continue">
    <li id="algo-cross-margins">
     <a class="self-link" href="#algo-cross-margins"></a> <strong>Resolve cross-axis <a class="css" data-link-type="value">auto</a> margins.</strong> If a flex item has <a class="css" data-link-type="value">auto</a> cross-axis margins:
     <ul>
      <li> If its outer cross size
					(treating those <a class="css" data-link-type="value">auto</a> margins as zero)
					is less than the cross size of its flex line,
					distribute the difference in those sizes equally
					to the <a class="css" data-link-type="value">auto</a> margins.
      <li> Otherwise,
					if the <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#block-start" id="ref-for-block-start③">block-start</a> or <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#inline-start" id="ref-for-inline-start③">inline-start</a> margin (whichever is in the cross axis)
					is <a class="css" data-link-type="value">auto</a>, set it to zero.
					Set the opposite margin so that the outer cross size of the item
					equals the cross size of its flex line.
     </ul>
    <li id="algo-cross-align"><a class="self-link" href="#algo-cross-align"></a> <strong>Align all flex items along the cross-axis</strong> per <a class="property" data-link-type="propdesc" href="#propdef-align-self" id="ref-for-propdef-align-self①⑧">align-self</a>,
			if neither of the item’s cross-axis margins are <a class="css" data-link-type="value">auto</a>.
    <li id="algo-cross-container">
     <a class="self-link" href="#algo-cross-container"></a> <strong>Determine the flex container’s used cross size:</strong>
     <ul>
      <li> If the cross size property is a <a data-link-type="dfn" href="#definite" id="ref-for-definite①④">definite</a> size,
					use that,
					clamped by the used min and max <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size①⑥">cross sizes</a> of the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container④⑦">flex container</a>.
      <li> Otherwise,
					use the sum of the flex lines' cross sizes,
					clamped by the used min and max <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size①⑦">cross sizes</a> of the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container④⑧">flex container</a>.
     </ul>
    <li id="algo-line-align"><a class="self-link" href="#algo-line-align"></a> <strong>Align all flex lines</strong> per <a class="property" data-link-type="propdesc" href="#propdef-align-content" id="ref-for-propdef-align-content⑦">align-content</a>.
   </ol>

</details>
<h3 class="heading settled" data-level="9.7" id="resolve-flexible-lengths"><span class="secno">9.7. </span><span class="content"> Resolving Flexible Lengths</span><a class="self-link" href="#resolve-flexible-lengths"></a></h3>
<!--__VALOR_STATUS:resolve-flexible-lengths__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<p> To resolve the flexible lengths of the items within a flex line: </p>
   <ol>
    <li> <strong>Determine the used flex factor</strong>.
			Sum the outer hypothetical main sizes of all items on the line.
			If the sum is less than the flex container’s inner <a data-link-type="dfn" href="#main-size" id="ref-for-main-size①⑨">main size</a>,
			use the flex grow factor for the rest of this algorithm;
			otherwise, use the flex shrink factor.
    <li>
      <strong>Size inflexible items.</strong> Freeze,
			setting its <dfn class="dfn-paneled" data-dfn-type="dfn" data-noexport id="target-main-size">target main size</dfn> to its <a data-link-type="dfn" href="#hypothetical-main-size" id="ref-for-hypothetical-main-size②">hypothetical main size</a>…
     <ul>
      <li> any item that has a flex factor of zero
      <li> if using the <a data-link-type="dfn" href="#flex-flex-grow-factor" id="ref-for-flex-flex-grow-factor③">flex grow factor</a>:
					any item that has a <a data-link-type="dfn" href="#flex-base-size" id="ref-for-flex-base-size①⓪">flex base size</a> greater than its <a data-link-type="dfn" href="#hypothetical-main-size" id="ref-for-hypothetical-main-size③">hypothetical main size</a>
      <li> if using the <a data-link-type="dfn" href="#flex-flex-shrink-factor" id="ref-for-flex-flex-shrink-factor④">flex shrink factor</a>:
					any item that has a <a data-link-type="dfn" href="#flex-base-size" id="ref-for-flex-base-size①①">flex base size</a> smaller than its <a data-link-type="dfn" href="#hypothetical-main-size" id="ref-for-hypothetical-main-size④">hypothetical main size</a>
     </ul>
    <li> <strong>Calculate <dfn class="dfn-paneled" data-dfn-type="dfn" data-noexport id="initial-free-space">initial free space</dfn>.</strong> Sum the outer sizes of all items on the line,
			and subtract this from the flex container’s inner <a data-link-type="dfn" href="#main-size" id="ref-for-main-size②⓪">main size</a>.
			For frozen items, use their outer <a data-link-type="dfn" href="#target-main-size" id="ref-for-target-main-size">target main size</a>;
			for other items, use their outer <a data-link-type="dfn" href="#flex-base-size" id="ref-for-flex-base-size①②">flex base size</a>.
    <li>
      Loop:
     <ol type="a">
      <li> <strong>Check for flexible items.</strong> If all the flex items on the line are frozen,
					free space has been distributed;
					exit this loop.
      <li> <strong>Calculate the <dfn class="dfn-paneled" data-dfn-type="dfn" data-noexport id="remaining-free-space">remaining free space</dfn></strong> as for <a data-link-type="dfn" href="#initial-free-space" id="ref-for-initial-free-space">initial free space</a>, above.
					If the sum of the unfrozen flex items’ flex factors is less than one,
					multiply the <a data-link-type="dfn" href="#initial-free-space" id="ref-for-initial-free-space①">initial free space</a> by this sum.
					If the magnitude of this value is less than the magnitude of the <a data-link-type="dfn" href="#remaining-free-space" id="ref-for-remaining-free-space">remaining free space</a>,
					use this as the <a data-link-type="dfn" href="#remaining-free-space" id="ref-for-remaining-free-space①">remaining free space</a>.
      <li>
        <strong>Distribute free space proportional to the flex factors.</strong>
       <dl>
        <dt>If the <a data-link-type="dfn" href="#remaining-free-space" id="ref-for-remaining-free-space②">remaining free space</a> is zero
        <dd> Do nothing.
        <dt>If using the <a data-link-type="dfn" href="#flex-flex-grow-factor" id="ref-for-flex-flex-grow-factor④">flex grow factor</a>
        <dd> Find the ratio of the item’s flex grow factor
							to the sum of the flex grow factors of all unfrozen items on the line.
							Set the item’s <a data-link-type="dfn" href="#target-main-size" id="ref-for-target-main-size①">target main size</a> to its <a data-link-type="dfn" href="#flex-base-size" id="ref-for-flex-base-size①③">flex base size</a> plus a fraction of the <a data-link-type="dfn" href="#remaining-free-space" id="ref-for-remaining-free-space③">remaining free space</a> proportional to the ratio.
        <dt>If using the <a data-link-type="dfn" href="#flex-flex-shrink-factor" id="ref-for-flex-flex-shrink-factor⑤">flex shrink factor</a>
        <dd> For every unfrozen item on the line,
							multiply its flex shrink factor by its inner <a data-link-type="dfn" href="#flex-base-size" id="ref-for-flex-base-size①④">flex base size</a>,
							and note this as its <dfn class="dfn-paneled" data-dfn-type="dfn" data-noexport id="scaled-flex-shrink-factor">scaled flex shrink factor</dfn>.
							Find the ratio of the item’s <a data-link-type="dfn" href="#scaled-flex-shrink-factor" id="ref-for-scaled-flex-shrink-factor">scaled flex shrink factor</a> to the sum of the <a data-link-type="dfn" href="#scaled-flex-shrink-factor" id="ref-for-scaled-flex-shrink-factor①">scaled flex shrink factors</a> of all unfrozen items on the line.
							Set the item’s <a data-link-type="dfn" href="#target-main-size" id="ref-for-target-main-size②">target main size</a> to its <a data-link-type="dfn" href="#flex-base-size" id="ref-for-flex-base-size①⑤">flex base size</a> minus a fraction of the absolute value of the <a data-link-type="dfn" href="#remaining-free-space" id="ref-for-remaining-free-space④">remaining free space</a> proportional to the ratio. <span class="note">Note this may result in a negative inner <a data-link-type="dfn" href="#main-size" id="ref-for-main-size②①">main size</a>;
							it will be corrected in the next step.</span>
        <dt>Otherwise
        <dd> Do nothing.
       </dl>
      <li> <strong>Fix min/max violations.</strong> Clamp each non-frozen item’s <a data-link-type="dfn" href="#target-main-size" id="ref-for-target-main-size③">target main size</a> by
					its <a data-link-type="dfn" href="https://www.w3.org/TR/css-cascade-4/#used-value" id="ref-for-used-value①">used</a> min and max <a data-link-type="dfn" href="#main-size" id="ref-for-main-size②②">main sizes</a> and floor its content-box size at zero.
					If the item’s <a data-link-type="dfn" href="#target-main-size" id="ref-for-target-main-size④">target main size</a> was made smaller by this,
					it’s a max violation.
					If the item’s <a data-link-type="dfn" href="#target-main-size" id="ref-for-target-main-size⑤">target main size</a> was made larger by this,
					it’s a min violation.
      <li>
        <strong>Freeze over-flexed items.</strong> The total violation is the sum of the adjustments from the previous step <code>∑(clamped size - unclamped size)</code>.
					If the total violation is:
       <dl>
        <dt>Zero
        <dd> Freeze all items.
        <dt>Positive
        <dd> Freeze all the items with min violations.
        <dt>Negative
        <dd> Freeze all the items with max violations.
       </dl>
      <li> Return to the start of this loop.
     </ol>
    <li> Set each item’s used <a data-link-type="dfn" href="#main-size" id="ref-for-main-size②③">main size</a> to its <a data-link-type="dfn" href="#target-main-size" id="ref-for-target-main-size⑥">target main size</a>.
   </ol>

</details>
<h3 class="heading settled" data-level="9.8" id="definite-sizes"><span class="secno">9.8. </span><span class="content"> Definite and Indefinite Sizes</span><a class="self-link" href="#definite-sizes"></a></h3>
<!--__VALOR_STATUS:definite-sizes__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<p>Although CSS Sizing <a data-link-type="biblio" href="#biblio-css-sizing-3">[CSS-SIZING-3]</a> defines <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#definite" id="ref-for-definite①⑤">definite</a> and <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#indefinite" id="ref-for-indefinite">indefinite</a> lengths,
	Flexbox has several additional cases where a length can be considered <dfn class="dfn-paneled" data-dfn-type="dfn" data-lt="definite|definite size|indefinite|indefinite size" data-noexport id="definite">definite</dfn>:</p>
   <ol>
    <li> If a <a data-link-type="dfn" href="#single-line-flex-container" id="ref-for-single-line-flex-container⑨">single-line</a> <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container④⑨">flex container</a> has a definite <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size①⑧">cross size</a>,
			the outer <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size①⑨">cross size</a> of any <a data-link-type="dfn" href="#stretched" id="ref-for-stretched">stretched</a> <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①⓪①">flex items</a> is the flex container’s inner <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size②⓪">cross size</a> (clamped to the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①⓪②">flex item</a>’s min and max <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size②①">cross size</a>)
			and is considered <a data-link-type="dfn" href="#definite" id="ref-for-definite①⑥">definite</a>.
    <li> If the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑤⓪">flex container</a> has a <a data-link-type="dfn" href="#definite" id="ref-for-definite①⑦">definite</a> <a data-link-type="dfn" href="#main-size" id="ref-for-main-size②④">main size</a>,
			a <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①⓪③">flex item</a>’s post-flexing <a data-link-type="dfn" href="#main-size" id="ref-for-main-size②⑤">main size</a> is treated as <a data-link-type="dfn" href="#definite" id="ref-for-definite①⑧">definite</a>,
			even though it can rely on the <a data-link-type="dfn" href="#definite" id="ref-for-definite①⑨">indefinite</a> sizes
			of any flex items in the same line.
    <li> Once the cross size of a flex line has been determined,
			items in auto-sized flex containers are also considered
			definite for the purpose of layout; see <a href="#algo-stretch">step 11</a>.
   </ol>
   <p class="note" role="note"><span>Note:</span> The main size of a <a data-link-type="dfn" href="#fully-inflexible" id="ref-for-fully-inflexible①">fully inflexible</a> item with a <a data-link-type="dfn" href="#definite" id="ref-for-definite②⓪">definite</a> <a data-link-type="dfn" href="#flex-flex-basis" id="ref-for-flex-flex-basis⑧">flex basis</a> is, by definition, <a data-link-type="dfn" href="#definite" id="ref-for-definite②①">definite</a>.</p>

</details>
<h3 class="heading settled" data-level="9.9" id="intrinsic-sizes"><span class="secno">9.9. </span><span class="content"> Intrinsic Sizes</span><a class="self-link" href="#intrinsic-sizes"></a></h3>
<!--__VALOR_STATUS:intrinsic-sizes__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<p>The <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#intrinsic-sizing" id="ref-for-intrinsic-sizing">intrinsic sizing</a> of a <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑤①">flex container</a> is used
	to produce various types of content-based automatic sizing,
	such as shrink-to-fit logical widths (which use the <span class="css">fit-content</span> formula)
	and content-based logical heights (which use the <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#max-content" id="ref-for-max-content①">max-content size</a>).</p>
   <p>See <a data-link-type="biblio" href="#biblio-css-sizing-3">[CSS-SIZING-3]</a> for a definition of the terms in this section.</p>
   <h4 class="heading settled" data-level="9.9.1" id="intrinsic-main-sizes"><span class="secno">9.9.1. </span><span class="content"> Flex Container Intrinsic Main Sizes</span><a class="self-link" href="#intrinsic-main-sizes"></a></h4>
   <p>The <strong><a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#max-content" id="ref-for-max-content②">max-content</a> <a data-link-type="dfn" href="#main-size" id="ref-for-main-size②⑥">main size</a> of a <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑤②">flex container</a></strong> is the smallest size the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑤③">flex container</a> can take
	while maintaining the <a href="#intrinsic-item-contributions">max-content contributions</a> of its <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①⓪④">flex items</a>,
	insofar as allowed by the items’ own flexibility:</p>
   <ol>
    <li> For each <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①⓪⑤">flex item</a>,
			subtract its outer <a data-link-type="dfn" href="#flex-base-size" id="ref-for-flex-base-size①⑥">flex base size</a> from its <a href="#intrinsic-item-contributions">max-content contribution</a> size.
			If that result is positive,
			divide by its <a data-link-type="dfn" href="#flex-flex-grow-factor" id="ref-for-flex-flex-grow-factor⑤">flex grow factor</a> floored at 1;
			if negative,
			divide by its <a data-link-type="dfn" href="#scaled-flex-shrink-factor" id="ref-for-scaled-flex-shrink-factor②">scaled flex shrink factor</a> having floored the <a data-link-type="dfn" href="#flex-flex-shrink-factor" id="ref-for-flex-flex-shrink-factor⑥">flex shrink factor</a> at 1.
			This is the item’s <var>max-content flex fraction</var>.
    <li> Place all <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①⓪⑥">flex items</a> into lines of infinite length.
    <li> Within each line,
			find the largest <var>max-content flex fraction</var> among all the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①⓪⑦">flex items</a>.
			Add each item’s <a data-link-type="dfn" href="#flex-base-size" id="ref-for-flex-base-size①⑦">flex base size</a> to the product of its <a data-link-type="dfn" href="#flex-flex-grow-factor" id="ref-for-flex-flex-grow-factor⑥">flex grow factor</a> (or <a data-link-type="dfn" href="#scaled-flex-shrink-factor" id="ref-for-scaled-flex-shrink-factor③">scaled flex shrink factor</a>, if the chosen <var>max-content flex fraction</var> was negative)
			and the chosen <var>max-content flex fraction</var>,
			then clamp that result by the <a data-link-type="dfn" href="#max-main-size" id="ref-for-max-main-size">max main size</a> floored by the <a data-link-type="dfn" href="#min-main-size" id="ref-for-min-main-size">min main size</a>.
    <li> The <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑤④">flex container</a>’s <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#max-content" id="ref-for-max-content③">max-content size</a> is the
			largest sum of the afore-calculated sizes of all items within a single line.
   </ol>
   <p>The <strong><a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#min-content" id="ref-for-min-content②">min-content</a> <a data-link-type="dfn" href="#main-size" id="ref-for-main-size②⑦">main size</a></strong> of a <em><a data-link-type="dfn" href="#single-line-flex-container" id="ref-for-single-line-flex-container①⓪">single-line</a></em> flex container
	is calculated identically to the <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#max-content" id="ref-for-max-content④">max-content</a> <a data-link-type="dfn" href="#main-size" id="ref-for-main-size②⑧">main size</a>,
	except that the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①⓪⑧">flex item’s</a> <a href="#intrinsic-item-contributions">min-content contribution</a> is used
	instead of its <a href="#intrinsic-item-contributions">max-content contribution</a>.
	However, for a <em><a data-link-type="dfn" href="#multi-line-flex-container" id="ref-for-multi-line-flex-container⑧">multi-line</a></em> container,
	it is simply the largest <a href="#intrinsic-item-contributions">min-content contribution</a> of all the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①⓪⑨">flex items</a> in the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑤⑤">flex container</a>.</p>
   <details class="note">
    <summary>Implications of this algorithm when the sum of flex is less than 1</summary>
    <p>The above algorithm is designed to give the correct behavior for two cases in particular,
		and make the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑤⑥">flex container’s</a> size continuous as you transition between the two:</p>
    <ol>
     <li data-md>
      <p>If all items are inflexible,
the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑤⑦">flex container</a> is sized to the sum of their <a data-link-type="dfn" href="#flex-base-size" id="ref-for-flex-base-size①⑧">flex base size</a>.
(An inflexible <a data-link-type="dfn" href="#flex-base-size" id="ref-for-flex-base-size①⑨">flex base size</a> basically substitutes for a <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-width" id="ref-for-propdef-width①⑤">width</a>/<a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-height" id="ref-for-propdef-height①⓪">height</a>,
which, when specified, is what a <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#max-content-contribution" id="ref-for-max-content-contribution">max-content contribution</a> is based on in Block Layout.)</p>
     <li data-md>
      <p>When all items are flexible with <a data-link-type="dfn" href="#flex-factor" id="ref-for-flex-factor">flex factors</a> ≥ 1,
the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑤⑧">flex container</a> is sized to the sum of the <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#max-content-contribution" id="ref-for-max-content-contribution①">max-content contributions</a> of its items
(or perhaps a slightly larger size,
so that every flex item is <em>at least</em> the size of its <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#max-content-contribution" id="ref-for-max-content-contribution②">max-content contribution</a>,
but also has the correct ratio of its size to the size of the other items,
as determined by its flexibility).</p>
    </ol>
    <p>For example, if a <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑤⑨">flex container</a> has a single <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①①⓪">flex item</a> with <a class="css" data-link-type="propdesc" href="#propdef-flex-basis" id="ref-for-propdef-flex-basis①⑥">flex-basis: 100px;</a> but a max-content size of <span class="css">200px</span>,
		then when the item is <a class="css" data-link-type="propdesc" href="#propdef-flex-grow" id="ref-for-propdef-flex-grow①⑤">flex-grow: 0</a>, the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑥⓪">flex container</a> (and <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①①①">flex item</a>) is <span class="css">100px</span> wide,
		but when the item is <a class="css" data-link-type="propdesc" href="#propdef-flex-grow" id="ref-for-propdef-flex-grow①⑥">flex-grow: 1</a> or higher, the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑥①">flex container</a> (and flex item) is <span class="css">200px</span> wide.</p>
    <p>There are several possible ways to make the overall behavior continuous between these two cases,
		particularly when the sum of flexibilities on a line is between 0 and 1,
		but all of them have drawbacks.
		We chose one we feel has the least bad implications;
		unfortunately, it "double-applies" the flexibility when the sum of the flexibilities is less than 1.
		In the above example, if the item has <a class="css" data-link-type="propdesc" href="#propdef-flex-grow" id="ref-for-propdef-flex-grow①⑦">flex-grow: .5</a>,
		then the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑥②">flex container</a> ends up <span class="css">150px</span> wide,
		but the item then sizes normally into that available space,
		ending up <span class="css">125px</span> wide.</p>
   </details>
   <h4 class="heading settled" data-level="9.9.2" id="intrinsic-cross-sizes"><span class="secno">9.9.2. </span><span class="content"> Flex Container Intrinsic Cross Sizes</span><a class="self-link" href="#intrinsic-cross-sizes"></a></h4>
   <p>The <strong><a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#min-content" id="ref-for-min-content③">min-content</a>/<a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#max-content" id="ref-for-max-content⑤">max-content</a> <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size②②">cross size</a></strong> of a <em><a data-link-type="dfn" href="#single-line-flex-container" id="ref-for-single-line-flex-container①①">single-line</a></em> <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑥③">flex container</a> is the largest <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#min-content-contribution" id="ref-for-min-content-contribution">min-content contribution</a>/<a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#max-content-contribution" id="ref-for-max-content-contribution③">max-content contribution</a> (respectively)
	of its <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①①②">flex items</a>.</p>
   <p>For a <em><a data-link-type="dfn" href="#multi-line-flex-container" id="ref-for-multi-line-flex-container⑨">multi-line</a></em> <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑥④">flex container</a>,
	the <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#min-content" id="ref-for-min-content④">min-content</a>/<a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#max-content" id="ref-for-max-content⑥">max-content</a> <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size②③">cross size</a> is the sum of the flex line cross sizes
	resulting from sizing the flex container under a <a data-link-type="dfn" href="#cross-axis" id="ref-for-cross-axis①⑤">cross-axis</a> <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#min-content-constraint" id="ref-for-min-content-constraint①">min-content constraint</a>/<a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#max-content-constraint" id="ref-for-max-content-constraint①">max-content constraint</a> (respectively).
	However, if the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑥⑤">flex container</a> is <a class="css" data-link-type="propdesc" href="#propdef-flex-flow" id="ref-for-propdef-flex-flow⑥">flex-flow: column wrap;</a>,
	then it’s sized by first finding the largest <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#min-content-contribution" id="ref-for-min-content-contribution①">min-content</a>/<a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#min-content-contribution" id="ref-for-min-content-contribution②">max-content</a> <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size②④">cross-size</a> contribution among the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①①③">flex items</a> (respectively),
	then using that size as the <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#available" id="ref-for-available④">available space</a> in the <a data-link-type="dfn" href="#cross-axis" id="ref-for-cross-axis①⑥">cross axis</a> for each of the <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①①④">flex items</a> during layout.</p>
   <p class="note" role="note"><span>Note:</span> This heuristic for <span class="css">column wrap</span> <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑥⑥">flex containers</a> gives a reasonable approximation of the size that the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑥⑦">flex container</a> should be,
	with each flex item ending up as min(<var>item’s own max-content</var>, <var>maximum min-content among all items</var>),
	and each <a data-link-type="dfn" href="#flex-line" id="ref-for-flex-line⑥">flex line</a> no larger than its largest <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①①⑤">flex item</a>.
	It’s not a <em>perfect</em> fit in some cases,
	but doing it completely correct is insanely expensive,
	and this works reasonably well.</p>
   <h4 class="heading settled" data-level="9.9.3" id="intrinsic-item-contributions"><span class="secno">9.9.3. </span><span class="content"> Flex Item Intrinsic Size Contributions</span><a class="self-link" href="#intrinsic-item-contributions"></a></h4>
   <p>The <strong>main-size <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#min-content-contribution" id="ref-for-min-content-contribution③">min-content contribution</a> of a <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①①⑥">flex item</a></strong> is the larger of its <em>outer</em> <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#min-content" id="ref-for-min-content⑤">min-content size</a> and outer <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#preferred-size-properties" id="ref-for-preferred-size">preferred size</a> (its <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-width" id="ref-for-propdef-width①⑥">width</a>/<a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-height" id="ref-for-propdef-height①①">height</a> as appropriate)
	if that is not <a class="css" data-link-type="maybe" href="https://www.w3.org/TR/css-sizing-3/#valdef-width-auto" id="ref-for-valdef-width-auto⑦">auto</a>,
	clamped by its <a data-link-type="dfn" href="#flex-base-size" id="ref-for-flex-base-size②⓪">flex base size</a> as a maximum (if it is not growable)
	and/or as a minimum (if it is not shrinkable),
	and then further clamped by its <a data-link-type="dfn" href="#min-main-size" id="ref-for-min-main-size①">min</a>/<a data-link-type="dfn" href="#max-main-size" id="ref-for-max-main-size①">max main size</a>.</p>
   <p>The <strong>main-size <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#max-content-contribution" id="ref-for-max-content-contribution④">max-content contribution</a> of a <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①①⑦">flex item</a></strong> is the larger of its <em>outer</em> <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#max-content" id="ref-for-max-content⑦">max-content size</a> and outer <a data-link-type="dfn" href="https://www.w3.org/TR/css-sizing-3/#preferred-size-properties" id="ref-for-preferred-size①">preferred size</a> (its <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-width" id="ref-for-propdef-width①⑦">width</a>/<a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/CSS21/visudet.html#propdef-height" id="ref-for-propdef-height①②">height</a> as appropriate)
	clamped by its <a data-link-type="dfn" href="#flex-base-size" id="ref-for-flex-base-size②①">flex base size</a> as a maximum (if it is not growable)
	and/or as a minimum (if it is not shrinkable),
	and then further clamped by its <a data-link-type="dfn" href="#min-main-size" id="ref-for-min-main-size②">min</a>/<a data-link-type="dfn" href="#max-main-size" id="ref-for-max-main-size②">max main size</a>.</p>

</details>
<h2 class="heading settled" data-level="10" id="pagination"><span class="secno">10. </span><span class="content"> Fragmenting Flex Layout</span><a class="self-link" href="#pagination"></a></h2>
<!--__VALOR_STATUS:pagination__-->

<details class="valor-spec" data-level="2">
  <summary>Show spec text</summary>

<p> Flex containers can break across pages
		between items,
		between lines of items (in <a data-link-type="dfn" href="#multi-line-flex-container" id="ref-for-multi-line-flex-container①⓪">multi-line</a> mode),
		and inside items.
		The <a class="css" data-link-type="property" href="https://www.w3.org/TR/css3-break/#propdef-break-before" id="ref-for-propdef-break-before">break-*</a> properties apply to flex containers as normal for block-level or inline-level boxes.
		This section defines how they apply to flex items
		and the contents of flex items.
		See the <a href="https://www.w3.org/TR/css-break/">CSS Fragmentation Module</a> for more context <a data-link-type="biblio" href="#biblio-css3-break">[CSS3-BREAK]</a>. </p>
   <p> The following breaking rules refer to the <a data-link-type="dfn" href="https://www.w3.org/TR/css3-break/#fragmentation-container" id="ref-for-fragmentation-container">fragmentation container</a> as the “page”.
		The same rules apply in any other <a data-link-type="dfn" href="https://www.w3.org/TR/css3-break/#fragmentation-context" id="ref-for-fragmentation-context">fragmentation context</a>.
		(Substitute “page” with the appropriate <a data-link-type="dfn" href="https://www.w3.org/TR/css3-break/#fragmentation-container" id="ref-for-fragmentation-container①">fragmentation container</a> type as needed.)
		For readability, in this section the terms "row" and "column" refer to the relative orientation
		of the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑥⑧">flex container</a> with respect to the block flow direction of the <a data-link-type="dfn" href="https://www.w3.org/TR/css3-break/#fragmentation-context" id="ref-for-fragmentation-context①">fragmentation context</a>,
		rather than to that of the <a data-link-type="dfn" href="#flex-container" id="ref-for-flex-container⑥⑨">flex container</a> itself. </p>
   <p> The exact layout of a fragmented flex container
		is not defined in this level of Flexible Box Layout.
		However, breaks inside a flex container are subject to the following rules
		(interpreted using <a data-link-type="dfn" href="#order-modified-document-order" id="ref-for-order-modified-document-order①">order-modified document order</a>): </p>
   <ul>
    <li>
      In a row flex container,
			the <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css3-break/#propdef-break-before" id="ref-for-propdef-break-before①">break-before</a> and <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css3-break/#propdef-break-after" id="ref-for-propdef-break-after">break-after</a> values on flex items
			are propagated to the flex line.
			The <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css3-break/#propdef-break-before" id="ref-for-propdef-break-before②">break-before</a> values on the first line
			and the <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css3-break/#propdef-break-after" id="ref-for-propdef-break-after①">break-after</a> values on the last line
			are propagated to the flex container.
     <p class="note" role="note"><span>Note:</span> Break propagation
			(like <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css-text-decor-3/#propdef-text-decoration" id="ref-for-propdef-text-decoration">text-decoration</a> propagation)
			does not affect <a data-link-type="dfn" href="https://www.w3.org/TR/css-cascade-4/#computed-value" id="ref-for-computed-value">computed values</a>.</p>
    <li> In a column flex container,
			the <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css3-break/#propdef-break-before" id="ref-for-propdef-break-before③">break-before</a> values on the first item
			and the <a class="property" data-link-type="propdesc" href="https://www.w3.org/TR/css3-break/#propdef-break-after" id="ref-for-propdef-break-after②">break-after</a> values on the last item
			are propagated to the flex container.
			Forced breaks on other items are applied to the item itself.
    <li> A forced break inside a flex item effectively increases the size of its contents;
			it does not trigger a forced break inside sibling items.
    <li> In a row flex container, <a href="https://www.w3.org/TR/css3-break/#btw-blocks">Class A break opportunities</a> occur between sibling flex lines,
			and <a href="https://www.w3.org/TR/css3-break/#end-block">Class C break opportunities</a> occur between the first/last flex line and the flex container’s content edges.
			In a column flex container, <a href="https://www.w3.org/TR/css3-break/#btw-blocks">Class A break opportunities</a> occur between sibling flex items,
			and <a href="https://www.w3.org/TR/css3-break/#end-block">Class C break opportunities</a> occur between the first/last flex items on a line and the flex container’s content edges. <a data-link-type="biblio" href="#biblio-css3-break">[CSS3-BREAK]</a>
    <li> When a flex container is continued after a break,
			the space available to its <a data-link-type="dfn" href="#flex-item" id="ref-for-flex-item①①⑧">flex items</a> (in the block flow direction of the fragmentation context)
			is reduced by the space consumed by flex container fragments on previous pages.
			The space consumed by a flex container fragment is
			the size of its content box on that page.
			If as a result of this adjustment the available space becomes negative,
			it is set to zero.
    <li> If the first fragment of the flex container is not at the top of the page,
			and none of its flex items fit in the remaining space on the page,
			the entire fragment is moved to the next page.
    <li> When a <a data-link-type="dfn" href="#multi-line-flex-container" id="ref-for-multi-line-flex-container①①">multi-line</a> column flex container breaks,
			each fragment has its own "stack" of flex lines,
			just like each fragment of a multi-column container
			has its own row of column boxes.
    <li> Aside from the rearrangement of items imposed by the previous point,
			UAs should attempt to minimize distortion of the flex container
			with respect to unfragmented flow.
   </ul>

</details>
<h3 class="heading settled" data-level="10.1" id="pagination-algo"><span class="secno">10.1. </span><span class="content"> Sample Flex Fragmentation Algorithm</span><a class="self-link" href="#pagination-algo"></a></h3>
<!--__VALOR_STATUS:pagination-algo__-->

<details class="valor-spec" data-level="3">
  <summary>Show spec text</summary>

<p> This informative section presents a possible fragmentation algorithm for flex containers.
		Implementors are encouraged to improve on this algorithm and <a href="#status">provide feedback to the CSS Working Group</a>. </p>
   <div class="example" id="example-c4b58c6e">
    <a class="self-link" href="#example-c4b58c6e"></a>
    <p class="note" role="note"> This algorithm assumes that pagination always proceeds only in the forward direction;
		therefore, in the algorithms below, alignment is mostly ignored prior to pagination.
		Advanced layout engines may be able to honor alignment across fragments. </p>
    <dl>
     <dt><a data-link-type="dfn" href="#single-line-flex-container" id="ref-for-single-line-flex-container①②">single-line</a> column flex container
     <dd>
      <ol>
       <li> Run the flex layout algorithm (without regards to pagination)
					through <a href="#cross-sizing">Cross Sizing Determination</a>.
       <li> Lay out as many consecutive flex items or item fragments as possible
					(but at least one or a fragment thereof),
					starting from the first,
					until there is no more room on the page
					or a forced break is encountered.
       <li> If the previous step ran out of room
					and the free space is positive,
					the UA may reduce the distributed free space on this page
					(down to, but not past, zero)
					in order to make room for the next unbreakable flex item or fragment.
					Otherwise,
					the item or fragment that does not fit is pushed to the next page.
					The UA should pull up if more than 50% of the fragment would have fit in the remaining space
					and should push otherwise.
       <li> If there are any flex items or fragments not laid out by the previous steps,
					rerun the flex layout algorithm
					from <a href="#line-sizing">Line Length Determination</a> through <a href="#cross-sizing">Cross Sizing Determination</a> with the next page’s size
					and <em>all</em> the contents (including those already laid out),
					and return to the previous step,
					but starting from the first item or fragment not already laid out.
       <li> For each fragment of the flex container,
					continue the flex layout algorithm
					from <a href="#main-alignment">Main-Axis Alignment</a> to its finish.
      </ol>
      <p class="note" role="note"> It is the intent of this algorithm that column-direction <a data-link-type="dfn" href="#single-line-flex-container" id="ref-for-single-line-flex-container①③">single-line</a> flex containers
				paginate very similarly to block flow.
				As a test of the intent,
				a flex container with <span class="css">justify-content:start</span> and no flexible items
				should paginate identically to
				a block with in-flow children with same content,
				same used size and same used margins. </p>
     <dt><a data-link-type="dfn" href="#multi-line-flex-container" id="ref-for-multi-line-flex-container①②">multi-line</a> column flex container
     <dd>
      <ol>
       <li> Run the flex layout algorithm <em>with</em> regards to pagination
					(limiting the flex container’s maximum line length to the space left on the page)
					through <a href="#cross-sizing">Cross Sizing Determination</a>.
       <li>
         Lay out as many flex lines as possible
					(but at least one)
					until there is no more room in the flex container
					in the cross dimension
					or a forced break is encountered:
        <ol>
         <li> Lay out as many consecutive flex items as possible
							(but at least one),
							starting from the first,
							until there is no more room on the page
							or a forced break is encountered.
							Forced breaks <em>within</em> flex items are ignored.
         <li> If this is the first flex container fragment,
							this line contains only a single flex item
							that is larger than the space left on the page,
							and the flex container is not at the top of the page already,
							move the flex container to the next page
							and restart flex container layout entirely.
         <li> If there are any flex items not laid out by the first step,
							rerun the flex layout algorithm
							from <a href="#main-sizing">Main Sizing Determination</a> through <a href="#cross-sizing">Cross Sizing Determination</a> using only the items not laid out on a previous line,
							and return to the previous step,
							starting from the first item not already laid out.
        </ol>
       <li> If there are any flex items not laid out by the previous step,
					rerun the flex layout algorithm
					from <a href="#line-sizing">Line Sizing Determination</a> through <a href="#cross-sizing">Cross Sizing Determination</a> with the next page’s size
					and only the items not already laid out,
					and return to the previous step,
					but starting from the first item not already laid out.
       <li> For each fragment of the flex container,
					continue the flex layout algorithm
					from <a href="#main-alignment">Main-Axis Alignment</a> to its finish.
      </ol>
      <p class="note" role="note"> If a flex item does not entirely fit on a single page,
				it will <em>not</em> be paginated in <a data-link-type="dfn" href="#multi-line-flex-container" id="ref-for-multi-line-flex-container①③">multi-line</a> column flex containers. </p>
     <dt><a data-link-type="dfn" href="#single-line-flex-container" id="ref-for-single-line-flex-container①④">single-line</a> row flex container
     <dd>
      <ol>
       <li> Run the entire flex layout algorithm (without regards to pagination),
					except treat any <a class="property" data-link-type="propdesc" href="#propdef-align-self" id="ref-for-propdef-align-self①⑨">align-self</a> other than <a class="css" data-link-type="maybe" href="#valdef-align-items-flex-start" id="ref-for-valdef-align-items-flex-start①">flex-start</a> or <a class="css" data-link-type="maybe" href="#valdef-align-items-baseline" id="ref-for-valdef-align-items-baseline①">baseline</a> as <a class="css" data-link-type="maybe" href="#valdef-align-items-flex-start" id="ref-for-valdef-align-items-flex-start②">flex-start</a>.
       <li> If an unbreakable item doesn’t fit within the space left on the page,
					and the flex container is not at the top of the page,
					move the flex container to the next page
					and restart flex container layout entirely.
       <li>
         For each item,
					lay out as much of its contents as will fit in the space left on the page,
					and fragment the remaining content onto the next page,
					rerunning the flex layout algorithm
					from <a href="#line-sizing">Line Length Determination</a> through <a href="#main-alignment">Main-Axis Alignment</a> into the new page size
					using <em>all</em> the contents (including items completed on previous pages).
        <p class="note" role="note"> Any flex items that fit entirely into previous fragments
						still take up space in the main axis in later fragments. </p>
       <li> For each fragment of the flex container,
					rerun the flex layout algorithm
					from <a href="#cross-alignment">Cross-Axis Alignment</a> to its finish.
					For all fragments besides the first,
					treat <a class="property" data-link-type="propdesc" href="#propdef-align-self" id="ref-for-propdef-align-self②⓪">align-self</a> and <a class="property" data-link-type="propdesc" href="#propdef-align-content" id="ref-for-propdef-align-content⑧">align-content</a> as being <a class="css" data-link-type="maybe" href="#valdef-align-items-flex-start" id="ref-for-valdef-align-items-flex-start③">flex-start</a> for all item fragments and lines.
       <li> If any item,
					when aligned according to its original <a class="property" data-link-type="propdesc" href="#propdef-align-self" id="ref-for-propdef-align-self②①">align-self</a> value
					into the combined <a data-link-type="dfn" href="#cross-size" id="ref-for-cross-size②⑤">cross size</a> of all the flex container fragments,
					would fit entirely within a single flex container fragment,
					it may be shifted into that fragment
					and aligned appropriately.
      </ol>
     <dt><a data-link-type="dfn" href="#multi-line-flex-container" id="ref-for-multi-line-flex-container①④">multi-line</a> row flex container
     <dd>
      <ol>
       <li> Run the flex layout algorithm (without regards to pagination),
					through <a href="#cross-sizing">Cross Sizing Determination</a>.
       <li>
         Lay out as many flex lines as possible
					(but at least one),
					starting from the first,
					until there is no more room on the page
					or a forced break is encountered.
        <p> If a line doesn’t fit on the page,
						and the line is not at the top of the page,
						move the line to the next page
						and restart the flex layout algorithm entirely,
						using only the items in and following this line. </p>
        <p> If a flex item itself causes a forced break,
						rerun the flex layout algorithm
						from <a href="#main-sizing">Main Sizing Determination</a> through <a href="#main-alignment">Main-Axis Alignment</a>,
						using only the items on this and following lines,
						but with the item causing the break automatically starting a new line
						in the <a href="#algo-line-break">line breaking step</a>,
						then continue with this step.
						Forced breaks <em>within</em> flex items are ignored. </p>
       <li> If there are any flex items not laid out by the previous step,
					rerun the flex layout algorithm
					from <a href="#line-sizing">Line Length Determination</a> through <a href="#main-alignment">Main-Axis Alignment</a> with the next page’s size
					and only the items not already laid out.
					Return to the previous step,
					but starting from the first line not already laid out.
       <li> For each fragment of the flex container,
					continue the flex layout algorithm
					from <a href="#cross-alignment">Cross Axis Alignment</a> to its finish.
      </ol>
    </dl>
   </div>

</details>
<h2 class="no-num heading settled" id="axis-mapping"><span class="content"> Appendix A: Axis Mappings</span><a class="self-link" href="#axis-mapping"></a></h2>
<!--__VALOR_STATUS:axis-mapping__-->

<details class="valor-spec" data-level="2">
  <summary>Show spec text</summary>

<p><em>This appendix is non-normative.</em></p>
   <table class="data" id="axis-mapping-table-en">
    <caption>Axis Mappings for <a class="css" data-link-type="maybe" href="https://www.w3.org/TR/css-writing-modes-4/#valdef-direction-ltr" id="ref-for-valdef-direction-ltr">ltr</a> + <a class="css" data-link-type="maybe" href="https://www.w3.org/TR/css-writing-modes-4/#valdef-writing-mode-horizontal-tb" id="ref-for-valdef-writing-mode-horizontal-tb">horizontal-tb</a> <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#writing-mode" id="ref-for-writing-mode①⓪">Writing Mode</a> (e.g. English)</caption>
    <colgroup span="1">
    <colgroup span="3">
    <colgroup span="3">
    <thead>
     <tr>
      <th><a class="property" data-link-type="propdesc" href="#propdef-flex-flow" id="ref-for-propdef-flex-flow⑦">flex-flow</a>
      <th><a data-link-type="dfn" href="#main-axis" id="ref-for-main-axis①⑤">main axis</a>
      <th><a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-3/#start" id="ref-for-start">start</a>
      <th><a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-3/#end" id="ref-for-end①">end</a>
      <th><a data-link-type="dfn" href="#cross-axis" id="ref-for-cross-axis①⑦">cross axis</a>
      <th><a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-3/#start" id="ref-for-start①">start</a>
      <th><a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-3/#end" id="ref-for-end②">end</a>
    <tbody>
     <tr>
      <th><a class="css" data-link-type="maybe" href="#valdef-flex-direction-row" id="ref-for-valdef-flex-direction-row③">row</a> + <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-nowrap" id="ref-for-valdef-flex-wrap-nowrap">nowrap</a>/<a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap" id="ref-for-valdef-flex-wrap-wrap②">wrap</a>
      <td rowspan="4">horizontal
      <td>left
      <td>right
      <td rowspan="4">vertical
      <td rowspan="2">top
      <td rowspan="2">bottom
     <tr>
      <th><a class="css" data-link-type="maybe" href="#valdef-flex-direction-row-reverse" id="ref-for-valdef-flex-direction-row-reverse①">row-reverse</a> + <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-nowrap" id="ref-for-valdef-flex-wrap-nowrap①">nowrap</a>/<a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap" id="ref-for-valdef-flex-wrap-wrap③">wrap</a>
      <td>right
      <td>left
     <tr>
      <th><a class="css" data-link-type="maybe" href="#valdef-flex-direction-row" id="ref-for-valdef-flex-direction-row④">row</a> + <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap-reverse" id="ref-for-valdef-flex-wrap-wrap-reverse②">wrap-reverse</a>
      <td>left
      <td>right
      <td rowspan="2">bottom
      <td rowspan="2">top
     <tr>
      <th><a class="css" data-link-type="maybe" href="#valdef-flex-direction-row-reverse" id="ref-for-valdef-flex-direction-row-reverse②">row-reverse</a> + <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap-reverse" id="ref-for-valdef-flex-wrap-wrap-reverse③">wrap-reverse</a>
      <td>right
      <td>left
     <tr>
      <th><a class="css" data-link-type="maybe" href="#valdef-flex-direction-column" id="ref-for-valdef-flex-direction-column①">column</a> + <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-nowrap" id="ref-for-valdef-flex-wrap-nowrap②">nowrap</a>/<a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap" id="ref-for-valdef-flex-wrap-wrap④">wrap</a>
      <td rowspan="4">vertical
      <td>top
      <td>bottom
      <td rowspan="4">horizontal
      <td rowspan="2">left
      <td rowspan="2">right
     <tr>
      <th><a class="css" data-link-type="maybe" href="#valdef-flex-direction-column-reverse" id="ref-for-valdef-flex-direction-column-reverse">column-reverse</a> + <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-nowrap" id="ref-for-valdef-flex-wrap-nowrap③">nowrap</a>/<a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap" id="ref-for-valdef-flex-wrap-wrap⑤">wrap</a>
      <td>bottom
      <td>top
     <tr>
      <th><a class="css" data-link-type="maybe" href="#valdef-flex-direction-column" id="ref-for-valdef-flex-direction-column②">column</a> + <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap-reverse" id="ref-for-valdef-flex-wrap-wrap-reverse④">wrap-reverse</a>
      <td>top
      <td>bottom
      <td rowspan="2">right
      <td rowspan="2">left
     <tr>
      <th><a class="css" data-link-type="maybe" href="#valdef-flex-direction-column-reverse" id="ref-for-valdef-flex-direction-column-reverse①">column-reverse</a> + <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap-reverse" id="ref-for-valdef-flex-wrap-wrap-reverse⑤">wrap-reverse</a>
      <td>bottom
      <td>top
   </table>
   <table class="data" id="axis-mapping-table-fa">
    <caption>Axis Mappings for <a class="css" data-link-type="maybe" href="https://www.w3.org/TR/css-writing-modes-4/#valdef-direction-rtl" id="ref-for-valdef-direction-rtl">rtl</a> + <a class="css" data-link-type="maybe" href="https://www.w3.org/TR/css-writing-modes-4/#valdef-writing-mode-horizontal-tb" id="ref-for-valdef-writing-mode-horizontal-tb①">horizontal-tb</a> <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#writing-mode" id="ref-for-writing-mode①①">Writing Mode</a> (e.g. Farsi)</caption>
    <colgroup span="1">
    <colgroup span="3">
    <colgroup span="3">
    <thead>
     <tr>
      <th><a class="property" data-link-type="propdesc" href="#propdef-flex-flow" id="ref-for-propdef-flex-flow⑧">flex-flow</a>
      <th><a data-link-type="dfn" href="#main-axis" id="ref-for-main-axis①⑥">main axis</a>
      <th><a data-link-type="dfn" href="#main-start" id="ref-for-main-start⑨">main-start</a>
      <th><a data-link-type="dfn" href="#main-end" id="ref-for-main-end⑨">main-end</a>
      <th><a data-link-type="dfn" href="#cross-axis" id="ref-for-cross-axis①⑧">cross axis</a>
      <th><a data-link-type="dfn" href="#cross-start" id="ref-for-cross-start①④">cross-start</a>
      <th><a data-link-type="dfn" href="#cross-end" id="ref-for-cross-end⑨">cross-end</a>
    <tbody>
     <tr>
      <th><a class="css" data-link-type="maybe" href="#valdef-flex-direction-row" id="ref-for-valdef-flex-direction-row⑤">row</a> + <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-nowrap" id="ref-for-valdef-flex-wrap-nowrap④">nowrap</a>/<a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap" id="ref-for-valdef-flex-wrap-wrap⑥">wrap</a>
      <td rowspan="4">horizontal
      <td>right
      <td>left
      <td rowspan="4">vertical
      <td rowspan="2">top
      <td rowspan="2">bottom
     <tr>
      <th><a class="css" data-link-type="maybe" href="#valdef-flex-direction-row-reverse" id="ref-for-valdef-flex-direction-row-reverse③">row-reverse</a> + <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-nowrap" id="ref-for-valdef-flex-wrap-nowrap⑤">nowrap</a>/<a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap" id="ref-for-valdef-flex-wrap-wrap⑦">wrap</a>
      <td>left
      <td>right
     <tr>
      <th><a class="css" data-link-type="maybe" href="#valdef-flex-direction-row" id="ref-for-valdef-flex-direction-row⑥">row</a> + <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap-reverse" id="ref-for-valdef-flex-wrap-wrap-reverse⑥">wrap-reverse</a>
      <td>right
      <td>left
      <td rowspan="2">bottom
      <td rowspan="2">top
     <tr>
      <th><a class="css" data-link-type="maybe" href="#valdef-flex-direction-row-reverse" id="ref-for-valdef-flex-direction-row-reverse④">row-reverse</a> + <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap-reverse" id="ref-for-valdef-flex-wrap-wrap-reverse⑦">wrap-reverse</a>
      <td>left
      <td>right
     <tr>
      <th><a class="css" data-link-type="maybe" href="#valdef-flex-direction-column" id="ref-for-valdef-flex-direction-column③">column</a> + <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-nowrap" id="ref-for-valdef-flex-wrap-nowrap⑥">nowrap</a>/<a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap" id="ref-for-valdef-flex-wrap-wrap⑧">wrap</a>
      <td rowspan="4">vertical
      <td>top
      <td>bottom
      <td rowspan="4">horizontal
      <td rowspan="2">right
      <td rowspan="2">left
     <tr>
      <th><a class="css" data-link-type="maybe" href="#valdef-flex-direction-column-reverse" id="ref-for-valdef-flex-direction-column-reverse②">column-reverse</a> + <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-nowrap" id="ref-for-valdef-flex-wrap-nowrap⑦">nowrap</a>/<a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap" id="ref-for-valdef-flex-wrap-wrap⑨">wrap</a>
      <td>bottom
      <td>top
     <tr>
      <th><a class="css" data-link-type="maybe" href="#valdef-flex-direction-column" id="ref-for-valdef-flex-direction-column④">column</a> + <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap-reverse" id="ref-for-valdef-flex-wrap-wrap-reverse⑧">wrap-reverse</a>
      <td>top
      <td>bottom
      <td rowspan="2">left
      <td rowspan="2">right
     <tr>
      <th><a class="css" data-link-type="maybe" href="#valdef-flex-direction-column-reverse" id="ref-for-valdef-flex-direction-column-reverse③">column-reverse</a> + <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap-reverse" id="ref-for-valdef-flex-wrap-wrap-reverse⑨">wrap-reverse</a>
      <td>bottom
      <td>top
   </table>
   <table class="data" id="axis-mapping-table-ja">
    <caption>Axis Mappings for <a class="css" data-link-type="maybe" href="https://www.w3.org/TR/css-writing-modes-4/#valdef-direction-ltr" id="ref-for-valdef-direction-ltr①">ltr</a> + <a class="css" data-link-type="maybe" href="https://www.w3.org/TR/css-writing-modes-4/#valdef-writing-mode-vertical-rl" id="ref-for-valdef-writing-mode-vertical-rl">vertical-rl</a> <a data-link-type="dfn" href="https://www.w3.org/TR/css-writing-modes-4/#writing-mode" id="ref-for-writing-mode①②">Writing Mode</a> (e.g. Japanese)</caption>
    <colgroup span="1">
    <colgroup span="3">
    <colgroup span="3">
    <thead>
     <tr>
      <th><a class="property" data-link-type="propdesc" href="#propdef-flex-flow" id="ref-for-propdef-flex-flow⑨">flex-flow</a>
      <th><a data-link-type="dfn" href="#main-axis" id="ref-for-main-axis①⑦">main axis</a>
      <th><a data-link-type="dfn" href="#main-start" id="ref-for-main-start①⓪"><abbr title="cross-start">start</abbr></a>
      <th><a data-link-type="dfn" href="#main-end" id="ref-for-main-end①⓪"><abbr title="cross-end">end</abbr></a>
      <th><a data-link-type="dfn" href="#cross-axis" id="ref-for-cross-axis①⑨">cross axis</a>
      <th><a data-link-type="dfn" href="#cross-start" id="ref-for-cross-start①⑤"><abbr title="cross-start">start</abbr></a>
      <th><a data-link-type="dfn" href="#cross-end" id="ref-for-cross-end①⓪"><abbr title="cross-end">end</abbr></a>
    <tbody>
     <tr>
      <th><a class="css" data-link-type="maybe" href="#valdef-flex-direction-row" id="ref-for-valdef-flex-direction-row⑦">row</a> + <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-nowrap" id="ref-for-valdef-flex-wrap-nowrap⑧">nowrap</a>/<a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap" id="ref-for-valdef-flex-wrap-wrap①⓪">wrap</a>
      <td rowspan="4">vertical
      <td>top
      <td>bottom
      <td rowspan="4">horizontal
      <td rowspan="2">right
      <td rowspan="2">left
     <tr>
      <th><a class="css" data-link-type="maybe" href="#valdef-flex-direction-row-reverse" id="ref-for-valdef-flex-direction-row-reverse⑤">row-reverse</a> + <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-nowrap" id="ref-for-valdef-flex-wrap-nowrap⑨">nowrap</a>/<a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap" id="ref-for-valdef-flex-wrap-wrap①①">wrap</a>
      <td>bottom
      <td>top
     <tr>
      <th><a class="css" data-link-type="maybe" href="#valdef-flex-direction-row" id="ref-for-valdef-flex-direction-row⑧">row</a> + <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap-reverse" id="ref-for-valdef-flex-wrap-wrap-reverse①⓪">wrap-reverse</a>
      <td>top
      <td>bottom
      <td rowspan="2">left
      <td rowspan="2">right
     <tr>
      <th><a class="css" data-link-type="maybe" href="#valdef-flex-direction-row-reverse" id="ref-for-valdef-flex-direction-row-reverse⑥">row-reverse</a> + <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap-reverse" id="ref-for-valdef-flex-wrap-wrap-reverse①①">wrap-reverse</a>
      <td>bottom
      <td>top
     <tr>
      <th><a class="css" data-link-type="maybe" href="#valdef-flex-direction-column" id="ref-for-valdef-flex-direction-column⑤">column</a> + <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-nowrap" id="ref-for-valdef-flex-wrap-nowrap①⓪">nowrap</a>/<a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap" id="ref-for-valdef-flex-wrap-wrap①②">wrap</a>
      <td rowspan="4">vertical
      <td>right
      <td>left
      <td rowspan="4">horizonal
      <td rowspan="2">top
      <td rowspan="2">bottom
     <tr>
      <th><a class="css" data-link-type="maybe" href="#valdef-flex-direction-column-reverse" id="ref-for-valdef-flex-direction-column-reverse④">column-reverse</a> + <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-nowrap" id="ref-for-valdef-flex-wrap-nowrap①①">nowrap</a>/<a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap" id="ref-for-valdef-flex-wrap-wrap①③">wrap</a>
      <td>left
      <td>right
     <tr>
      <th><a class="css" data-link-type="maybe" href="#valdef-flex-direction-column" id="ref-for-valdef-flex-direction-column⑥">column</a> + <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap-reverse" id="ref-for-valdef-flex-wrap-wrap-reverse①②">wrap-reverse</a>
      <td>right
      <td>left
      <td rowspan="2">bottom
      <td rowspan="2">top
     <tr>
      <th><a class="css" data-link-type="maybe" href="#valdef-flex-direction-column-reverse" id="ref-for-valdef-flex-direction-column-reverse⑤">column-reverse</a> + <a class="css" data-link-type="maybe" href="#valdef-flex-wrap-wrap-reverse" id="ref-for-valdef-flex-wrap-wrap-reverse①③">wrap-reverse</a>
      <td>left
      <td>right
   </table>

</details>
<!-- END VERBATIM SPEC: DO NOT EDIT ABOVE. -->
